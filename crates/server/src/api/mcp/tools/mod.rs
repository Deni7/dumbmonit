//! Les outils que le serveur MCP expose à l'assistant.
//!
//! Chaque outil est une façade : il traduit des arguments JSON en appels aux
//! fonctions que l'interface web utilise déjà (`api::targets`, `api::alerts`,
//! `api::metrics`, `db::*`), puis rend un texte Markdown court — ce que le modèle
//! lit — doublé, quand c'est utile, d'un `structuredContent` JSON.
//!
//! Le vocabulaire est celui de l'interface : *reporting / unreachable / waiting*
//! pour un équipement, *info / advisory / warning* pour une alerte (la sévérité
//! interne `warning` s'affiche « advisory », `critical` s'affiche « warning »).

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use dumbmonit_proto::{Target, TargetId};
use serde_json::{Value, json};

use crate::alerting::model::Severity;
use crate::alerting::silence::Schedule;
use crate::api::alerts::{
    self, AckPayload, ActiveAlertView, EnablePayload, HistoryQuery, SilencePayload,
};
use crate::api::metrics::{self, RangeQuery};
use crate::api::{
    ApiError, agent_commands, channels, collectors, discovery, packs, status_pages, targets,
};
use crate::auth::middleware::{AdminIdentity, CurrentPrincipal, Principal};
use crate::auth::token::{ApiToken, Scope};
use crate::collectors::push::{self, Verdict};
use crate::db;
use crate::state::AppState;

mod alert_tools;
mod devices;
mod helpers;
mod inventory;
mod notify;
mod overview;
mod pages;
mod rules;
mod series;
mod silences;
mod spec;
mod spec_read;
mod spec_write;
#[cfg(test)]
mod tests;

use alert_tools::*;
use devices::*;
use helpers::*;
use inventory::*;
use notify::*;
pub use overview::dashboard_summary;
use overview::*;
use pages::*;
use rules::*;
use series::*;
use silences::*;
use spec::*;
pub use spec::{ToolSpec, catalogue, find};
use spec_read::*;
use spec_write::*;

/// Ce que l'assistant lit à l'initialisation, avant tout appel d'outil.
pub const INSTRUCTIONS: &str = "DumbMonit monitors a homelab or a small fleet: devices \
(SNMP, Proxmox, Synology, agents…) and services (HTTP, TCP, DNS, ping, TLS checks, \
heartbeats). Start with get_status to answer \"is everything fine?\". Device states are \
reporting, unreachable, waiting (no data yet), disabled, down (service check failing). \
Alert severities, from mild to serious, are info, advisory and warning; an alert \
\"building up\" is not firing yet; \"suppressed by parent\" means the device's parent is \
unreachable, so the alert is expected; \"acknowledged\" means someone knows and reminders \
are paused; a \"dismissed\" alert (\"got it\") is hidden from the active list and silent until \
it resolves. Tools marked read-only never change anything; the others (silence_device, \
schedule_maintenance, remove_silence, acknowledge_alert, dismiss_alert, probe_device, \
set_device_enabled, \
set_rule_enabled, add_device, discover_network, restart_container, test_channel, \
post_incident) need a token with the write scope, and some act outside the instance \
(test_channel sends a real message, post_incident notifies status page subscribers): \
confirm with the user before calling them. Before add_device, call list_device_types \
with the kind to learn the address, credential and options it expects. Devices can be \
named by id or by name. Times are UTC, RFC 3339.";

/// Types de cibles qui surveillent un *service* : leur état vient du résultat de
/// la sonde (`dumbmonit_probe_success`), pas de la dernière interrogation.
const UPTIME_KINDS: [&str; 7] = ["http", "tcp", "dns", "ping", "tls", "push", "provider"];

/// Fenêtre de fraîcheur d'un résultat de sonde, alignée sur l'interface.
const PROBE_WINDOW: &str = "10m";

/// Nombre maximal de points renvoyés par série par `query_metrics` : un modèle
/// n'a que faire de deux mille points, et ils coûteraient autant de jetons.
const MAX_POINTS: usize = 60;

/// Plafonds de `query_metrics` et `alert_history`.
const MAX_RANGE_HOURS: f64 = 24.0 * 31.0;
const MAX_HISTORY_LIMIT: i64 = 500;
const DEFAULT_HISTORY_LIMIT: i64 = 50;

/// Durée maximale d'un silence posé par un assistant : au-delà d'une semaine,
/// c'est une règle à désactiver, pas une maintenance.
const MAX_SILENCE_HOURS: f64 = 24.0 * 7.0;

/// Durée d'un acquittement posé par un assistant quand il ne précise rien.
const DEFAULT_ACK_HOURS: f64 = 4.0;

pub struct ToolOutput {
    pub text: String,
    pub structured: Option<Value>,
}

impl ToolOutput {
    fn new(text: String, structured: Value) -> Self {
        Self { text, structured: Some(structured) }
    }
}

pub enum ToolError {
    /// L'outil n'a pas pu faire ce qu'on lui demandait ; le texte est pour
    /// l'assistant, qui le reformulera.
    Failed(String),
    /// Panne interne : journalisée, jamais détaillée au client.
    Internal(anyhow::Error),
}

impl From<ApiError> for ToolError {
    fn from(error: ApiError) -> Self {
        match error {
            ApiError::Internal(error) => Self::Internal(error),
            other => Self::Failed(other.into_parts().1),
        }
    }
}

impl From<anyhow::Error> for ToolError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

type ToolResult = Result<ToolOutput, ToolError>;

fn failed(message: impl Into<String>) -> ToolError {
    ToolError::Failed(message.into())
}

/// Lecture tolérante des arguments : un modèle envoie volontiers `"3"` pour 3.
struct Args(Value);

impl Args {
    fn str(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())
    }

    fn i64(&self, key: &str) -> Option<i64> {
        let value = self.0.get(key)?;
        value.as_i64().or_else(|| value.as_str()?.trim().parse().ok())
    }

    fn f64(&self, key: &str) -> Option<f64> {
        let value = self.0.get(key)?;
        value.as_f64().or_else(|| value.as_str()?.trim().parse().ok())
    }

    fn bool(&self, key: &str) -> Option<bool> {
        let value = self.0.get(key)?;
        value.as_bool().or_else(|| match value.as_str()?.trim() {
            "true" | "yes" | "on" | "1" => Some(true),
            "false" | "no" | "off" | "0" => Some(false),
            _ => None,
        })
    }

    fn required_bool(&self, key: &str) -> Result<bool, ToolError> {
        self.bool(key).ok_or_else(|| failed(format!("\"{key}\" (true or false) is required.")))
    }

    /// Désignation d'équipement : `device`, `id` ou `name`, au choix de l'appelant.
    fn device_ref(&self) -> Option<String> {
        if let Some(id) = self.i64("id") {
            return Some(id.to_string());
        }
        for key in ["device", "name"] {
            if let Some(text) = self.str(key) {
                return Some(text.to_string());
            }
            if let Some(id) = self.i64(key) {
                return Some(id.to_string());
            }
        }
        None
    }
}

pub async fn call(state: &AppState, token: &ApiToken, name: &str, arguments: Value) -> ToolResult {
    let args = Args(arguments);
    match name {
        "get_status" => get_status(state).await,
        "list_devices" => list_devices(state, &args).await,
        "get_device" => get_device(state, &args).await,
        "list_alerts" => list_alerts(state, &args).await,
        "alert_history" => alert_history(state, &args).await,
        "query_metrics" => query_metrics(state, &args).await,
        "list_silences" => list_silences(state).await,
        "list_rules" => list_rules(state).await,
        "list_device_types" => list_device_types(state, &args).await,
        "list_agents" => list_agents(state).await,
        "list_containers" => list_containers(state, &args).await,
        "list_heartbeats" => list_heartbeats(state).await,
        "list_status_pages" => list_status_pages(state).await,
        "list_channels" => list_channels(state).await,
        "list_packs" => list_packs(state, token).await,
        "silence_device" => silence_device(state, &args).await,
        "schedule_maintenance" => schedule_maintenance(state, &args).await,
        "remove_silence" => remove_silence(state, &args).await,
        "acknowledge_alert" => acknowledge_alert(state, &args).await,
        "dismiss_alert" => dismiss_alert(state, &args).await,
        "probe_device" => probe_device(state, &args).await,
        "set_device_enabled" => set_device_enabled(state, &args).await,
        "set_rule_enabled" => set_rule_enabled(state, &args).await,
        "add_device" => add_device(state, &args).await,
        "discover_network" => discover_network(token, &args).await,
        "restart_container" => restart_container(state, token, &args).await,
        "test_channel" => test_channel(state, &args).await,
        "post_incident" => post_incident(state, &args).await,
        other => Err(failed(format!("Unknown tool: {other}"))),
    }
}
