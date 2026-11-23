//! Les agents visibles : combien, quelles versions (la dernière est celle que
//! ce serveur livre), les relais et leur dernier contact, la mise à jour du
//! serveur quand la vérification en a trouvé une ; et, pour le fil, les mises
//! à jour d'agent de la semaine.

use std::collections::BTreeMap;

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};

use crate::collectors::agent::{self, selfupdate};
use crate::db;
use crate::state::AppState;

use super::events::EVENT_DAYS;
use super::model::{
    AgentSummary, AgentVersionCount, EventKind, RelaySummary, ServerUpdate, WidgetEvent,
};
use super::{Context, parse_time, stamp};

/// Un relais est connecté s'il s'est manifesté depuis moins de cinq minutes.
const RELAY_FRESH_SECS: i64 = 5 * 60;

#[derive(Debug, Default)]
pub struct AgentPart {
    pub summary: AgentSummary,
    pub events: Vec<WidgetEvent>,
}

pub async fn load(state: &AppState, ctx: &Context) -> Result<AgentPart> {
    let pool = &state.pool;
    let mut hosts = agent::list_hosts(pool).await?;
    hosts.retain(|(id, _, _)| ctx.sees(*id));
    let relayed = db::targets::relayed_counts(pool).await?;
    let latest = selfupdate::server_version();

    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    for (_, _, info) in &hosts {
        let version = info.agent_version.trim();
        if !version.is_empty() {
            *counts.entry(version.to_string()).or_default() += 1;
        }
    }
    let mut versions: Vec<AgentVersionCount> = counts
        .into_iter()
        .map(|(version, count)| AgentVersionCount { latest: version == latest, version, count })
        .collect();
    versions.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| b.version.cmp(&a.version)));

    let mut relays: Vec<RelaySummary> = hosts
        .iter()
        .filter(|(id, _, info)| info.relay || relayed.get(id).is_some_and(|n| *n > 0))
        .map(|(_, name, info)| {
            let seen = info.last_seen_at.as_deref().and_then(parse_time);
            RelaySummary {
                name: name.clone(),
                connected: seen.is_some_and(|at| (ctx.now - at).num_seconds() <= RELAY_FRESH_SECS),
                last_seen: seen.map(stamp),
            }
        })
        .collect();
    relays.sort_by(|a, b| a.connected.cmp(&b.connected).then_with(|| a.name.cmp(&b.name)));

    let summary = AgentSummary {
        total: u32::try_from(hosts.len()).unwrap_or(u32::MAX),
        versions,
        relays,
        server_update: server_update(state).await,
    };
    let events = update_events(state, ctx).await;
    Ok(AgentPart { summary, events })
}

/// La version plus récente du serveur, si la vérification (active) en a trouvé
/// une. Même lecture que `GET /api/update`.
async fn server_update(state: &AppState) -> Option<ServerUpdate> {
    let locked = crate::update::env_disabled();
    let enabled = !locked
        && !state.config.demo
        && crate::update::setting_enabled(&state.pool).await.unwrap_or(false);
    let info = crate::update::Checker::global()
        .get(Utc::now(), enabled, locked, false, crate::update::fetch_releases)
        .await
        .ok()?;
    if !info.update_available {
        return None;
    }
    info.latest.map(|version| ServerUpdate { version })
}

/// Les mises à jour d'agent rapportées pendant la profondeur du fil.
async fn update_events(state: &AppState, ctx: &Context) -> Vec<WidgetEvent> {
    let hosts = match selfupdate::hosts(&state.pool).await {
        Ok(hosts) => hosts,
        Err(error) => {
            tracing::warn!(?error, "overview widgets: agent updates not read");
            return Vec::new();
        }
    };
    let since = ctx.now - TimeDelta::days(EVENT_DAYS);
    hosts
        .into_iter()
        .filter(|host| ctx.sees(host.target_id))
        .filter_map(|host| {
            let outcome = host.state.outcome?;
            let at = DateTime::<Utc>::from_timestamp_millis(outcome.at_ms)?;
            if !(since..=ctx.now).contains(&at) {
                return None;
            }
            let version = outcome.version.unwrap_or_default();
            let name = host.name;
            let text = match outcome.outcome.as_str() {
                "succeeded" if version.is_empty() => format!("Agent updated on {name}"),
                "succeeded" => format!("Agent updated to {version} on {name}"),
                "rolled_back" => format!("Agent update rolled back on {name}"),
                "refused" => format!("Agent update refused on {name}"),
                _ => format!("Agent update failed on {name}"),
            };
            let kind = if outcome.outcome == "succeeded" { EventKind::Info } else { EventKind::Warn };
            Some(WidgetEvent { at: stamp(at), kind, text, target_id: Some(host.target_id) })
        })
        .collect()
}
