//! Rassemble les chiffres d'un rapport : disponibilité par équipement et par
//! dossier (mêmes requêtes que les pages de statut), incidents tirés de
//! l'historique des alertes, équipements les plus instables, et la même mesure
//! sur la période précédente.
//!
//! Le calcul d'assemblage est pur ([`assemble`], [`build_incidents`]) : seule
//! [`collect`] touche à la base et à VictoriaMetrics.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use dumbmonit_proto::{Target, TargetId};
use sqlx::Row;

use crate::api::status_pages::{
    PROBE_KINDS, first_up_query, instant_map, presence_slots, probe_uptime_query, rescale, selector,
};
use crate::db;
use crate::reports::layout::{Objectives, Scope};
use crate::reports::schedule::{Frequency, Period};
use crate::state::AppState;

/// Plafonds d'affichage : un rapport reste lisible dans un client de messagerie.
pub const MAX_DEVICE_ROWS: usize = 40;
pub const MAX_INCIDENT_ROWS: usize = 25;
pub const MAX_UNSTABLE: usize = 5;

/// Libellé du dossier des équipements qui n'en ont pas.
pub const UNGROUPED: &str = "Ungrouped";

#[derive(Debug, Clone)]
pub struct DeviceStat {
    pub name: String,
    pub group: String,
    pub uptime: Option<f64>,
    pub previous_uptime: Option<f64>,
    pub incidents: usize,
    pub downtime_secs: i64,
    /// Temps de réponse moyen et 95e centile sur la période, en ms ; seulement
    /// pour les sondes (HTTP, TCP…), qui mesurent une durée.
    pub latency_avg_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    /// Objectif propre à ce moniteur ou à son groupe (rapport SLA), qui
    /// remplace celui du rapport ; voir [`ReportData::apply_objectives`].
    pub objective: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct GroupStat {
    pub name: String,
    pub devices: usize,
    pub uptime: Option<f64>,
    pub previous_uptime: Option<f64>,
    /// Objectif propre à ce groupe, qui remplace celui du rapport.
    pub objective: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Incident {
    pub device: String,
    pub rule: String,
    pub severity: String,
    pub started: DateTime<Utc>,
    /// `None` : toujours en cours à la fin de la période.
    pub ended: Option<DateTime<Utc>>,
    pub began_before_period: bool,
}

impl Incident {
    pub fn duration(&self, period_end: DateTime<Utc>) -> TimeDelta {
        self.ended.unwrap_or(period_end) - self.started
    }
}

/// Une occurrence de fenêtre de maintenance dans la période.
#[derive(Debug, Clone, PartialEq)]
pub struct MaintenanceWindow {
    pub name: String,
    /// Moniteur visé ; `None` : plusieurs moniteurs (étiquettes) ou l'instance.
    pub monitor: Option<String>,
    pub starts: DateTime<Utc>,
    pub ends: DateTime<Utc>,
    /// Occurrence d'une fenêtre hebdomadaire ou mensuelle.
    pub recurring: bool,
}

#[derive(Debug, Clone)]
pub struct ReportData {
    pub name: String,
    pub frequency: Frequency,
    pub period: Period,
    /// Lien vers l'instance, si une URL publique est réglée.
    pub link: Option<String>,
    /// Faux quand VictoriaMetrics n'a pas répondu : le rapport le dit.
    pub availability_known: bool,
    pub fleet_uptime: Option<f64>,
    pub fleet_previous_uptime: Option<f64>,
    pub devices: Vec<DeviceStat>,
    pub groups: Vec<GroupStat>,
    pub incidents: Vec<Incident>,
    pub previous_incident_count: usize,
    pub downtime_secs: i64,
    /// Fenêtres de maintenance qui touchent la période (rapport SLA).
    pub maintenance: Vec<MaintenanceWindow>,
}

impl ReportData {
    /// Pose sur chaque ligne l'objectif propre à son moniteur ou à son groupe.
    pub fn apply_objectives(&mut self, objectives: &Objectives) {
        for device in &mut self.devices {
            device.objective = objectives.for_monitor(&device.name, &device.group);
        }
        for group in &mut self.groups {
            group.objective = objectives.for_group(&group.name);
        }
    }

    /// Équipements les plus instables : le plus d'incidents, puis le plus
    /// d'indisponibilité. Ceux qui n'ont rien eu n'y figurent pas.
    pub fn unstable(&self) -> Vec<&DeviceStat> {
        let mut ranked: Vec<&DeviceStat> =
            self.devices.iter().filter(|d| d.incidents > 0).collect();
        ranked.sort_by(|a, b| {
            b.incidents
                .cmp(&a.incidents)
                .then(b.downtime_secs.cmp(&a.downtime_secs))
                .then(a.name.cmp(&b.name))
        });
        ranked.truncate(MAX_UNSTABLE);
        ranked
    }
}

// --------------------------------------------------------------------------
// Incidents (purs)
// --------------------------------------------------------------------------

/// Ligne d'`alert_history` utile au rapport.
#[derive(Debug, Clone)]
pub struct HistoryRow {
    pub fingerprint: String,
    pub rule_uid: String,
    pub target_id: Option<i64>,
    pub to_phase: String,
    pub severity: String,
    pub at: DateTime<Utc>,
}

/// Incident brut : du passage en `firing` à la résolution.
#[derive(Debug, Clone, PartialEq)]
pub struct RawIncident {
    pub rule_uid: String,
    pub target_id: Option<i64>,
    pub severity: String,
    pub started: DateTime<Utc>,
    pub ended: Option<DateTime<Utc>>,
}

/// Apparie les transitions : une alerte ouvre un incident en passant à `firing`
/// et le ferme en repassant à `resolved` (ou `ok`). Les alertes d'information
/// ne sont pas des incidents.
pub fn build_incidents(rows: &[HistoryRow]) -> Vec<RawIncident> {
    let mut sorted: Vec<&HistoryRow> = rows.iter().collect();
    sorted.sort_by_key(|row| row.at);

    let mut open: HashMap<&str, RawIncident> = HashMap::new();
    let mut done = Vec::new();
    for row in sorted {
        if row.severity == "info" {
            continue;
        }
        match row.to_phase.as_str() {
            "firing" => {
                open.entry(row.fingerprint.as_str()).or_insert_with(|| RawIncident {
                    rule_uid: row.rule_uid.clone(),
                    target_id: row.target_id,
                    severity: row.severity.clone(),
                    started: row.at,
                    ended: None,
                });
            }
            "resolved" | "ok" => {
                if let Some(mut incident) = open.remove(row.fingerprint.as_str()) {
                    incident.ended = Some(row.at);
                    done.push(incident);
                }
            }
            _ => {}
        }
    }
    done.extend(open.into_values());
    done.sort_by_key(|incident| incident.started);
    done
}

fn overlaps(incident: &RawIncident, from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
    incident.started < to && incident.ended.unwrap_or(to) > from
}

/// Durée cumulée de plusieurs intervalles, sans compter deux fois les
/// chevauchements (deux règles en alerte sur le même équipement en même temps).
fn merged_secs(mut spans: Vec<(DateTime<Utc>, DateTime<Utc>)>) -> i64 {
    spans.sort();
    let mut total = 0;
    let mut current: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    for (start, end) in spans {
        match current {
            Some((cs, ce)) if start <= ce => current = Some((cs, ce.max(end))),
            Some((cs, ce)) => {
                total += (ce - cs).num_seconds();
                current = Some((start, end));
            }
            None => current = Some((start, end)),
        }
    }
    if let Some((cs, ce)) = current {
        total += (ce - cs).num_seconds();
    }
    total
}

// --------------------------------------------------------------------------
// Assemblage (pur)
// --------------------------------------------------------------------------

/// Ce qu'il faut savoir d'un équipement pour le rapport.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: i64,
    pub name: String,
    pub group: String,
}

pub struct Inputs<'a> {
    pub name: &'a str,
    pub frequency: Frequency,
    pub period: Period,
    pub link: Option<String>,
    pub devices: &'a [DeviceInfo],
    pub rule_names: &'a HashMap<String, String>,
    pub uptime: &'a HashMap<i64, f64>,
    pub previous_uptime: &'a HashMap<i64, f64>,
    pub availability_known: bool,
    pub incidents: &'a [RawIncident],
    /// Par équipement : (moyenne, 95e centile) du temps de réponse, en ms.
    pub latency: &'a HashMap<i64, (f64, f64)>,
}

fn mean(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values.fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

pub fn assemble(input: Inputs<'_>) -> ReportData {
    let Inputs { period, devices, incidents, .. } = input;
    let names: HashMap<i64, &DeviceInfo> = devices.iter().map(|d| (d.id, d)).collect();

    let in_period: Vec<&RawIncident> =
        incidents.iter().filter(|i| overlaps(i, period.start, period.end)).collect();
    let previous_incident_count =
        incidents.iter().filter(|i| overlaps(i, period.previous_start, period.start)).count();

    type Span = (DateTime<Utc>, DateTime<Utc>);
    let mut per_device: HashMap<i64, Vec<Span>> = HashMap::new();
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for incident in &in_period {
        if let Some(id) = incident.target_id {
            *counts.entry(id).or_default() += 1;
            per_device.entry(id).or_default().push((
                incident.started.max(period.start),
                incident.ended.unwrap_or(period.end).min(period.end),
            ));
        }
    }

    let mut stats: Vec<DeviceStat> = devices
        .iter()
        .map(|d| DeviceStat {
            name: d.name.clone(),
            group: if d.group.trim().is_empty() { UNGROUPED.to_string() } else { d.group.clone() },
            uptime: input.uptime.get(&d.id).copied(),
            previous_uptime: input.previous_uptime.get(&d.id).copied(),
            incidents: counts.get(&d.id).copied().unwrap_or(0),
            downtime_secs: per_device.remove(&d.id).map(merged_secs).unwrap_or(0),
            latency_avg_ms: input.latency.get(&d.id).map(|(avg, _)| *avg),
            latency_p95_ms: input.latency.get(&d.id).map(|(_, p95)| *p95),
            objective: None,
        })
        .collect();
    // Les moins disponibles d'abord ; les équipements sans mesure à la fin.
    stats.sort_by(|a, b| match (a.uptime, b.uptime) {
        (Some(x), Some(y)) => x.total_cmp(&y).then(a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });

    let mut by_group: BTreeMap<String, Vec<&DeviceStat>> = BTreeMap::new();
    for stat in &stats {
        by_group.entry(stat.group.clone()).or_default().push(stat);
    }
    let mut groups: Vec<GroupStat> = by_group
        .into_iter()
        .map(|(name, members)| GroupStat {
            name,
            devices: members.len(),
            uptime: mean(members.iter().filter_map(|d| d.uptime)),
            previous_uptime: mean(members.iter().filter_map(|d| d.previous_uptime)),
            objective: None,
        })
        .collect();
    groups.sort_by(|a, b| match (a.uptime, b.uptime) {
        (Some(x), Some(y)) => x.total_cmp(&y).then(a.name.cmp(&b.name)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.name.cmp(&b.name),
    });

    let listed: Vec<Incident> = in_period
        .iter()
        .map(|incident| Incident {
            device: incident
                .target_id
                .and_then(|id| names.get(&id).map(|d| d.name.clone()))
                .unwrap_or_else(|| "Removed device".to_string()),
            rule: input
                .rule_names
                .get(&incident.rule_uid)
                .cloned()
                .unwrap_or_else(|| incident.rule_uid.clone()),
            severity: incident.severity.clone(),
            started: incident.started,
            ended: incident.ended.filter(|end| *end <= period.end),
            began_before_period: incident.started < period.start,
        })
        .collect();

    let downtime_secs = stats.iter().map(|d| d.downtime_secs).sum();
    ReportData {
        name: input.name.to_string(),
        frequency: input.frequency,
        period,
        link: input.link,
        availability_known: input.availability_known,
        fleet_uptime: mean(stats.iter().filter_map(|d| d.uptime)),
        fleet_previous_uptime: mean(stats.iter().filter_map(|d| d.previous_uptime)),
        devices: stats,
        groups,
        incidents: listed,
        previous_incident_count,
        downtime_secs,
        maintenance: Vec::new(),
    }
}

/// Plafond d'occurrences listées : une fenêtre quotidienne sur un an en
/// ferait 366.
pub const MAX_MAINTENANCE_WINDOWS: usize = 200;

/// Occurrences des fenêtres de maintenance qui touchent `[from, to)`, triées
/// par début. Pur : les silences sont déjà lus et filtrés.
pub fn maintenance_windows(
    silences: &[crate::alerting::Silence],
    names: &HashMap<i64, String>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Vec<MaintenanceWindow> {
    use crate::alerting::Schedule;
    let mut out = Vec::new();
    for silence in silences.iter().filter(|s| s.enabled) {
        let recurring = !matches!(silence.schedule, Schedule::Once { .. });
        let monitor = silence.target_id.and_then(|id| names.get(&id).cloned());
        let mut push = |(starts, ends): (DateTime<Utc>, DateTime<Utc>)| {
            if starts < to && ends > from {
                out.push(MaintenanceWindow {
                    name: silence.name.clone(),
                    monitor: monitor.clone(),
                    starts,
                    ends,
                    recurring,
                });
            }
        };
        match &silence.schedule {
            Schedule::Recovery { .. } => {}
            Schedule::Once { starts_at, ends_at } => push((*starts_at, *ends_at)),
            schedule => {
                // L'occurrence ouverte au début de la période, puis les suivantes.
                if let Some(window) = schedule.window_containing(from) {
                    push(window);
                }
                let duration = schedule.duration();
                let mut cursor = from;
                for _ in 0..=MAX_MAINTENANCE_WINDOWS {
                    let Some(start) = schedule.next_start(cursor) else { break };
                    if start >= to {
                        break;
                    }
                    push((start, start + duration));
                    cursor = start;
                }
            }
        }
    }
    out.sort_by(|a, b| a.starts.cmp(&b.starts).then(a.name.cmp(&b.name)));
    out.truncate(MAX_MAINTENANCE_WINDOWS);
    out
}

// --------------------------------------------------------------------------
// Collecte (E/S)
// --------------------------------------------------------------------------

fn to_sql(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Disponibilité de chaque équipement sur la période et sur la précédente,
/// avec les mêmes requêtes et la même correction de couverture que les pages
/// de statut. `None` si VictoriaMetrics ne répond pas.
async fn uptimes(
    state: &AppState,
    targets: &[&Target],
    period: Period,
    now: DateTime<Utc>,
) -> Option<(HashMap<i64, f64>, HashMap<i64, f64>)> {
    let span = period.span().num_seconds();
    let now_secs = period.end.timestamp() as f64;
    // Une période passée (rapport d'un mois écoulé) se lit en décalant les
    // requêtes de ce qui sépare sa fin de maintenant.
    let lag = (now - period.end).num_seconds().max(0);
    let shift = if lag > 0 { format!(" offset {lag}s") } else { String::new() };
    let shift_prev = format!(" offset {}s", lag + span);
    // Fenêtre qui remonte jusqu'au début de la période précédente.
    let reach = (now - period.previous_start).num_seconds().max(span * 2);
    let span_f = span as f64;
    let mut current: HashMap<TargetId, f64> = HashMap::new();
    let mut previous: HashMap<TargetId, f64> = HashMap::new();

    let probes: Vec<TargetId> =
        targets.iter().filter(|t| PROBE_KINDS.contains(&t.kind.as_str())).map(|t| t.id).collect();
    let devices: Vec<TargetId> =
        targets.iter().filter(|t| !PROBE_KINDS.contains(&t.kind.as_str())).map(|t| t.id).collect();

    if !probes.is_empty() {
        let sel = selector(&probes);
        let window = format!("{span}s");
        let now_q = probe_uptime_query(&sel, &window, &shift);
        let prev_q = probe_uptime_query(&sel, &window, &shift_prev);
        match tokio::try_join!(state.victoria.query(&now_q), state.victoria.query(&prev_q)) {
            Ok((a, b)) => {
                current.extend(instant_map(a));
                previous.extend(instant_map(b));
            }
            Err(error) => {
                tracing::warn!(%error, "report: probe availability unavailable");
                return None;
            }
        }
    }

    if !devices.is_empty() {
        let sel = selector(&devices);
        let presence = presence_slots(&sel, &format!("{span}s"));
        let now_q = format!("avg_over_time({presence}{shift}) * 100");
        let prev_q = format!("avg_over_time({presence}{shift_prev}) * 100");
        let first_q = first_up_query(&sel, &format!("{reach}s"));
        match tokio::try_join!(
            state.victoria.query(&now_q),
            state.victoria.query(&prev_q),
            state.victoria.query(&first_q)
        ) {
            Ok((a, b, first)) => {
                let first = instant_map(first);
                for (id, measured) in instant_map(a) {
                    let Some(first_ts) = first.get(&id) else { continue };
                    let covered = span_f.min(now_secs - first_ts);
                    if let Some(value) = rescale(measured, span_f, covered) {
                        current.insert(id, value);
                    }
                }
                for (id, measured) in instant_map(b) {
                    let Some(first_ts) = first.get(&id) else { continue };
                    let covered = (now_secs - span_f - first_ts).clamp(0.0, span_f);
                    if let Some(value) = rescale(measured, span_f, covered) {
                        previous.insert(id, value);
                    }
                }
            }
            Err(error) => {
                tracing::warn!(%error, "report: device availability unavailable");
                return None;
            }
        }
    }
    Some((current, previous))
}

/// Temps de réponse des sondes sur la période : moyenne et 95e centile, en ms.
/// Meilleur effort : sans réponse de VictoriaMetrics, le rapport n'en montre pas.
async fn latencies(
    state: &AppState,
    targets: &[&Target],
    period: Period,
    now: DateTime<Utc>,
) -> HashMap<i64, (f64, f64)> {
    let probes: Vec<TargetId> =
        targets.iter().filter(|t| PROBE_KINDS.contains(&t.kind.as_str())).map(|t| t.id).collect();
    if probes.is_empty() {
        return HashMap::new();
    }
    let span = period.span().num_seconds();
    let lag = (now - period.end).num_seconds().max(0);
    let shift = if lag > 0 { format!(" offset {lag}s") } else { String::new() };
    let range =
        format!("dumbmonit_probe_duration_seconds{{{}}}[{span}s]{shift}", selector(&probes));
    let avg_q = format!("avg by (target) (avg_over_time({range})) * 1000");
    let p95_q = format!("max by (target) (quantile_over_time(0.95, {range})) * 1000");
    match tokio::try_join!(state.victoria.query(&avg_q), state.victoria.query(&p95_q)) {
        Ok((avg, p95)) => {
            let p95 = instant_map(p95);
            instant_map(avg)
                .into_iter()
                .filter_map(|(id, avg)| p95.get(&id).map(|p95| (id, (avg, *p95))))
                .collect()
        }
        Err(error) => {
            tracing::warn!(%error, "report: response times unavailable");
            HashMap::new()
        }
    }
}

async fn history(state: &AppState, since: DateTime<Utc>) -> Result<Vec<HistoryRow>> {
    let rows = sqlx::query(
        "SELECT fingerprint, rule_uid, target_id, to_phase, severity, at
         FROM alert_history WHERE at >= ? ORDER BY at, id",
    )
    .bind(to_sql(since))
    .fetch_all(&state.pool)
    .await
    .context("lecture de l'historique des alertes")?;
    rows.iter()
        .map(|row| {
            let at: String = row.try_get("at")?;
            Ok(HistoryRow {
                fingerprint: row.try_get("fingerprint")?,
                rule_uid: row.try_get("rule_uid")?,
                target_id: row.try_get("target_id")?,
                to_phase: row.try_get("to_phase")?,
                severity: row.try_get("severity")?,
                at: DateTime::parse_from_rfc3339(&at)
                    .map(|at| at.with_timezone(&Utc))
                    .unwrap_or_default(),
            })
        })
        .collect()
}

async fn rule_names(state: &AppState) -> Result<HashMap<String, String>> {
    let rows = sqlx::query("SELECT uid, name FROM alert_rules")
        .fetch_all(&state.pool)
        .await
        .context("lecture des règles")?;
    rows.iter().map(|row| Ok((row.try_get("uid")?, row.try_get("name")?))).collect()
}

/// Construit les données d'un rapport pour la période se terminant à `end`.
pub async fn collect(
    state: &AppState,
    name: &str,
    frequency: Frequency,
    end: DateTime<Utc>,
) -> Result<ReportData> {
    collect_period(state, name, frequency, Period::ending_at(end, frequency), end).await
}

/// Les données d'un rapport pour une période quelconque, lue à l'instant `now`
/// (la période peut être passée : un mois civil écoulé).
pub async fn collect_period(
    state: &AppState,
    name: &str,
    frequency: Frequency,
    period: Period,
    now: DateTime<Utc>,
) -> Result<ReportData> {
    collect_period_for(state, name, frequency, period, now, None).await
}

/// Comme [`collect_period`], limité aux équipements d'un client (mode MSP) :
/// ni ses moniteurs, ni ses incidents, ni ses comptes ne citent un autre client.
pub async fn collect_period_for(
    state: &AppState,
    name: &str,
    frequency: Frequency,
    period: Period,
    now: DateTime<Utc>,
    client: Option<i64>,
) -> Result<ReportData> {
    collect_scoped(state, name, frequency, period, now, client, &Scope::default()).await
}

/// Comme [`collect_period_for`], limité en plus au périmètre choisi (moniteurs
/// et groupes) : les incidents et les maintenances des autres moniteurs n'y
/// figurent pas.
pub async fn collect_scoped(
    state: &AppState,
    name: &str,
    frequency: Frequency,
    period: Period,
    now: DateTime<Utc>,
    client: Option<i64>,
    scope: &Scope,
) -> Result<ReportData> {
    let owned = match client {
        Some(client) => Some(crate::auth::clients::targets_of(&state.pool, client).await?),
        None => None,
    };
    let targets: Vec<Target> = db::targets::list(&state.pool, &state.cipher)
        .await?
        .into_iter()
        .filter(|target| target.enabled)
        .filter(|target| owned.as_ref().is_none_or(|owned| owned.contains(&target.id)))
        .filter(|target| scope.keeps(target.id, &target.group_name))
        .collect();
    // Rapport restreint (client ou périmètre) : seuls ses moniteurs comptent.
    let restricted: Option<HashSet<i64>> = if owned.is_some() || !scope.is_empty() {
        Some(targets.iter().map(|t| t.id).collect())
    } else {
        None
    };
    let refs: Vec<&Target> = targets.iter().collect();
    let devices: Vec<DeviceInfo> = targets
        .iter()
        .map(|t| DeviceInfo { id: t.id, name: t.name.clone(), group: t.group_name.clone() })
        .collect();

    let latency = latencies(state, &refs, period, now).await;
    let (uptime, previous_uptime, known) = match uptimes(state, &refs, period, now).await {
        Some((now, before)) => (now, before, true),
        None => (HashMap::new(), HashMap::new(), false),
    };

    // Un incident né avant la période mais encore ouvert doit apparaître : on
    // remonte une semaine plus tôt que la période précédente.
    let mut rows = history(state, period.previous_start - TimeDelta::days(7)).await?;
    if let Some(kept) = &restricted {
        // Un incident sans équipement, ou d'un autre client, ou hors du
        // périmètre, n'a rien à faire dans un rapport restreint.
        rows.retain(|row| row.target_id.is_some_and(|id| kept.contains(&id)));
    }
    let incidents = build_incidents(&rows);
    let rules = rule_names(state).await?;

    let global = crate::notify::policy_store::load_global(&state.pool).await.ok();
    let env_url = crate::config::env_var("DUMBMONIT_PUBLIC_URL");
    let link = global
        .and_then(|policy| policy.public_url(env_url.as_deref()))
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"));

    let mut data = assemble(Inputs {
        name,
        frequency,
        period,
        link,
        devices: &devices,
        rule_names: &rules,
        uptime: &uptime,
        previous_uptime: &previous_uptime,
        availability_known: known,
        incidents: &incidents,
        latency: &latency,
    });

    // Maintenances : meilleur effort, un silence illisible n'arrête pas le rapport.
    match db::alerts::list_maintenance_silences(&state.pool).await {
        Ok(mut silences) => {
            if let Some(kept) = &restricted {
                // Une fenêtre sans moniteur précis pourrait nommer un autre client.
                silences.retain(|s| s.target_id.is_some_and(|id| kept.contains(&id)));
            }
            let names: HashMap<i64, String> =
                targets.iter().map(|t| (t.id, t.name.clone())).collect();
            data.maintenance = maintenance_windows(&silences, &names, period.start, period.end);
        }
        Err(error) => tracing::warn!(%error, "report: maintenance windows unavailable"),
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).expect("ISO date").with_timezone(&Utc)
    }

    fn row(fp: &str, target: i64, phase: &str, severity: &str, when: &str) -> HistoryRow {
        HistoryRow {
            fingerprint: fp.to_string(),
            rule_uid: "rule-down".to_string(),
            target_id: Some(target),
            to_phase: phase.to_string(),
            severity: severity.to_string(),
            at: at(when),
        }
    }

    fn period() -> Period {
        Period::ending_at(at("2026-10-12T08:00:00Z"), Frequency::Weekly)
    }

    #[test]
    fn un_incident_va_du_firing_a_la_resolution() {
        let rows = [
            row("a", 1, "pending", "critical", "2026-10-06T10:00:00Z"),
            row("a", 1, "firing", "critical", "2026-10-06T10:05:00Z"),
            row("a", 1, "resolved", "critical", "2026-10-06T11:05:00Z"),
        ];
        let incidents = build_incidents(&rows);
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].started, at("2026-10-06T10:05:00Z"));
        assert_eq!(incidents[0].ended, Some(at("2026-10-06T11:05:00Z")));
    }

    #[test]
    fn les_alertes_d_information_ne_sont_pas_des_incidents() {
        let rows = [row("a", 1, "firing", "info", "2026-10-06T10:05:00Z")];
        assert!(build_incidents(&rows).is_empty());
    }

    fn devices() -> Vec<DeviceInfo> {
        vec![
            DeviceInfo { id: 1, name: "NAS".into(), group: "Storage".into() },
            DeviceInfo { id: 2, name: "Router".into(), group: String::new() },
            DeviceInfo { id: 3, name: "<b>Evil</b>".into(), group: "Storage".into() },
        ]
    }

    fn data(rows: &[HistoryRow]) -> ReportData {
        let incidents = build_incidents(rows);
        let rules = HashMap::from([("rule-down".to_string(), "Monitor unreachable".to_string())]);
        let uptime = HashMap::from([(1, 99.0), (2, 100.0), (3, 90.0)]);
        let previous = HashMap::from([(1, 98.0), (2, 100.0)]);
        assemble(Inputs {
            name: "Weekly report",
            frequency: Frequency::Weekly,
            period: period(),
            link: None,
            devices: &devices(),
            rule_names: &rules,
            uptime: &uptime,
            previous_uptime: &previous,
            availability_known: true,
            incidents: &incidents,
            latency: &HashMap::from([(1, (120.0, 340.0))]),
        })
    }

    #[test]
    fn le_temps_de_reponse_suit_l_equipement() {
        let report = data(&[]);
        let nas = report.devices.iter().find(|d| d.name == "NAS").expect("NAS");
        assert_eq!((nas.latency_avg_ms, nas.latency_p95_ms), (Some(120.0), Some(340.0)));
        let router = report.devices.iter().find(|d| d.name == "Router").expect("Router");
        assert_eq!((router.latency_avg_ms, router.latency_p95_ms), (None, None));
    }

    #[test]
    fn la_disponibilite_est_agregee_par_dossier_et_comparee() {
        let report = data(&[]);
        let storage = report.groups.iter().find(|g| g.name == "Storage").expect("group");
        assert_eq!(storage.devices, 2);
        assert_eq!(storage.uptime, Some(94.5));
        assert_eq!(storage.previous_uptime, Some(98.0));
        assert!(report.groups.iter().any(|g| g.name == UNGROUPED), "devices without a folder");
        assert_eq!(report.devices[0].name, "<b>Evil</b>", "worst availability first");
    }

    #[test]
    fn les_incidents_sont_comptes_dans_la_periode_et_la_precedente() {
        let rows = [
            // Dans la période.
            row("a", 1, "firing", "critical", "2026-10-06T10:00:00Z"),
            row("a", 1, "resolved", "critical", "2026-10-06T11:00:00Z"),
            // Dans la période précédente seulement.
            row("b", 2, "firing", "warning", "2026-09-30T10:00:00Z"),
            row("b", 2, "resolved", "warning", "2026-09-30T10:30:00Z"),
        ];
        let report = data(&rows);
        assert_eq!(report.incidents.len(), 1);
        assert_eq!(report.incidents[0].device, "NAS");
        assert_eq!(report.incidents[0].rule, "Monitor unreachable");
        assert_eq!(report.previous_incident_count, 1);
        assert_eq!(report.downtime_secs, 3600);
    }

    #[test]
    fn un_incident_ouvert_avant_la_periode_est_signale_et_borne() {
        let rows = [row("a", 1, "firing", "critical", "2026-10-01T10:00:00Z")];
        let report = data(&rows);
        assert_eq!(report.incidents.len(), 1);
        assert!(report.incidents[0].began_before_period);
        assert_eq!(report.incidents[0].ended, None);
        // Borné à la période : une semaine pleine, pas plus.
        assert_eq!(report.downtime_secs, 7 * 86_400);
    }

    #[test]
    fn deux_alertes_simultanees_ne_comptent_qu_une_fois_d_indisponibilite() {
        let rows = [
            row("a", 1, "firing", "critical", "2026-10-06T10:00:00Z"),
            row("a", 1, "resolved", "critical", "2026-10-06T11:00:00Z"),
            row("b", 1, "firing", "warning", "2026-10-06T10:30:00Z"),
            row("b", 1, "resolved", "warning", "2026-10-06T11:30:00Z"),
        ];
        let report = data(&rows);
        assert_eq!(report.incidents.len(), 2);
        assert_eq!(report.downtime_secs, 5400);
    }

    #[test]
    fn les_equipements_les_plus_instables_sont_classes() {
        let rows = [
            row("a", 1, "firing", "critical", "2026-10-06T10:00:00Z"),
            row("a", 1, "resolved", "critical", "2026-10-06T10:10:00Z"),
            row("b", 2, "firing", "critical", "2026-10-07T10:00:00Z"),
            row("b", 2, "resolved", "critical", "2026-10-07T12:00:00Z"),
            row("c", 2, "firing", "critical", "2026-10-08T10:00:00Z"),
            row("c", 2, "resolved", "critical", "2026-10-08T10:01:00Z"),
        ];
        let report = data(&rows);
        let unstable = report.unstable();
        assert_eq!(unstable.len(), 2);
        assert_eq!(unstable[0].name, "Router", "two incidents beat one");
        assert_eq!(unstable[1].name, "NAS");
    }

    fn silence(
        name: &str,
        target: Option<i64>,
        schedule: crate::alerting::Schedule,
    ) -> crate::alerting::Silence {
        crate::alerting::Silence {
            id: 1,
            name: name.to_string(),
            comment: String::new(),
            target_id: target,
            matchers: std::collections::BTreeMap::new(),
            schedule,
            enabled: true,
        }
    }

    #[test]
    fn les_fenetres_de_maintenance_de_la_periode_sont_listees() {
        use crate::alerting::Schedule;
        let from = at("2026-10-01T00:00:00Z");
        let to = at("2026-11-01T00:00:00Z");
        let names = HashMap::from([(1, "NAS".to_string())]);
        let once = silence(
            "Firmware",
            Some(1),
            Schedule::Once {
                starts_at: at("2026-10-10T02:00:00Z"),
                ends_at: at("2026-10-10T04:00:00Z"),
            },
        );
        let outside = silence(
            "Last year",
            Some(1),
            Schedule::Once {
                starts_at: at("2025-10-10T02:00:00Z"),
                ends_at: at("2025-10-10T04:00:00Z"),
            },
        );
        // Chaque dimanche de 2 h à 4 h UTC : quatre dimanches en octobre 2026.
        let weekly = silence(
            "Backups",
            None,
            Schedule::Weekly {
                days: vec![6],
                start_minute: 120,
                end_minute: 240,
                utc_offset_minutes: 0,
                timezone: Some("UTC".to_string()),
            },
        );
        let mut disabled = once.clone();
        disabled.enabled = false;
        let windows = maintenance_windows(&[once, outside, weekly, disabled], &names, from, to);
        assert_eq!(windows.len(), 5, "{windows:?}");
        let firmware = windows.iter().find(|w| w.name == "Firmware").expect("one-off window");
        assert_eq!(firmware.monitor.as_deref(), Some("NAS"));
        assert!(!firmware.recurring);
        let sundays: Vec<_> = windows.iter().filter(|w| w.name == "Backups").collect();
        assert_eq!(sundays.len(), 4);
        assert!(sundays.iter().all(|w| w.recurring && w.monitor.is_none()));
        assert_eq!(sundays[0].starts, at("2026-10-04T02:00:00Z"));
        assert_eq!(sundays[0].ends, at("2026-10-04T04:00:00Z"));
        assert!(windows.windows(2).all(|pair| pair[0].starts <= pair[1].starts), "sorted");
    }
}
