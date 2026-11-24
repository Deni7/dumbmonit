//! Les moniteurs visibles, leur état tel que la console le calcule, et leur
//! site : celui de l'agent relais qui les interroge (`relay:<site>`), ou
//! « ce site » (`local`, au nom de l'instance) pour le reste.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Result;
use chrono::{DateTime, TimeDelta, Utc};

use crate::alerting::model::{Severity, TargetId};
use crate::alerting::{EffectivePhase, Schedule, Silence};
use crate::auth::teams::Access;
use crate::collectors::agent;
use crate::db;
use crate::state::AppState;

use super::category::{category_of, has_probe_state, kind_label};
use super::model::{BackupJob, Category, WidgetMonitor, WidgetSite, WidgetStatus};
use super::{Context, stamp, vm};

/// Clé du site des moniteurs interrogés par le serveur lui-même.
pub const LOCAL_KEY: &str = "local";
/// Nom de l'instance quand aucun n'est réglé.
const DEFAULT_INSTANCE_NAME: &str = "DumbMonit";
/// Fraîcheur d'un résultat de sonde, alignée sur l'interface (`STATE_WINDOW`).
const PROBE_WINDOW: &str = "15m";

/// Un moniteur visible.
#[derive(Debug, Clone)]
pub struct Monitor {
    pub id: TargetId,
    pub name: String,
    pub kind: String,
    pub address: String,
    pub category: Category,
    pub status: WidgetStatus,
    /// Indice dans [`Fleet::sites`].
    pub site: usize,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteInfo {
    pub key: String,
    pub name: String,
}

/// Le parc visible, rangé par site.
#[derive(Debug, Default)]
pub struct Fleet {
    pub monitors: Vec<Monitor>,
    pub by_id: HashMap<TargetId, usize>,
    /// `local` d'abord, puis les sites des relais par nom.
    pub sites: Vec<SiteInfo>,
}

impl Fleet {
    pub fn monitor(&self, id: TargetId) -> Option<&Monitor> {
        self.by_id.get(&id).map(|index| &self.monitors[*index])
    }

    pub fn name_of(&self, id: TargetId) -> Option<&str> {
        self.monitor(id).map(|m| m.name.as_str())
    }

    /// Nom du site d'un moniteur, vide s'il est inconnu.
    pub fn site_of(&self, id: TargetId) -> String {
        self.monitor(id)
            .and_then(|m| self.sites.get(m.site))
            .map(|site| site.name.clone())
            .unwrap_or_default()
    }
}

/// Lit le parc visible et calcule l'état de chaque moniteur.
pub async fn load(
    state: &AppState,
    visible: Option<&HashSet<TargetId>>,
    filter: Option<&str>,
    health: &vm::Health,
    now: DateTime<Utc>,
) -> Result<Fleet> {
    let pool = &state.pool;
    let mut targets = db::targets::list(pool, &state.cipher).await?;
    targets.retain(|t| visible.is_none_or(|visible| visible.contains(&t.id)));
    let statuses = db::targets::statuses(pool).await?;
    let hosts = agent::list_hosts(pool).await?;
    let relayed = db::targets::relayed_counts(pool).await?;
    let instance = crate::instance::load(pool).await.name;
    let local_name = match instance.trim() {
        "" => DEFAULT_INSTANCE_NAME.to_string(),
        name => name.to_string(),
    };
    let firing = firing_targets(pool, now).await?;
    let probes = if targets.iter().any(|t| t.enabled && has_probe_state(&t.kind)) {
        probe_states(state, filter, health).await
    } else {
        HashMap::new()
    };

    // Les agents qui relaient : déclarés relais, ou choisis comme tels.
    let relays: HashMap<TargetId, String> = hosts
        .iter()
        .filter(|(id, _, info)| info.relay || relayed.get(id).is_some_and(|n| *n > 0))
        .map(|(id, name, info)| {
            let declared = info.site.clone().or_else(|| info.relay_wanted_site.clone());
            let seen = visible.is_none_or(|visible| visible.contains(id));
            let site = declared
                .map(|site| site.trim().to_string())
                .filter(|site| !site.is_empty())
                .or_else(|| seen.then(|| name.clone()))
                .unwrap_or_else(|| format!("Relay {id}"));
            (*id, site)
        })
        .collect();

    let mut entries = Vec::with_capacity(targets.len());
    for target in targets {
        let status = statuses.get(&target.id);
        let via = status.and_then(|s| s.via_agent);
        let site = site_name(target.id, via, &relays);
        let monitor_status = status_of(
            &target.kind,
            target.enabled,
            status.and_then(|s| s.last_error.as_deref()),
            status.and_then(|s| s.last_probe_at.as_deref()),
            target.interval.as_secs(),
            probes.get(&target.id).copied(),
            firing.contains(&target.id),
            now,
        );
        entries.push((
            site.map(str::to_string),
            Monitor {
                id: target.id,
                category: category_of(&target.kind),
                status: monitor_status,
                site: 0,
                created_at: status.and_then(|s| s.created_at.clone()),
                name: target.name,
                address: target.address,
                kind: target.kind,
            },
        ));
    }
    Ok(group(entries, &local_name))
}

/// Site d'un moniteur : celui de son relais, ou le sien s'il est lui-même un
/// relais. `None` : le site local.
pub fn site_name(
    id: TargetId,
    via: Option<TargetId>,
    relays: &HashMap<TargetId, String>,
) -> Option<&str> {
    via.and_then(|relay| relays.get(&relay)).or_else(|| relays.get(&id)).map(String::as_str)
}

/// Range les moniteurs par site : le site local d'abord (toujours présent quand
/// il a des moniteurs, ou quand il n'y a rien d'autre), puis les relais par nom.
pub fn group(entries: Vec<(Option<String>, Monitor)>, local_name: &str) -> Fleet {
    let mut remote: Vec<String> = entries.iter().filter_map(|(site, _)| site.clone()).collect();
    // Un ordre total (sans casse, puis exact) : deux noms égaux sont voisins et
    // `dedup` les fusionne. Trier sur la seule forme en minuscules laissait
    // « Lyon », « lyon », « Lyon » dans cet ordre, donc deux sites `relay:Lyon`.
    remote.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()).then_with(|| a.cmp(b)));
    remote.dedup();
    let has_local = entries.iter().any(|(site, _)| site.is_none());
    let mut sites = Vec::new();
    if has_local || remote.is_empty() {
        sites.push(SiteInfo { key: LOCAL_KEY.to_string(), name: local_name.to_string() });
    }
    let offset = sites.len();
    sites.extend(
        remote.iter().map(|name| SiteInfo { key: format!("relay:{name}"), name: name.clone() }),
    );
    let mut fleet = Fleet { sites, ..Fleet::default() };
    for (site, mut monitor) in entries {
        monitor.site = match site {
            None => 0,
            Some(name) => offset + remote.iter().position(|n| *n == name).unwrap_or(0),
        };
        fleet.by_id.insert(monitor.id, fleet.monitors.len());
        fleet.monitors.push(monitor);
    }
    fleet
}

/// État d'un moniteur, comme `consoleStatus` de l'interface
/// (`web/src/lib/components/enterprise/status.ts`). Les six états de la console
/// sont ramenés aux quatre du contrat : une erreur de configuration est
/// « dégradé » (à regarder, sans panne), un moniteur qui n'a encore rien
/// mesuré est « en pause » (neutre : ni en service, ni en panne).
#[allow(clippy::too_many_arguments)]
pub fn status_of(
    kind: &str,
    enabled: bool,
    last_error: Option<&str>,
    last_probe_at: Option<&str>,
    interval_secs: u64,
    probe: Option<bool>,
    firing: bool,
    now: DateTime<Utc>,
) -> WidgetStatus {
    if !enabled {
        return WidgetStatus::Paused;
    }
    if let Some(error) = last_error.map(str::trim).filter(|e| !e.is_empty()) {
        let down =
            db::targets::error_means_down(error) || db::targets::error_means_service_down(error);
        return if down { WidgetStatus::Down } else { WidgetStatus::Degraded };
    }
    let online = if has_probe_state(kind) {
        match probe {
            None => return WidgetStatus::Paused,
            Some(up) => up,
        }
    } else {
        let Some(last) = last_probe_at.and_then(super::parse_time) else {
            return WidgetStatus::Paused;
        };
        // Trois périodes, jamais moins de 90 secondes (ni plus d'un an).
        let tolerance = interval_secs.saturating_mul(3).clamp(90, 365 * 86_400);
        let tolerance = TimeDelta::seconds(i64::try_from(tolerance).unwrap_or(90));
        now - last <= tolerance
    };
    match (online, firing) {
        (false, _) => WidgetStatus::Down,
        (true, true) => WidgetStatus::Degraded,
        (true, false) => WidgetStatus::Up,
    }
}

/// Les moniteurs qui ont une alerte qui compte comme un problème : en cours,
/// ni masquée par un parent, ni acquittée, ni écartée, d'une gravité autre
/// qu'« info » (`countsAsProblem` de l'interface).
async fn firing_targets(pool: &sqlx::SqlitePool, now: DateTime<Utc>) -> Result<HashSet<TargetId>> {
    let severities: HashMap<String, Severity> = db::alerts::list_rules(pool)
        .await?
        .into_iter()
        .map(|rule| (rule.uid, rule.severity))
        .collect();
    Ok(db::alerts::list_active(pool)
        .await?
        .into_iter()
        .filter(|alert| {
            let severity = severities.get(&alert.rule_uid).copied().unwrap_or(Severity::Warning);
            severity != Severity::Info
                && alert.state.effective_phase() == EffectivePhase::Firing
                && !alert.state.is_acked(now)
                && !alert.state.is_dismissed()
        })
        .filter_map(|alert| alert.target_id)
        .collect())
}

/// Dernier résultat des sondes de service, le pire des lieux de sonde.
async fn probe_states(
    state: &AppState,
    filter: Option<&str>,
    health: &vm::Health,
) -> HashMap<TargetId, bool> {
    let query =
        format!("min by (target) (last_over_time(dumbmonit_probe_success[{PROBE_WINDOW}]))");
    vm::instant(state, filter, health, &query)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|s| Some((vm::target_of(&s.metric)?, vm::instant_value(s)? >= 1.0)))
        .collect()
}

/// Fenêtres de maintenance visibles du compte, sans les silences « jusqu'au
/// rétablissement » (un « je m'en occupe », pas une maintenance prévue).
pub async fn maintenance_silences(state: &AppState, access: &Access) -> Result<Vec<Silence>> {
    let mut silences = crate::api::alerts::visible_silences(access, state)
        .await
        .map_err(|error| anyhow::anyhow!(error.into_parts().1))?;
    silences.retain(|s| s.enabled && !matches!(s.schedule, Schedule::Recovery { .. }));
    Ok(silences)
}

/// Les moniteurs que couvre une fenêtre : le sien, celui de l'étiquette
/// `target`, ou tous quand elle vise toute l'instance. `None` : tous.
fn silence_scope(silence: &Silence) -> Option<Option<TargetId>> {
    if let Some(id) = silence.target_id {
        return Some(Some(id));
    }
    match silence.matchers.get("target") {
        Some(raw) => Some(raw.parse().ok()),
        None if silence.matchers.is_empty() => None,
        // Des étiquettes qui ne nomment pas de moniteur : on ne devine pas.
        None => Some(None),
    }
}

/// Début de la fenêtre en cours, sinon de la prochaine.
fn next_window(silence: &Silence, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    silence
        .schedule
        .window_containing(now)
        .map(|(start, _)| start)
        .or_else(|| silence.schedule.next_start(now))
}

/// Les sites, avec leurs moniteurs, la prochaine maintenance et la dernière
/// sauvegarde réussie.
pub fn build(ctx: &Context, silences: &[Silence], backups: &[BackupJob]) -> Vec<WidgetSite> {
    let fleet = &ctx.fleet;
    let mut members: Vec<Vec<&Monitor>> = vec![Vec::new(); fleet.sites.len()];
    for monitor in &fleet.monitors {
        if let Some(list) = members.get_mut(monitor.site) {
            list.push(monitor);
        }
    }
    let windows: Vec<(Option<Option<TargetId>>, DateTime<Utc>)> = silences
        .iter()
        .filter_map(|s| next_window(s, ctx.now).map(|start| (silence_scope(s), start)))
        .collect();
    let mut last_backup: BTreeMap<usize, String> = BTreeMap::new();
    for job in backups {
        let Some(at) = job.last_success.as_ref() else { continue };
        let Some(monitor) = fleet.monitor(job.target_id) else { continue };
        let entry = last_backup.entry(monitor.site).or_default();
        if *at > *entry {
            entry.clone_from(at);
        }
    }

    fleet
        .sites
        .iter()
        .enumerate()
        .map(|(index, site)| {
            let ids: HashSet<TargetId> = members[index].iter().map(|m| m.id).collect();
            let next = windows
                .iter()
                .filter(|(scope, _)| match scope {
                    None => true,
                    Some(Some(id)) => ids.contains(id),
                    Some(None) => false,
                })
                .map(|(_, start)| *start)
                .min();
            WidgetSite {
                key: site.key.clone(),
                name: site.name.clone(),
                monitors: members[index]
                    .iter()
                    .map(|m| WidgetMonitor {
                        id: m.id,
                        name: m.name.clone(),
                        kind: kind_label(&m.kind, &m.address),
                        category: m.category,
                        status: m.status,
                    })
                    .collect(),
                // Aucune série de latence entre le serveur et ses relais n'est
                // publiée aujourd'hui.
                wan_latency_ms: None,
                next_maintenance: next.map(stamp),
                last_backup: last_backup.remove(&index),
            }
        })
        .collect()
}
