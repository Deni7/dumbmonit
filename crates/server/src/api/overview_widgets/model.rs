//! La forme exacte de la réponse, miroir de `web/src/lib/api/types/overview.ts`
//! (`OverviewWidgets`). Les noms de champs n'y changent pas : le test de forme
//! (`tests.rs`) les compare à ceux du contrat.

use serde::Serialize;

use crate::alerting::model::TargetId;

/// Famille d'un moniteur, pour les vues site × catégorie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Servers,
    Storage,
    Network,
    Services,
    Backups,
    Other,
}

/// État d'un moniteur, avec les mots de la console.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetStatus {
    Up,
    Down,
    Degraded,
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct WidgetMonitor {
    pub id: TargetId,
    pub name: String,
    pub kind: String,
    pub category: Category,
    pub status: WidgetStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct WidgetSite {
    pub key: String,
    pub name: String,
    pub monitors: Vec<WidgetMonitor>,
    pub wan_latency_ms: Option<Vec<f64>>,
    pub next_maintenance: Option<String>,
    pub last_backup: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TopEntry {
    pub target_id: TargetId,
    pub label: String,
    pub site: String,
    pub value: f64,
    pub unit: String,
    pub series: Vec<f64>,
    pub forecast_days: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TopConsumers {
    pub cpu: Vec<TopEntry>,
    pub mem: Vec<TopEntry>,
    pub disk: Vec<TopEntry>,
    pub net: Vec<TopEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupRun {
    pub at: String,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupStatus {
    Failed,
    Stale,
    Running,
    Ok,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupJob {
    pub job: String,
    pub engine: String,
    pub detail: String,
    pub site: String,
    pub target_id: TargetId,
    pub last_success: Option<String>,
    pub duration_s: Option<f64>,
    pub changed_bytes: Option<f64>,
    pub runs: Vec<BackupRun>,
    pub status: BackupStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceCheck {
    pub target_id: TargetId,
    pub name: String,
    pub kind: String,
    pub status: WidgetStatus,
    pub checks: Vec<Option<bool>>,
    pub p95_ms: Option<f64>,
    pub availability_30d: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlertDay {
    pub day: String,
    pub critical: u32,
    pub warning: u32,
    pub info: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Noisiest {
    pub rule: String,
    pub target: String,
    pub target_id: Option<TargetId>,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlertStats {
    pub days: Vec<AlertDay>,
    pub total: u32,
    pub previous_total: u32,
    pub mtta_s: Option<i64>,
    pub previous_mtta_s: Option<i64>,
    pub mttr_s: Option<i64>,
    pub previous_mttr_s: Option<i64>,
    pub acked_within_15m_pct: Option<f64>,
    pub noisiest: Option<Noisiest>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentVersionCount {
    pub version: String,
    pub count: u32,
    pub latest: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelaySummary {
    pub name: String,
    pub connected: bool,
    pub last_seen: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerUpdate {
    pub version: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AgentSummary {
    pub total: u32,
    pub versions: Vec<AgentVersionCount>,
    pub relays: Vec<RelaySummary>,
    pub server_update: Option<ServerUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EventKind {
    Crit,
    Warn,
    Ok,
    Info,
    Maint,
    Ack,
}

#[derive(Debug, Clone, Serialize)]
pub struct WidgetEvent {
    pub at: String,
    pub kind: EventKind,
    pub text: String,
    pub target_id: Option<TargetId>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayPct {
    pub day: String,
    pub pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvailabilitySeries {
    pub days: Vec<DayPct>,
    pub avg_30d: Option<f64>,
    pub previous_avg_30d: Option<f64>,
    pub slo_target: f64,
    pub budget_left_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayCount {
    pub day: String,
    pub count: u32,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AlertsOpened {
    pub days: Vec<DayCount>,
    pub this_week: u32,
    pub last_week: u32,
    pub last_resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OverviewWidgets {
    pub sites: Vec<WidgetSite>,
    pub top: TopConsumers,
    pub backups: Vec<BackupJob>,
    pub services: Vec<ServiceCheck>,
    pub alert_stats: AlertStats,
    pub agents: AgentSummary,
    pub events: Vec<WidgetEvent>,
    pub availability: AvailabilitySeries,
    pub alerts_opened: AlertsOpened,
}
