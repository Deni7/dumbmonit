//! Mise à jour automatique des agents : quand proposer à une machine le
//! binaire de ce build.
//!
//! À ne pas confondre avec la mise à jour des conteneurs Docker (`policy.rs`).
//!
//! Le serveur livre l'agent de **son propre build** (`DUMBMONIT_AGENT_DIR`) ;
//! c'est l'empreinte SHA-256 du binaire qui dit si une machine est à jour, pas
//! la seule version — un serveur `:edge` aligne ses agents sur son build. La
//! décision tient dans une fonction pure ([`evaluate`]) : réglages globaux et
//! par machine, fenêtre (n'importe quand, maintenance, à la main), déploiement
//! progressif (machines pilotes d'abord), pas de retour en arrière sans
//! demande, pas de nouvelle tentative automatique d'une version qui a échoué.
//! L'agent, de son côté, vérifie la signature et peut encore refuser.
//!
//! Le déploiement progressif a deux formes : les machines pilotes d'abord
//! (défaut), ou des vagues ordonnées, avec pause et arrêt sur échec
//! (`rollout.rs`).

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use dumbmonit_proto::{AgentIdentity, AgentUpdateOffer, TargetId};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::alerting::silence;
use crate::api::agent_files::{self, ShippedBinary};
use crate::collectors::agent::rollout::{self, Hold, Member, MemberState, Plan, Rollout};
use crate::db;
use crate::state::AppState;

/// Réglage « mise à jour automatique des agents » : `on` / `off` (absent : `on`).
pub const SETTING_ENABLED: &str = "agent_update_enabled";
/// Fenêtre : `any`, `maintenance` ou `manual` (absent : `any`).
pub const SETTING_WINDOW: &str = "agent_update_window";
/// Minutes de succès des machines pilotes avant les autres (absent : 60).
pub const SETTING_CANARY_MINUTES: &str = "agent_update_canary_minutes";
/// « Accepter les builds non signés (développement) » : `on` / `off`.
pub const SETTING_ALLOW_UNSIGNED: &str = "agent_update_allow_unsigned";
/// Instant (ms) du dernier changement de ce réglage : les refus antérieurs
/// (« build non signé ») ne comptent plus.
pub const SETTING_RESET_MS: &str = "agent_update_reset_ms";

/// Minutes d'attente par défaut après la mise à jour des machines pilotes.
pub const DEFAULT_CANARY_MINUTES: u32 = 60;
/// Plafond du réglage : une semaine.
pub const MAX_CANARY_MINUTES: u32 = 7 * 24 * 60;

/// Une offre faite il y a moins que cela, sans réponse, est « en cours ».
const UPDATING_FOR_MS: i64 = 15 * 60 * 1000;

/// Fenêtre de mise à jour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Window {
    /// Dès que possible.
    Any,
    /// Seulement pendant une fenêtre de maintenance (silence) de la machine.
    Maintenance,
    /// Jamais d'elle-même : bouton « Update now ».
    Manual,
}

impl Window {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "any" => Some(Self::Any),
            "maintenance" => Some(Self::Maintenance),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::Maintenance => "maintenance",
            Self::Manual => "manual",
        }
    }
}

/// Réglages globaux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Settings {
    pub enabled: bool,
    pub window: Window,
    pub canary_minutes: u32,
    pub allow_unsigned: bool,
    #[serde(skip)]
    pub reset_ms: i64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            window: Window::Any,
            canary_minutes: DEFAULT_CANARY_MINUTES,
            allow_unsigned: false,
            reset_ms: 0,
        }
    }
}

/// Issue de la dernière tentative, telle que l'agent l'a rapportée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// `succeeded`, `failed`, `rolled_back` ou `refused`.
    pub outcome: String,
    #[serde(skip)]
    pub sha256: String,
    /// Version visée.
    pub version: Option<String>,
    pub reason: Option<String>,
    #[serde(skip)]
    pub at_ms: i64,
    pub at: Option<String>,
}

/// Une machine, du point de vue de sa mise à jour.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostState {
    pub agent_version: String,
    pub sha256: Option<String>,
    pub platform: Option<String>,
    /// Ce que l'agent dit de sa capacité ; `None` : agent antérieur.
    pub ability: Option<String>,
    /// Réglage propre à la machine ; `None` : le réglage global.
    pub mode: Option<bool>,
    pub canary: bool,
    pub requested_ms: Option<i64>,
    pub allow_downgrade: bool,
    pub offered_ms: Option<i64>,
    pub offered_sha: Option<String>,
    pub sha_since_ms: Option<i64>,
    pub outcome: Option<Outcome>,
    pub in_maintenance: bool,
}

impl HostState {
    fn enabled(&self, settings: &Settings) -> bool {
        self.mode.unwrap_or(settings.enabled)
    }

    fn up_to_date(&self, binary: &ShippedBinary) -> bool {
        self.sha256.as_deref().is_some_and(|sha| sha.eq_ignore_ascii_case(&binary.sha256))
    }

    /// Échec (ou refus) déjà rapporté pour ce binaire, et toujours d'actualité.
    fn failure_for(&self, binary: &ShippedBinary, settings: &Settings) -> Option<&Outcome> {
        let outcome = self.outcome.as_ref()?;
        if outcome.outcome == "succeeded" || !outcome.sha256.eq_ignore_ascii_case(&binary.sha256) {
            return None;
        }
        // Une demande explicite postérieure le fait retenter une fois.
        if self.requested_ms.is_some_and(|requested| outcome.at_ms <= requested) {
            return None;
        }
        // Un refus se réévalue quand le réglage des builds non signés change.
        if outcome.outcome == "refused" && outcome.at_ms <= settings.reset_ms {
            return None;
        }
        Some(outcome)
    }
}

/// Feu du déploiement progressif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Open,
    /// Les machines ordinaires attendent les pilotes : pourquoi.
    Waiting(String),
    /// Déploiement par vagues : la vague de la machine n'est pas ouverte.
    Wave,
    /// Pause manuelle.
    Paused,
    /// Propagation arrêtée : trop d'échecs dans une vague.
    Halted,
}

impl From<Option<Hold>> for Gate {
    fn from(hold: Option<Hold>) -> Self {
        match hold {
            None => Self::Open,
            Some(Hold::Wave) => Self::Wave,
            Some(Hold::Paused) => Self::Paused,
            Some(Hold::Halted) => Self::Halted,
        }
    }
}

/// Ce que la décision rend pour une machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Code d'état pour l'interface.
    pub status: &'static str,
    pub reason: Option<String>,
    /// Faut-il joindre l'offre à l'accusé de réception ?
    pub offer: bool,
    /// Demande explicite : retenter même une version en échec.
    pub force: bool,
    pub allow_downgrade: bool,
}

impl Verdict {
    fn hold(status: &'static str, reason: Option<String>) -> Self {
        Self { status, reason, offer: false, force: false, allow_downgrade: false }
    }
}

/// Décide pour une machine.
pub fn evaluate(
    settings: &Settings,
    host: &HostState,
    binary: Option<&ShippedBinary>,
    server_version: &str,
    gate: &Gate,
    now_ms: i64,
) -> Verdict {
    match host.ability.as_deref() {
        None => return Verdict::hold("legacy", None),
        Some("ready") => {}
        Some("container") => return Verdict::hold("container", None),
        Some("read_only") => return Verdict::hold("read_only", None),
        Some("disabled") => return Verdict::hold("disabled_locally", None),
        Some(_) => return Verdict::hold("unsupported", None),
    }
    let Some(binary) = binary else { return Verdict::hold("no_binary", None) };
    if host.up_to_date(binary) {
        return Verdict::hold("up_to_date", None);
    }
    let requested = host.requested_ms.is_some();
    if !requested && !host.enabled(settings) {
        return Verdict::hold("disabled", None);
    }
    // Build sans signature : l'agent le refuserait. Rien n'est proposé, et
    // l'interface dit pourquoi (« mises à jour bloquées : builds non signés »)
    // plutôt que de montrer un refus après coup.
    if binary.signature.is_none() && !settings.allow_unsigned {
        return Verdict::hold("unsigned", None);
    }
    let downgrade = dumbmonit_proto::is_older(server_version, &host.agent_version);
    let allow_downgrade = requested && host.allow_downgrade;
    if downgrade && !allow_downgrade {
        return Verdict::hold("newer", None);
    }
    if let Some(failure) = host.failure_for(binary, settings) {
        let status = if failure.outcome == "refused" { "refused" } else { "failed" };
        return Verdict::hold(status, failure.reason.clone());
    }
    if !requested {
        match settings.window {
            Window::Manual => return Verdict::hold("manual", None),
            Window::Maintenance if !host.in_maintenance => {
                return Verdict::hold("waiting_window", None);
            }
            _ => {}
        }
        match gate {
            Gate::Open => {}
            Gate::Waiting(why) if !host.canary => {
                return Verdict::hold("waiting_canary", Some(why.clone()));
            }
            Gate::Waiting(_) => {}
            Gate::Wave => return Verdict::hold("waiting_wave", None),
            Gate::Paused => return Verdict::hold("paused", None),
            Gate::Halted => return Verdict::hold("halted", None),
        }
    }
    let recently_offered = host.offered_sha.as_deref() == Some(binary.sha256.as_str())
        && host.offered_ms.is_some_and(|at| now_ms - at < UPDATING_FOR_MS);
    Verdict {
        status: if recently_offered { "updating" } else { "pending" },
        reason: None,
        offer: true,
        force: requested,
        allow_downgrade,
    }
}

/// Une machine pilote, pour le feu du déploiement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canary {
    pub name: String,
    pub up_to_date: bool,
    /// Depuis quand elle tourne le binaire attendu.
    pub since_ms: Option<i64>,
    pub failed: bool,
}

/// Feu du déploiement progressif : ouvert quand toutes les machines pilotes
/// tournent le nouveau binaire depuis `minutes`, sans échec.
pub fn gate(canaries: &[Canary], minutes: u32, now_ms: i64) -> Gate {
    if canaries.is_empty() {
        return Gate::Open;
    }
    if let Some(failed) = canaries.iter().find(|canary| canary.failed) {
        return Gate::Waiting(format!("canary agent {} failed to update", failed.name));
    }
    let waiting = canaries.iter().filter(|canary| !canary.up_to_date).count();
    if waiting > 0 {
        return Gate::Waiting(format!("waiting for {waiting} canary agent(s) to update"));
    }
    let latest = canaries.iter().filter_map(|canary| canary.since_ms).max().unwrap_or(0);
    let opens_at = latest + i64::from(minutes) * 60_000;
    if now_ms < opens_at {
        let at = DateTime::<Utc>::from_timestamp_millis(opens_at)
            .map(|at| at.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_default();
        return Gate::Waiting(format!("canary agents updated, the others follow at {at}"));
    }
    Gate::Open
}

// ------------------------------------------------------------------ réglages

async fn read_setting(pool: &SqlitePool, key: &str) -> Result<Option<String>> {
    Ok(sqlx::query_scalar("SELECT value FROM app_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?)
}

async fn write_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO app_settings (key, value) VALUES (?, ?) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Réglages globaux, défauts compris.
pub async fn settings(pool: &SqlitePool) -> Result<Settings> {
    let mut settings = Settings::default();
    if let Some(value) = read_setting(pool, SETTING_ENABLED).await? {
        settings.enabled = value != "off";
    }
    if let Some(window) = read_setting(pool, SETTING_WINDOW).await?.and_then(|w| Window::parse(&w))
    {
        settings.window = window;
    }
    if let Some(minutes) =
        read_setting(pool, SETTING_CANARY_MINUTES).await?.and_then(|m| m.parse::<u32>().ok())
    {
        settings.canary_minutes = minutes.min(MAX_CANARY_MINUTES);
    }
    settings.allow_unsigned =
        read_setting(pool, SETTING_ALLOW_UNSIGNED).await?.as_deref() == Some("on");
    settings.reset_ms = read_setting(pool, SETTING_RESET_MS)
        .await?
        .and_then(|ms| ms.parse::<i64>().ok())
        .unwrap_or(0);
    Ok(settings)
}

/// Modification partielle des réglages.
#[derive(Debug, Clone, Default)]
pub struct SettingsChange {
    pub enabled: Option<bool>,
    pub window: Option<Window>,
    pub canary_minutes: Option<u32>,
    pub allow_unsigned: Option<bool>,
}

pub async fn change_settings(pool: &SqlitePool, change: &SettingsChange) -> Result<()> {
    let on_off = |flag: bool| if flag { "on" } else { "off" };
    if let Some(enabled) = change.enabled {
        write_setting(pool, SETTING_ENABLED, on_off(enabled)).await?;
    }
    if let Some(window) = change.window {
        write_setting(pool, SETTING_WINDOW, window.as_str()).await?;
    }
    if let Some(minutes) = change.canary_minutes {
        write_setting(pool, SETTING_CANARY_MINUTES, &minutes.min(MAX_CANARY_MINUTES).to_string())
            .await?;
    }
    if let Some(allow) = change.allow_unsigned {
        let before = settings(pool).await?.allow_unsigned;
        write_setting(pool, SETTING_ALLOW_UNSIGNED, on_off(allow)).await?;
        if before != allow {
            let now = Utc::now().timestamp_millis().to_string();
            write_setting(pool, SETTING_RESET_MS, &now).await?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- machines

/// Range ce que l'agent dit de lui-même (empreinte, capacité, dernière
/// tentative), à chaque lot.
pub async fn record(
    pool: &SqlitePool,
    target_id: TargetId,
    identity: &AgentIdentity,
) -> Result<()> {
    let info = identity.self_update.as_ref();
    let sha = info.and_then(|info| info.sha256.as_deref()).map(str::to_ascii_lowercase);
    let platform = info.and_then(|info| info.platform.clone());
    let ability = info.map(|info| info.ability.as_str());
    let now = Utc::now().timestamp_millis();
    sqlx::query(
        "UPDATE agent_hosts
         SET agent_sha_since_ms = CASE WHEN agent_sha256 IS ? THEN agent_sha_since_ms ELSE ? END,
             agent_sha256 = ?, agent_platform = ?, update_ability = ?
         WHERE target_id = ?",
    )
    .bind(&sha)
    .bind(now)
    .bind(&sha)
    .bind(&platform)
    .bind(ability)
    .bind(target_id)
    .execute(pool)
    .await
    .context("recording the agent binary")?;

    if let Some(last) = info.and_then(|info| info.last.as_ref()) {
        sqlx::query(
            "UPDATE agent_hosts
             SET update_outcome = ?, update_outcome_sha = ?, update_outcome_reason = ?,
                 update_outcome_ms = ?, update_outcome_to = ?
             WHERE target_id = ? AND (update_outcome_ms IS NULL OR update_outcome_ms < ?)",
        )
        .bind(last.outcome.as_str())
        .bind(last.to_sha256.to_ascii_lowercase())
        .bind(last.reason.as_deref().map(|reason| truncate(reason, 500)))
        .bind(last.at_ms)
        .bind(&last.to_version)
        .bind(target_id)
        .bind(last.at_ms)
        .execute(pool)
        .await
        .context("recording the agent update outcome")?;
    }
    Ok(())
}

fn truncate(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

// `AssertSqlSafe` : seule cette constante du module entre dans le texte.
const HOST_COLUMNS: &str = "h.target_id, t.name, h.hostname, h.agent_version, h.agent_sha256,
     h.agent_platform, h.update_ability, h.update_mode, h.update_canary, h.update_requested_ms,
     h.update_allow_downgrade, h.update_offered_ms, h.update_offered_sha, h.agent_sha_since_ms,
     h.update_outcome, h.update_outcome_sha, h.update_outcome_reason, h.update_outcome_ms,
     h.update_outcome_to, h.last_seen_at, h.site, t.group_name, t.tags";

/// Une machine telle que la base la connaît.
#[derive(Debug, Clone)]
pub struct HostRow {
    pub target_id: TargetId,
    pub name: String,
    pub hostname: String,
    pub last_seen_at: Option<String>,
    /// Site déclaré par l'agent.
    pub site: Option<String>,
    /// Dossier de l'équipement (vide : aucun).
    pub group: String,
    pub tags: BTreeMap<String, String>,
    pub state: HostState,
}

fn at(ms: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|at| at.format("%Y-%m-%dT%H:%M:%S").to_string())
}

fn row_to_host(row: &sqlx::sqlite::SqliteRow) -> Result<HostRow> {
    let outcome = match row.try_get::<Option<String>, _>("update_outcome")? {
        Some(outcome) => {
            let at_ms: i64 = row.try_get::<Option<i64>, _>("update_outcome_ms")?.unwrap_or(0);
            Some(Outcome {
                outcome,
                sha256: row.try_get::<Option<String>, _>("update_outcome_sha")?.unwrap_or_default(),
                version: row.try_get("update_outcome_to")?,
                reason: row.try_get("update_outcome_reason")?,
                at_ms,
                at: at(at_ms),
            })
        }
        None => None,
    };
    let mode = match row.try_get::<Option<String>, _>("update_mode")?.as_deref() {
        Some("on") => Some(true),
        Some("off") => Some(false),
        _ => None,
    };
    Ok(HostRow {
        target_id: row.try_get("target_id")?,
        name: row.try_get("name")?,
        hostname: row.try_get("hostname")?,
        last_seen_at: row.try_get("last_seen_at")?,
        site: row.try_get("site")?,
        group: row.try_get("group_name")?,
        // Illisibles : sans étiquette plutôt qu'une erreur pour tout le parc.
        tags: serde_json::from_str(&row.try_get::<String, _>("tags")?).unwrap_or_default(),
        state: HostState {
            agent_version: row.try_get("agent_version")?,
            sha256: row.try_get("agent_sha256")?,
            platform: row.try_get("agent_platform")?,
            ability: row.try_get("update_ability")?,
            mode,
            canary: row.try_get::<i64, _>("update_canary")? != 0,
            requested_ms: row.try_get("update_requested_ms")?,
            allow_downgrade: row.try_get::<i64, _>("update_allow_downgrade")? != 0,
            offered_ms: row.try_get("update_offered_ms")?,
            offered_sha: row.try_get("update_offered_sha")?,
            sha_since_ms: row.try_get("agent_sha_since_ms")?,
            outcome,
            in_maintenance: false,
        },
    })
}

/// Toutes les machines à agent.
pub async fn hosts(pool: &SqlitePool) -> Result<Vec<HostRow>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {HOST_COLUMNS} FROM agent_hosts h JOIN targets t ON t.id = h.target_id
         ORDER BY t.name"
    )))
    .fetch_all(pool)
    .await
    .context("listing the agents")?;
    rows.iter().map(row_to_host).collect()
}

async fn host_row(pool: &SqlitePool, target_id: TargetId) -> Result<Option<HostRow>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {HOST_COLUMNS} FROM agent_hosts h JOIN targets t ON t.id = h.target_id
         WHERE h.target_id = ?"
    )))
    .bind(target_id)
    .fetch_optional(pool)
    .await
    .context("reading the agent")?;
    row.as_ref().map(row_to_host).transpose()
}

/// Binaires de ce build, par plateforme.
pub type Binaries = BTreeMap<String, ShippedBinary>;

/// Binaires que l'image livre, par plateforme.
pub async fn binaries(agent_dir: &std::path::Path) -> Binaries {
    let mut found = BTreeMap::new();
    for name in agent_files::AGENT_FILES {
        let platform = name.trim_start_matches("dumbmonit-agent-").trim_end_matches(".exe");
        if let Some(binary) = agent_files::shipped_binary(agent_dir, platform).await {
            found.insert(platform.to_string(), binary);
        }
    }
    found
}

/// Machines pilotes qui comptent pour le feu : celles qui peuvent se mettre
/// à jour, et le veulent.
fn canaries(hosts: &[HostRow], binaries: &Binaries, settings: &Settings) -> Vec<Canary> {
    hosts
        .iter()
        .filter(|host| host.state.canary && host.state.enabled(settings))
        .filter(|host| host.state.ability.as_deref() == Some("ready"))
        .filter_map(|host| {
            let binary = binaries.get(host.state.platform.as_deref()?)?;
            let up_to_date = host.state.up_to_date(binary);
            Some(Canary {
                name: host.name.clone(),
                up_to_date,
                since_ms: if up_to_date { host.state.sha_since_ms } else { None },
                failed: host.state.failure_for(binary, settings).is_some(),
            })
        })
        .collect()
}

/// Une machine qu'on n'a pas vue depuis une semaine n'est plus attendue par les
/// vagues : elle ne doit pas les bloquer.
const ABANDONED_AFTER_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Instant (ms) d'une date du serveur (UTC, sans suffixe ou avec `Z`).
fn seen_ms(text: &str) -> Option<i64> {
    let text = text.trim_end_matches('Z');
    let naive = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(naive.and_utc().timestamp_millis())
}

/// Les machines que les vagues ordonnent : celles qui peuvent se mettre à
/// jour, le veulent (une machine exclue, c'est-à-dire épinglée, n'en fait pas
/// partie) et ont donné signe de vie cette semaine. Le binaire de leur
/// plateforme doit être livré.
fn rollout_members(
    hosts: &[HostRow],
    binaries: &Binaries,
    settings: &Settings,
    rollout: &Rollout,
    now_ms: i64,
) -> Vec<Member> {
    hosts
        .iter()
        .filter_map(|host| {
            let state = &host.state;
            if !state.enabled(settings) || state.ability.as_deref() != Some("ready") {
                return None;
            }
            let binary = binaries.get(state.platform.as_deref()?)?;
            let seen = host.last_seen_at.as_deref().and_then(seen_ms);
            if seen.is_some_and(|seen| now_ms - seen > ABANDONED_AFTER_MS) {
                return None;
            }
            let member_state = if state.up_to_date(binary) {
                MemberState::Done { since_ms: state.sha_since_ms.unwrap_or(0) }
            } else if dumbmonit_proto::is_older(server_version(), &state.agent_version) {
                MemberState::Idle
            } else {
                match state.failure_for(binary, settings) {
                    Some(outcome) if outcome.at_ms > rollout.forgiven_before_ms => {
                        MemberState::Failed
                    }
                    Some(_) => MemberState::Idle,
                    None => MemberState::Pending,
                }
            };
            Some(Member {
                id: host.target_id,
                canary: state.canary,
                site: host.site.clone(),
                group: host.group.clone(),
                tags: host.tags.clone(),
                state: member_state,
            })
        })
        .collect()
}

/// Le plan des vagues ; `None` quand aucune n'est configurée (déploiement
/// « pilotes d'abord »).
fn wave_plan(
    hosts: &[HostRow],
    binaries: &Binaries,
    settings: &Settings,
    rollout: &Rollout,
    now_ms: i64,
) -> Option<Plan> {
    if rollout.waves.is_empty() {
        return None;
    }
    let members = rollout_members(hosts, binaries, settings, rollout, now_ms);
    Some(rollout::plan(rollout, &members, now_ms))
}

/// Machine sous une fenêtre de maintenance (silence) en ce moment.
fn in_maintenance(silences: &[silence::Silence], target_id: TargetId) -> bool {
    silence::first_match(silences, Utc::now(), Some(target_id), &BTreeMap::new()).is_some()
}

/// Version et build de ce serveur, ceux du binaire de l'agent qu'il livre.
pub fn server_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn server_build() -> Option<&'static str> {
    option_env!("DUMBMONIT_BUILD").map(str::trim).filter(|build| !build.is_empty())
}

/// L'offre à joindre à l'accusé de réception de cette machine, s'il y en a
/// une. Note l'offre, et clôt une demande explicite qui a abouti.
pub async fn offer_for(state: &AppState, target_id: TargetId) -> Result<Option<AgentUpdateOffer>> {
    let pool = &state.pool;
    let Some(mut host) = host_row(pool, target_id).await? else { return Ok(None) };
    // Agent antérieur, ou qui ne peut pas se remplacer : rien à calculer.
    if host.state.ability.as_deref() != Some("ready") {
        return Ok(None);
    }
    let Some(platform) = host.state.platform.clone() else { return Ok(None) };
    let Some(binary) = agent_files::shipped_binary(&state.config.agent_dir, &platform).await else {
        return Ok(None);
    };
    if host.state.up_to_date(&binary) || answered(&host.state) {
        clear_request(pool, target_id).await?;
        if host.state.up_to_date(&binary) {
            return Ok(None);
        }
        host.state.requested_ms = None;
    }
    let settings = settings(pool).await?;
    if settings.window == Window::Maintenance && host.state.requested_ms.is_none() {
        let silences = db::alerts::list_silences(pool).await?;
        host.state.in_maintenance = in_maintenance(&silences, target_id);
    }
    let rollout = rollout::load(pool).await?;
    let gate = if host.state.requested_ms.is_some() {
        Gate::Open
    } else if rollout.paused {
        Gate::Paused
    } else if rollout.waves.is_empty() {
        if host.state.canary {
            Gate::Open
        } else {
            let hosts = hosts(pool).await?;
            let binaries = binaries(&state.config.agent_dir).await;
            gate(&canaries(&hosts, &binaries, &settings), settings.canary_minutes, now_ms())
        }
    } else {
        let hosts = hosts(pool).await?;
        let binaries = binaries(&state.config.agent_dir).await;
        let plan = wave_plan(&hosts, &binaries, &settings, &rollout, now_ms());
        Gate::from(plan.and_then(|plan| plan.hold_for(target_id)))
    };
    let verdict =
        evaluate(&settings, &host.state, Some(&binary), server_version(), &gate, now_ms());
    if !verdict.offer {
        return Ok(None);
    }
    sqlx::query(
        "UPDATE agent_hosts
         SET update_offered_ms =
                 CASE WHEN update_offered_sha IS ? THEN update_offered_ms ELSE ? END,
             update_offered_sha = ?
         WHERE target_id = ?",
    )
    .bind(&binary.sha256)
    .bind(now_ms())
    .bind(&binary.sha256)
    .bind(target_id)
    .execute(pool)
    .await
    .context("recording the update offer")?;
    Ok(Some(AgentUpdateOffer {
        version: server_version().to_string(),
        build: server_build().map(str::to_string),
        file: binary.file.clone(),
        sha256: binary.sha256.clone(),
        size: Some(binary.size),
        signature: binary.signature.clone(),
        allow_unsigned: settings.allow_unsigned,
        allow_downgrade: verdict.allow_downgrade,
        force: verdict.force,
    }))
}

/// Vrai si l'agent a rendu compte d'une tentative depuis la demande explicite.
fn answered(host: &HostState) -> bool {
    match (host.requested_ms, host.outcome.as_ref()) {
        (Some(requested), Some(outcome)) => outcome.at_ms > requested,
        _ => false,
    }
}

async fn clear_request(pool: &SqlitePool, target_id: TargetId) -> Result<()> {
    sqlx::query(
        "UPDATE agent_hosts SET update_requested_ms = NULL, update_allow_downgrade = 0
         WHERE target_id = ? AND update_requested_ms IS NOT NULL",
    )
    .bind(target_id)
    .execute(pool)
    .await
    .context("closing the update request")?;
    Ok(())
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Réglage propre à une machine.
pub async fn set_host(
    pool: &SqlitePool,
    target_id: TargetId,
    mode: Option<Option<bool>>,
    canary: Option<bool>,
) -> Result<bool> {
    let mut found = false;
    if let Some(mode) = mode {
        let value = mode.map(|on| if on { "on" } else { "off" });
        let result = sqlx::query("UPDATE agent_hosts SET update_mode = ? WHERE target_id = ?")
            .bind(value)
            .bind(target_id)
            .execute(pool)
            .await?;
        found |= result.rows_affected() > 0;
    }
    if let Some(canary) = canary {
        let result = sqlx::query("UPDATE agent_hosts SET update_canary = ? WHERE target_id = ?")
            .bind(i64::from(canary))
            .bind(target_id)
            .execute(pool)
            .await?;
        found |= result.rows_affected() > 0;
    }
    if mode.is_none() && canary.is_none() {
        found = host_row(pool, target_id).await?.is_some();
    }
    Ok(found)
}

/// Modification partielle de la politique de vagues.
#[derive(Debug, Clone, Default)]
pub struct RolloutChange {
    pub waves: Option<Vec<rollout::Wave>>,
    pub delay_minutes: Option<u32>,
    pub max_failure_percent: Option<u32>,
}

/// Change la politique de vagues (champs absents inchangés). Rend le message
/// d'erreur de l'API quand elle ne tient pas.
pub async fn change_rollout(
    pool: &SqlitePool,
    change: RolloutChange,
) -> Result<std::result::Result<(), String>> {
    let mut rollout = rollout::load(pool).await?;
    if let Some(waves) = change.waves {
        rollout.waves = waves;
    }
    if let Some(minutes) = change.delay_minutes {
        rollout.delay_minutes = minutes;
    }
    if let Some(percent) = change.max_failure_percent {
        rollout.max_failure_percent = percent;
    }
    if let Err(message) = rollout.validate() {
        return Ok(Err(message));
    }
    rollout::save(pool, &rollout).await?;
    Ok(Ok(()))
}

/// Pause manuelle : plus aucune offre automatique, « Update now » mis à part.
pub async fn pause_rollout(pool: &SqlitePool) -> Result<()> {
    let mut rollout = rollout::load(pool).await?;
    if !rollout.paused {
        rollout.paused = true;
        rollout::save(pool, &rollout).await?;
    }
    Ok(())
}

/// Reprise, après une pause ou un arrêt sur échec. Reprendre un arrêt pardonne
/// les échecs déjà rapportés : sans cela il se redéclencherait aussitôt.
pub async fn resume_rollout(state: &AppState) -> Result<()> {
    let pool = &state.pool;
    let mut rollout = rollout::load(pool).await?;
    let settings = settings(pool).await?;
    let binaries = binaries(&state.config.agent_dir).await;
    let hosts = hosts(pool).await?;
    let now = now_ms();
    let halted = wave_plan(&hosts, &binaries, &settings, &rollout, now)
        .is_some_and(|plan| plan.halt.is_some());
    if halted {
        rollout.forgiven_before_ms = now;
    }
    if halted || rollout.paused {
        rollout.paused = false;
        rollout::save(pool, &rollout).await?;
    }
    Ok(())
}

/// « Update now » : la machine reçoit l'offre à son prochain lot, hors
/// fenêtre et hors déploiement progressif, et retente une version en échec.
pub async fn request(
    pool: &SqlitePool,
    target_id: TargetId,
    allow_downgrade: bool,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_hosts SET update_requested_ms = ?, update_allow_downgrade = ?
         WHERE target_id = ?",
    )
    .bind(now_ms())
    .bind(i64::from(allow_downgrade))
    .bind(target_id)
    .execute(pool)
    .await
    .context("requesting the agent update")?;
    Ok(result.rows_affected() > 0)
}

/// « Update all now » : chaque machine capable et pas à jour. Rend le nombre
/// de machines concernées.
pub async fn request_all(state: &AppState) -> Result<usize> {
    let binaries = binaries(&state.config.agent_dir).await;
    let mut count = 0;
    for host in hosts(&state.pool).await? {
        if host.state.ability.as_deref() != Some("ready") {
            continue;
        }
        let Some(binary) = host.state.platform.as_deref().and_then(|p| binaries.get(p)) else {
            continue;
        };
        if host.state.up_to_date(binary)
            || dumbmonit_proto::is_older(server_version(), &host.state.agent_version)
        {
            continue;
        }
        if request(&state.pool, host.target_id, false).await? {
            count += 1;
        }
    }
    Ok(count)
}

/// Une machine dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct AgentView {
    pub id: TargetId,
    pub name: String,
    pub hostname: String,
    pub version: String,
    pub platform: Option<String>,
    pub sha256: Option<String>,
    /// `null` : le réglage global ; `true` / `false` : forcé pour la machine.
    pub enabled: Option<bool>,
    pub canary: bool,
    pub requested: bool,
    /// Rang (à partir de 1) de sa vague, si des vagues sont configurées.
    pub wave: Option<usize>,
    pub status: &'static str,
    pub reason: Option<String>,
    pub last: Option<Outcome>,
    pub last_seen_at: Option<String>,
}

/// Un binaire livré, dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct BinaryView {
    pub platform: String,
    pub sha256: String,
    pub signed: bool,
}

/// Une vague, dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct WaveView {
    /// Sa sélection ; `null` : la dernière vague, « tout le reste ».
    pub select: Option<rollout::Wave>,
    pub members: usize,
    pub updated: usize,
    pub failed: usize,
    /// `done` (plus rien à mettre à jour), `active` (ouverte), `waiting`
    /// (la suivante, quand elle n'attend plus que le délai ou la précédente)
    /// ou `upcoming`.
    pub state: &'static str,
}

/// L'arrêt automatique, dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct HaltView {
    /// Rang (à partir de 1) de la vague en cause.
    pub wave: usize,
    pub failed: usize,
    pub attempted: usize,
}

/// La politique de vagues, dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct PolicyView {
    pub waves: Vec<rollout::Wave>,
    pub delay_minutes: u32,
    pub max_failure_percent: u32,
}

/// Feu du déploiement, dans la vue de l'interface.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutView {
    pub canaries: usize,
    pub canaries_updated: usize,
    /// Les machines qui ne sont pas pilotes peuvent se mettre à jour.
    pub open: bool,
    pub reason: Option<String>,
    /// Politique de vagues (aucune : pilotes d'abord).
    pub policy: PolicyView,
    pub paused: bool,
    pub halted: Option<HaltView>,
    pub waves: Vec<WaveView>,
    /// Rang (à partir de 1) de la vague en cours.
    pub current_wave: Option<usize>,
    /// Ouverture de la prochaine vague, quand seul le délai l'en sépare.
    pub next_wave_at: Option<String>,
}

/// `GET /api/agent/updates`
#[derive(Debug, Clone, Serialize)]
pub struct Overview {
    pub settings: Settings,
    pub version: &'static str,
    pub build: Option<&'static str>,
    /// Mises à jour bloquées : au moins un binaire livré n'est pas signé et
    /// les builds non signés ne sont pas acceptés.
    pub blocked_unsigned: bool,
    pub binaries: Vec<BinaryView>,
    pub rollout: RolloutView,
    pub agents: Vec<AgentView>,
}

fn rollout_view(
    rollout: &Rollout,
    plan: Option<&Plan>,
    canaries: &[Canary],
    legacy_gate: &Gate,
) -> RolloutView {
    let (open, reason) = match (plan, legacy_gate) {
        _ if rollout.paused => (false, None),
        (Some(plan), _) => (plan.fully_open(), None),
        (None, Gate::Open) => (true, None),
        (None, Gate::Waiting(why)) => (false, Some(why.clone())),
        (None, _) => (false, None),
    };
    let waves = plan.map_or_else(Vec::new, |plan| {
        let current = plan.current();
        plan.waves
            .iter()
            .enumerate()
            .map(|(k, wave)| WaveView {
                select: wave.wave.clone(),
                members: wave.members.len(),
                updated: wave.done,
                failed: wave.failed,
                state: if wave.pending == 0 && k < plan.open_count {
                    "done"
                } else if current == Some(k) {
                    "active"
                } else if k == plan.open_count && plan.next_at_ms.is_some() {
                    "waiting"
                } else {
                    "upcoming"
                },
            })
            .collect()
    });
    RolloutView {
        canaries: canaries.len(),
        canaries_updated: canaries.iter().filter(|canary| canary.up_to_date).count(),
        open,
        reason,
        policy: PolicyView {
            waves: rollout.waves.clone(),
            delay_minutes: rollout.delay_minutes,
            max_failure_percent: rollout.max_failure_percent,
        },
        paused: rollout.paused,
        halted: plan.and_then(|plan| plan.halt).map(|halt| HaltView {
            wave: halt.wave + 1,
            failed: halt.failed,
            attempted: halt.attempted,
        }),
        waves,
        current_wave: plan.and_then(Plan::current).map(|k| k + 1),
        next_wave_at: plan.and_then(|plan| plan.next_at_ms).and_then(at),
    }
}

pub async fn overview(state: &AppState) -> Result<Overview> {
    let pool = &state.pool;
    let settings = settings(pool).await?;
    let binaries = binaries(&state.config.agent_dir).await;
    let mut hosts = hosts(pool).await?;
    let silences = if settings.window == Window::Maintenance {
        db::alerts::list_silences(pool).await?
    } else {
        Vec::new()
    };
    let canaries = canaries(&hosts, &binaries, &settings);
    let now = now_ms();
    let rollout = rollout::load(pool).await?;
    let plan = wave_plan(&hosts, &binaries, &settings, &rollout, now);
    let legacy_gate = gate(&canaries, settings.canary_minutes, now);
    let mut agents = Vec::with_capacity(hosts.len());
    for host in &mut hosts {
        host.state.in_maintenance = in_maintenance(&silences, host.target_id);
        let binary = host.state.platform.as_deref().and_then(|p| binaries.get(p));
        let host_gate = match &plan {
            _ if rollout.paused => Gate::Paused,
            Some(plan) => Gate::from(plan.hold_for(host.target_id)),
            None => legacy_gate.clone(),
        };
        let verdict = evaluate(&settings, &host.state, binary, server_version(), &host_gate, now);
        agents.push(AgentView {
            id: host.target_id,
            name: host.name.clone(),
            hostname: host.hostname.clone(),
            version: host.state.agent_version.clone(),
            platform: host.state.platform.clone(),
            sha256: host.state.sha256.clone(),
            enabled: host.state.mode,
            canary: host.state.canary,
            requested: host.state.requested_ms.is_some(),
            wave: plan.as_ref().and_then(|plan| plan.wave_of(host.target_id)).map(|k| k + 1),
            status: verdict.status,
            reason: verdict.reason,
            last: host.state.outcome.clone(),
            last_seen_at: host.last_seen_at.clone(),
        });
    }
    Ok(Overview {
        blocked_unsigned: !settings.allow_unsigned
            && binaries.values().any(|binary| binary.signature.is_none()),
        rollout: rollout_view(&rollout, plan.as_ref(), &canaries, &legacy_gate),
        binaries: binaries
            .iter()
            .map(|(platform, binary)| BinaryView {
                platform: platform.clone(),
                sha256: binary.sha256.clone(),
                signed: binary.signature.is_some(),
            })
            .collect(),
        version: server_version(),
        build: server_build(),
        settings,
        agents,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;

    fn binary() -> ShippedBinary {
        ShippedBinary {
            file: "dumbmonit-agent-linux-x86_64".into(),
            sha256: "b".repeat(64),
            size: 10,
            signature: Some("c2ln".into()),
        }
    }

    fn host() -> HostState {
        HostState {
            agent_version: "0.13.5".into(),
            sha256: Some("a".repeat(64)),
            platform: Some("linux-x86_64".into()),
            ability: Some("ready".into()),
            ..HostState::default()
        }
    }

    fn decide(settings: &Settings, host: &HostState, gate: &Gate) -> Verdict {
        evaluate(settings, host, Some(&binary()), "0.14.0", gate, NOW)
    }

    fn failure(outcome: &str, at_ms: i64) -> Option<Outcome> {
        Some(Outcome {
            outcome: outcome.into(),
            sha256: "b".repeat(64),
            version: Some("0.14.0".into()),
            reason: Some("boom".into()),
            at_ms,
            at: None,
        })
    }

    #[test]
    fn an_outdated_agent_gets_the_offer_and_an_up_to_date_one_does_not() {
        let settings = Settings::default();
        let verdict = decide(&settings, &host(), &Gate::Open);
        assert!(verdict.offer && !verdict.force && !verdict.allow_downgrade);
        assert_eq!(verdict.status, "pending");

        let current = HostState { sha256: Some("B".repeat(64)), ..host() };
        assert_eq!(decide(&settings, &current, &Gate::Open).status, "up_to_date");
    }

    #[test]
    fn same_version_other_build_is_an_update() {
        // Serveur :edge : la version ne bouge pas, le build si.
        let edge = HostState { agent_version: "0.14.0".into(), ..host() };
        assert!(decide(&Settings::default(), &edge, &Gate::Open).offer);
    }

    #[test]
    fn a_newer_agent_is_never_downgraded_unless_asked() {
        let newer = HostState { agent_version: "0.15.0".into(), ..host() };
        let verdict = decide(&Settings::default(), &newer, &Gate::Open);
        assert_eq!((verdict.status, verdict.offer), ("newer", false));

        let asked =
            HostState { requested_ms: Some(NOW - 1), allow_downgrade: true, ..newer.clone() };
        let verdict = decide(&Settings::default(), &asked, &Gate::Open);
        assert!(verdict.offer && verdict.allow_downgrade && verdict.force);
        // « Update now » sans demander de retour en arrière : toujours non.
        let plain = HostState { requested_ms: Some(NOW - 1), ..newer };
        assert!(!decide(&Settings::default(), &plain, &Gate::Open).offer);
    }

    #[test]
    fn the_global_switch_and_the_per_agent_override() {
        let off = Settings { enabled: false, ..Settings::default() };
        assert_eq!(decide(&off, &host(), &Gate::Open).status, "disabled");
        let forced_on = HostState { mode: Some(true), ..host() };
        assert!(decide(&off, &forced_on, &Gate::Open).offer);
        let forced_off = HostState { mode: Some(false), ..host() };
        assert_eq!(decide(&Settings::default(), &forced_off, &Gate::Open).status, "disabled");
        // « Update now » passe outre.
        let asked = HostState { requested_ms: Some(NOW), ..forced_off };
        assert!(decide(&Settings::default(), &asked, &Gate::Open).offer);
    }

    #[test]
    fn the_window_holds_the_offer_unless_asked() {
        let manual = Settings { window: Window::Manual, ..Settings::default() };
        assert_eq!(decide(&manual, &host(), &Gate::Open).status, "manual");
        let asked = HostState { requested_ms: Some(NOW), ..host() };
        assert!(decide(&manual, &asked, &Gate::Open).offer);

        let maintenance = Settings { window: Window::Maintenance, ..Settings::default() };
        assert_eq!(decide(&maintenance, &host(), &Gate::Open).status, "waiting_window");
        let silenced = HostState { in_maintenance: true, ..host() };
        assert!(decide(&maintenance, &silenced, &Gate::Open).offer);
    }

    #[test]
    fn canaries_go_first_and_the_others_wait_for_them() {
        let waiting = Gate::Waiting("waiting for 1 canary agent(s) to update".into());
        let verdict = decide(&Settings::default(), &host(), &waiting);
        assert_eq!(verdict.status, "waiting_canary");
        assert!(verdict.reason.unwrap().contains("canary"));
        let canary = HostState { canary: true, ..host() };
        assert!(decide(&Settings::default(), &canary, &waiting).offer);
    }

    #[test]
    fn the_rollout_gate_opens_after_the_canaries_held_for_n_minutes() {
        let canary = |up_to_date: bool, since_ms: Option<i64>, failed: bool| Canary {
            name: "pilote".into(),
            up_to_date,
            since_ms,
            failed,
        };
        assert_eq!(gate(&[], 60, NOW), Gate::Open, "sans pilote : tout le monde");
        assert!(matches!(gate(&[canary(false, None, false)], 60, NOW), Gate::Waiting(_)));
        let fresh = canary(true, Some(NOW - 10 * 60_000), false);
        assert!(matches!(gate(&[fresh], 60, NOW), Gate::Waiting(why) if why.contains("follow")));
        let held = canary(true, Some(NOW - 61 * 60_000), false);
        assert_eq!(gate(std::slice::from_ref(&held), 60, NOW), Gate::Open);
        let failed = canary(false, None, true);
        let blocked = gate(&[held, failed], 60, NOW);
        assert!(matches!(blocked, Gate::Waiting(why) if why.contains("failed")));
    }

    #[test]
    fn a_failed_version_is_not_offered_again_until_asked() {
        let failed = HostState { outcome: failure("rolled_back", NOW - 5), ..host() };
        let verdict = decide(&Settings::default(), &failed, &Gate::Open);
        assert_eq!((verdict.status, verdict.offer), ("failed", false));
        assert_eq!(verdict.reason.as_deref(), Some("boom"));

        let asked = HostState { requested_ms: Some(NOW - 1), ..failed.clone() };
        let verdict = decide(&Settings::default(), &asked, &Gate::Open);
        assert!(verdict.offer && verdict.force, "demande explicite : une nouvelle tentative");

        // Une autre version (autre empreinte) est proposée normalement.
        let other = HostState {
            outcome: Some(Outcome { sha256: "c".repeat(64), ..failed.outcome.clone().unwrap() }),
            ..failed
        };
        assert!(decide(&Settings::default(), &other, &Gate::Open).offer);
    }

    #[test]
    fn a_refusal_is_reconsidered_when_the_unsigned_setting_changes() {
        let refused = HostState { outcome: failure("refused", NOW - 100), ..host() };
        assert_eq!(decide(&Settings::default(), &refused, &Gate::Open).status, "refused");
        let changed = Settings { allow_unsigned: true, reset_ms: NOW - 50, ..Settings::default() };
        assert!(decide(&changed, &refused, &Gate::Open).offer);
        // Un échec réel, lui, attend une demande explicite.
        let failed = HostState { outcome: failure("failed", NOW - 100), ..host() };
        assert_eq!(decide(&changed, &failed, &Gate::Open).status, "failed");
    }

    #[test]
    fn an_unsigned_build_is_held_and_says_so_unless_unsigned_builds_are_accepted() {
        let unsigned = ShippedBinary { signature: None, ..binary() };
        let held =
            evaluate(&Settings::default(), &host(), Some(&unsigned), "0.14.0", &Gate::Open, NOW);
        assert_eq!((held.status, held.offer), ("unsigned", false));
        // Même un « Update now » : l'agent le refuserait.
        let asked = HostState { requested_ms: Some(NOW), ..host() };
        let held =
            evaluate(&Settings::default(), &asked, Some(&unsigned), "0.14.0", &Gate::Open, NOW);
        assert_eq!(held.status, "unsigned");
        let accepted = Settings { allow_unsigned: true, ..Settings::default() };
        assert!(evaluate(&accepted, &host(), Some(&unsigned), "0.14.0", &Gate::Open, NOW).offer);
    }

    #[test]
    fn agents_that_cannot_update_themselves_say_why() {
        for (ability, status) in [
            (None, "legacy"),
            (Some("container"), "container"),
            (Some("read_only"), "read_only"),
            (Some("disabled"), "disabled_locally"),
            (Some("unsupported"), "unsupported"),
        ] {
            let stuck = HostState { ability: ability.map(str::to_string), ..host() };
            let verdict = decide(&Settings::default(), &stuck, &Gate::Open);
            assert_eq!((verdict.status, verdict.offer), (status, false));
        }
        let no_binary = evaluate(&Settings::default(), &host(), None, "0.14.0", &Gate::Open, NOW);
        assert_eq!(no_binary.status, "no_binary");
    }

    #[test]
    fn a_recent_offer_shows_as_updating() {
        let offered = HostState {
            offered_sha: Some("b".repeat(64)),
            offered_ms: Some(NOW - 60_000),
            ..host()
        };
        assert_eq!(decide(&Settings::default(), &offered, &Gate::Open).status, "updating");
        let stale = HostState { offered_ms: Some(NOW - UPDATING_FOR_MS - 1), ..offered };
        assert_eq!(decide(&Settings::default(), &stale, &Gate::Open).status, "pending");
    }

    #[test]
    fn waves_pause_and_halt_hold_the_offer_unless_asked() {
        let cases = [
            (Gate::Wave, "waiting_wave"),
            (Gate::Paused, "paused"),
            (Gate::Halted, "halted"),
        ];
        for (gate, status) in cases {
            let verdict = decide(&Settings::default(), &host(), &gate);
            assert_eq!((verdict.status, verdict.offer), (status, false));
            let asked = HostState { requested_ms: Some(NOW - 1), ..host() };
            assert!(decide(&Settings::default(), &asked, &gate).offer, "Update now passe outre");
        }
    }

    fn row(id: TargetId, state: HostState) -> HostRow {
        HostRow {
            target_id: id,
            name: format!("agent-{id}"),
            hostname: format!("agent-{id}"),
            last_seen_at: at(NOW),
            site: None,
            group: String::new(),
            tags: BTreeMap::new(),
            state,
        }
    }

    fn shipped() -> Binaries {
        BTreeMap::from([("linux-x86_64".to_string(), binary())])
    }

    #[test]
    fn only_agents_that_can_and_want_to_update_are_ordered_in_waves() {
        let policy = Rollout::default();
        let mut paris = row(1, host());
        paris.site = Some("Paris".into());
        paris.group = "prod".into();
        paris.tags.insert("env".into(), "prod".into());
        let pinned = row(2, HostState { mode: Some(false), ..host() });
        let container = row(3, HostState { ability: Some("container".into()), ..host() });
        let mut gone = row(4, host());
        gone.last_seen_at = at(NOW - ABANDONED_AFTER_MS - 1);
        let running = HostState { sha256: Some("b".repeat(64)), sha_since_ms: Some(7), ..host() };
        let current = row(5, running);
        let newer = row(6, HostState { agent_version: "99.0.0".into(), ..host() });
        let failed = row(7, HostState { outcome: failure("rolled_back", NOW - 5), ..host() });
        let forgiven = row(8, HostState { outcome: failure("failed", NOW - 5), ..host() });
        let pilot = row(9, HostState { canary: true, ..host() });
        let hosts = vec![paris, pinned, container, gone, current, newer, failed, forgiven, pilot];

        let members = rollout_members(&hosts, &shipped(), &Settings::default(), &policy, NOW);
        let by_id = |id: TargetId| members.iter().find(|member| member.id == id);
        for excluded in [2, 3, 4] {
            assert!(by_id(excluded).is_none(), "agent {excluded} ne fait partie d'aucune vague");
        }
        let paris = by_id(1).expect("membre");
        assert_eq!(paris.state, MemberState::Pending);
        assert_eq!(paris.site.as_deref(), Some("Paris"));
        assert_eq!(paris.group, "prod");
        assert_eq!(paris.tags.get("env").map(String::as_str), Some("prod"));
        assert_eq!(by_id(5).unwrap().state, MemberState::Done { since_ms: 7 });
        assert_eq!(by_id(6).unwrap().state, MemberState::Idle, "plus récent que le serveur");
        assert_eq!(by_id(7).unwrap().state, MemberState::Failed);
        assert_eq!(by_id(8).unwrap().state, MemberState::Failed);
        assert!(by_id(9).unwrap().canary);

        let forgiving = Rollout { forgiven_before_ms: NOW, ..Rollout::default() };
        let members = rollout_members(&hosts, &shipped(), &Settings::default(), &forgiving, NOW);
        let state = |id: TargetId| members.iter().find(|member| member.id == id).unwrap().state;
        assert_eq!(state(8), MemberState::Idle, "un échec pardonné ne compte plus");
        assert_eq!(state(7), MemberState::Idle);
    }

    #[test]
    fn a_failing_wave_stops_the_rollout_and_resuming_forgives_it() {
        let policy = Rollout {
            waves: vec![rollout::Wave::Count { value: 2 }],
            delay_minutes: 0,
            max_failure_percent: 50,
            ..Rollout::default()
        };
        let settings = Settings::default();
        let mut hosts: Vec<HostRow> = (1..=6).map(|id| row(id, host())).collect();
        let first = wave_plan(&hosts, &shipped(), &settings, &policy, NOW).expect("des vagues");
        assert_eq!(first.open_count, 1);
        let (failing, other) = (first.waves[0].members[0], first.waves[0].members[1]);
        let outsider = first.waves[1].members[0];
        assert_eq!(first.hold_for(outsider), Some(Hold::Wave));
        assert_eq!(first.hold_for(other), None);

        // Un des deux canaris échoue : 1 sur 1 tentative, au-dessus de 50 %.
        for host in &mut hosts {
            if host.target_id == failing {
                host.state.outcome = failure("rolled_back", NOW - 5);
            }
        }
        let stopped = wave_plan(&hosts, &shipped(), &settings, &policy, NOW).unwrap();
        assert!(stopped.halt.is_some());
        let verdict = |plan: &Plan, id: TargetId| {
            let state = &hosts.iter().find(|host| host.target_id == id).unwrap().state;
            let gate = Gate::from(plan.hold_for(id));
            evaluate(&settings, state, Some(&binary()), "0.14.0", &gate, NOW)
        };
        for id in [other, outsider] {
            assert_eq!(verdict(&stopped, id).status, "halted");
        }
        assert_eq!(verdict(&stopped, failing).status, "failed", "son propre échec reste dit");

        // Reprise : l'échec est pardonné, le canari restant est de nouveau
        // offert, la deuxième vague attend toujours le canari.
        let resumed = Rollout { forgiven_before_ms: NOW, ..policy };
        let plan = wave_plan(&hosts, &shipped(), &settings, &resumed, NOW).unwrap();
        assert!(plan.halt.is_none());
        assert!(verdict(&plan, other).offer);
        assert_eq!(verdict(&plan, outsider).status, "waiting_wave");
    }
}
