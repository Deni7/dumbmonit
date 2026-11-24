//! Routes HTTP du moteur d'alerting : règles, alertes actives, historique, silences.
//!
//! Le module n'abrite aucune logique d'alerting : il traduit du JSON en types du
//! domaine, valide ce que l'utilisateur a saisi, puis délègue à
//! [`crate::db::alerts`]. Les vues de sortie sont des types distincts des types
//! internes, afin qu'une évolution du moteur ne change pas silencieusement le
//! contrat de l'interface web.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::alerting::machine::{EffectivePhase, Phase};
use crate::alerting::model::{
    AnomalyParams, DriftDirection, Operator, RULE_HOST_DOWN, Rule, RuleKind, Severity,
    TargetSelector,
};
use crate::alerting::overrides::RuleOverride;
use crate::alerting::silence::{
    LAST_OF_MONTH, MAX_DURATION_MINUTES, MINUTES_PER_DAY, Schedule, Silence,
};
use crate::api::channels;
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::clients;
use crate::auth::middleware::{AdminIdentity, OperatorIdentity};
use crate::auth::teams::{Access, Level, TeamAccess};
use crate::db;
use crate::notify::policy_store;
use crate::state::AppState;

/// Longueur maximale d'un identifiant stable de règle.
///
/// L'`uid` sert de préfixe lisible dans les empreintes et dans les journaux : le
/// borner garde ces derniers exploitables à l'œil nu.
const MAX_UID_LEN: usize = 64;

/// Profondeur d'historique renvoyée quand l'appelant ne précise rien.
const DEFAULT_HISTORY_DAYS: i64 = 7;

/// Nombre maximal de transitions renvoyées en une fois.
///
/// Au-delà, la réponse pèse plus lourd que ce que l'interface sait afficher ; le
/// client affine avec `since` plutôt que de tout rapatrier.
const MAX_HISTORY_LIMIT: i64 = 5_000;
const DEFAULT_HISTORY_LIMIT: i64 = 500;

/// Décalage horaire acceptable pour un silence hebdomadaire, en minutes.
///
/// Les fuseaux réels vont de UTC−12 à UTC+14 ; hors de cette plage, c'est une
/// erreur de saisie et le silence couvrirait une autre tranche horaire que celle
/// que l'utilisateur croit avoir posée.
const MIN_UTC_OFFSET: i32 = -12 * 60;
const MAX_UTC_OFFSET: i32 = 14 * 60;

/// Nombre maximal de jours ou de « n-ièmes jours » dans une fenêtre mensuelle.
/// Au-delà, ce n'est plus une maintenance mensuelle, c'est un calendrier.
const MAX_MONTHLY_ENTRIES: usize = 31;

/// Durée d'un acquittement quand l'appelant ne précise rien : quatre heures,
/// le temps de s'en occuper sans que le rappel du soir ne se perde.
pub const DEFAULT_ACK_SECS: i64 = 4 * 3600;

/// Durée maximale d'un acquittement : trente jours. « Jusqu'à résolution »
/// dans l'interface, c'est cela ; au-delà, c'est une règle à retoucher, pas une
/// alerte à faire taire.
pub const MAX_ACK_SECS: i64 = 30 * 24 * 3600;

/// Longueur maximale de la note d'acquittement.
const MAX_ACK_NOTE_LEN: usize = 200;

// --------------------------------------------------------------------------
// Vues
// --------------------------------------------------------------------------

/// Règle telle que l'API la renvoie.
///
/// Les durées sont exposées en secondes plutôt qu'en [`Duration`] : c'est ce que
/// manipulent les formulaires de l'interface, et cela évite une représentation
/// JSON `{secs, nanos}` que personne n'attend.
#[derive(Debug, Serialize)]
pub struct RuleView {
    pub id: i64,
    pub uid: String,
    pub name: String,
    pub description: String,
    pub kind: RuleKind,
    pub query: String,
    pub operator: Operator,
    pub threshold: f64,
    /// Seuil de retour au calme (hystérésis), `null` sans.
    pub clear_threshold: Option<f64>,
    pub for_secs: u64,
    pub severity: Severity,
    pub selector: TargetSelector,
    pub channels: Vec<i64>,
    pub params: AnomalyParams,
    pub unit: String,
    pub repeat_secs: Option<u64>,
    pub escalate_after_secs: Option<u64>,
    pub enabled: bool,
    pub builtin: bool,
    /// Surcharges par équipement de cette règle.
    pub overrides: Vec<RuleOverride>,
    /// Dernière fois que la règle est passée en alerte (RFC 3339, UTC), d'après
    /// l'historique ; `null` si elle n'a jamais déclenché sur la période conservée.
    pub last_fired_at: Option<String>,
}

impl From<Rule> for RuleView {
    fn from(rule: Rule) -> Self {
        Self {
            id: rule.id,
            uid: rule.uid,
            name: rule.name,
            description: rule.description,
            kind: rule.kind,
            query: rule.query,
            operator: rule.operator,
            threshold: rule.threshold,
            clear_threshold: rule.clear_threshold,
            for_secs: rule.for_duration.as_secs(),
            severity: rule.severity,
            selector: rule.selector,
            channels: rule.channels,
            params: rule.params,
            unit: rule.unit,
            repeat_secs: rule.repeat_interval.map(|d| d.as_secs()),
            escalate_after_secs: rule.escalate_after.map(|d| d.as_secs()),
            enabled: rule.enabled,
            builtin: rule.builtin,
            overrides: Vec::new(),
            last_fired_at: None,
        }
    }
}

/// Complète des vues de règles avec leurs surcharges par équipement.
async fn with_overrides(
    pool: &sqlx::SqlitePool,
    mut views: Vec<RuleView>,
) -> ApiResult<Vec<RuleView>> {
    let overrides = policy_store::list_overrides(pool, None, None).await?;
    let fired = db::alerts::last_fired_by_rule(pool).await?;
    for view in &mut views {
        view.overrides = overrides.iter().filter(|o| o.rule_uid == view.uid).cloned().collect();
        view.last_fired_at = fired.get(&view.uid).cloned();
    }
    Ok(views)
}

async fn view_with_overrides(pool: &sqlx::SqlitePool, rule: Rule) -> ApiResult<RuleView> {
    let mut views = with_overrides(pool, vec![RuleView::from(rule)]).await?;
    Ok(views.remove(0))
}

/// Surcharge soumise par l'interface. Chaque champ absent ou `null` laisse la
/// valeur de la règle s'appliquer ; une surcharge entièrement vide est effacée.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OverridePayload {
    pub threshold: Option<f64>,
    pub clear_threshold: Option<f64>,
    pub enabled: Option<bool>,
}

impl OverridePayload {
    /// Le seul geste de surcharge ouvert à un opérateur : `enabled: false`, sans
    /// seuil.
    pub fn is_ignore(&self) -> bool {
        self.enabled == Some(false) && self.threshold.is_none() && self.clear_threshold.is_none()
    }
}

#[derive(Debug, Deserialize)]
pub struct OverridesQuery {
    #[serde(default)]
    pub target_id: Option<i64>,
}

/// Alerte active, telle que l'interface doit l'afficher.
///
/// `phase` et `effective_phase` sont toutes deux exposées : la première dit où en
/// est la machine à états, la seconde ce que l'utilisateur doit lire. Une alerte
/// `firing` mais `suppressed` reste `firing` pour le moteur — c'est bien ce qui
/// permet de ne pas la renotifier à la sortie de la suppression —, alors qu'elle
/// doit s'afficher comme supprimée, avec l'équipement responsable en regard.
#[derive(Debug, Serialize)]
pub struct ActiveAlertView {
    pub fingerprint: String,
    pub rule_uid: String,
    /// Nom courant de la règle, repris de sa définition. Vide si la règle a été
    /// supprimée entre-temps : l'état survit jusqu'à la purge du cycle suivant.
    pub rule_name: String,
    /// Sévérité déclarée par la règle. L'escalade éventuelle ne relève que la
    /// sévérité *notifiée* et n'est pas persistée ici.
    pub severity: Severity,
    pub target_id: Option<i64>,
    pub series_key: String,
    pub labels: BTreeMap<String, String>,
    pub phase: Phase,
    pub effective_phase: EffectivePhase,
    pub suppressed: bool,
    /// Identifiant de l'équipement injoignable qui masque cette alerte.
    pub suppressed_by: Option<i64>,
    pub silenced: bool,
    pub learning: bool,
    /// Vrai tant que l'acquittement court : plus de rappel, la résolution reste
    /// annoncée. Calculé à la lecture — un acquittement échu n'est pas effacé
    /// tout de suite, il a simplement cessé d'agir.
    pub acked: bool,
    pub acked_until: Option<DateTime<Utc>>,
    /// Compte qui a acquitté, ou `token:nom` pour un jeton d'API.
    pub acked_by: Option<String>,
    pub ack_note: Option<String>,
    /// Vrai quand l'occurrence en cours a été écartée (« j'ai compris ») : plus
    /// aucune notification, résolution comprise, et l'alerte sort des vues
    /// actives. `GET /alerts` ne la renvoie qu'avec `?dismissed=true`.
    pub dismissed: bool,
    pub dismissed_at: Option<DateTime<Utc>>,
    /// Compte qui l'a écartée, ou `token:nom` pour un jeton d'API.
    pub dismissed_by: Option<String>,
    pub value: Option<f64>,
    pub score: Option<f64>,
    pub condition_since: Option<DateTime<Utc>>,
    pub firing_since: Option<DateTime<Utc>>,
    pub last_eval_at: Option<DateTime<Utc>>,
    pub last_notified_at: Option<DateTime<Utc>>,
    pub notify_count: u32,
    /// Tickets ouverts pour cette alerte par les canaux Jira, GLPI, Zammad,
    /// GitHub ou GitLab (vide sans canal de tickets).
    pub tickets: Vec<crate::notify::tickets::TicketLink>,
    /// Le repère de changement du même équipement qui précède le
    /// déclenchement de moins de 30 minutes (« après le changement : … »).
    pub after_change: Option<crate::annotations::AfterChange>,
}

impl ActiveAlertView {
    fn new(
        alert: crate::alerting::cycle::StoredAlert,
        rule: Option<&Rule>,
        now: DateTime<Utc>,
    ) -> Self {
        let dismissed = alert.state.is_dismissed();
        Self {
            effective_phase: alert.state.effective_phase(),
            phase: alert.state.phase,
            rule_name: rule.map(|rule| rule.name.clone()).unwrap_or_default(),
            severity: rule.map_or(Severity::Warning, |rule| rule.severity),
            fingerprint: alert.fingerprint,
            rule_uid: alert.rule_uid,
            target_id: alert.target_id,
            series_key: alert.series_key,
            labels: alert.labels,
            suppressed: alert.state.suppressed,
            suppressed_by: alert.state.suppressed_by,
            silenced: alert.state.silenced,
            learning: alert.state.learning,
            acked: alert.state.is_acked(now),
            acked_until: alert.state.acked_until,
            acked_by: alert.state.acked_by,
            ack_note: alert.state.ack_note,
            dismissed,
            dismissed_at: alert.state.dismissed_at,
            dismissed_by: alert.state.dismissed_by,
            value: alert.state.value,
            score: alert.state.score,
            condition_since: alert.state.condition_since,
            firing_since: alert.state.firing_since,
            last_eval_at: alert.state.last_eval_at,
            last_notified_at: alert.state.last_notified_at,
            notify_count: alert.state.notify_count,
            tickets: Vec::new(),
            after_change: None,
        }
    }
}

/// Transition consignée dans l'historique.
#[derive(Debug, Serialize)]
pub struct HistoryEntryView {
    pub id: i64,
    pub fingerprint: String,
    pub rule_uid: String,
    pub target_id: Option<i64>,
    pub from_phase: Phase,
    pub to_phase: Phase,
    pub severity: Severity,
    pub value: Option<f64>,
    /// Faux quand la transition n'a donné lieu à aucun envoi ; `reason` dit alors
    /// pourquoi (apprentissage, suppression, fenêtre de maintenance).
    pub notified: bool,
    pub reason: String,
    /// Horodatage RFC 3339 en UTC, tel que consigné.
    pub at: String,
    /// Vrai une fois « effacée » par l'utilisateur : la ligne reste en base (c'est
    /// la mémoire de l'instance) mais ne doit plus apparaître dans une vue par
    /// défaut. Seules les transitions vers `resolved` peuvent l'être.
    pub dismissed: bool,
}

/// Fenêtre de maintenance telle que l'API la renvoie.
#[derive(Debug, Serialize)]
pub struct SilenceView {
    pub id: i64,
    pub name: String,
    pub comment: String,
    pub target_id: Option<i64>,
    pub matchers: BTreeMap<String, String>,
    pub schedule: Schedule,
    pub enabled: bool,
    /// Vrai si la fenêtre couvre l'instant présent. Calculé ici pour que
    /// l'interface n'ait pas à réimplémenter le calendrier hebdomadaire, dont les
    /// fenêtres à cheval sur minuit sont la partie délicate.
    pub active_now: bool,
    /// Fin de l'occurrence en cours, quand il y en a une (RFC 3339).
    pub active_until: Option<String>,
    /// Début de la prochaine occurrence (RFC 3339), `null` s'il n'y en a plus.
    /// Le calendrier mensuel — « cinquième dimanche » — ne se devine pas côté
    /// interface : c'est le serveur qui le déroule.
    pub next_start_at: Option<String>,
}

impl SilenceView {
    fn new(silence: Silence, now: DateTime<Utc>) -> Self {
        let window = silence.schedule.window_containing(now);
        // `covers` plutôt que la fenêtre : un silence « jusqu'au rétablissement »
        // est actif sans avoir de fin à annoncer.
        let active_now = silence.enabled && silence.schedule.covers(now);
        let next_start_at = silence.schedule.next_start(now).map(|at| at.to_rfc3339());
        Self {
            id: silence.id,
            name: silence.name,
            comment: silence.comment,
            target_id: silence.target_id,
            matchers: silence.matchers,
            schedule: silence.schedule,
            enabled: silence.enabled,
            active_now,
            active_until: window.filter(|_| active_now).map(|(_, end)| end.to_rfc3339()),
            next_start_at,
        }
    }
}

// --------------------------------------------------------------------------
// Entrées
// --------------------------------------------------------------------------

/// Règle soumise par l'interface.
///
/// Les énumérations arrivent en chaînes libres plutôt que typées par serde : les
/// `parse` du domaine retombent silencieusement sur une valeur par défaut, ce qui
/// est le bon comportement pour relire la base mais le pire pour un formulaire —
/// une sévérité mal orthographiée deviendrait `warning` sans que personne ne le
/// sache. La validation ci-dessous les refuse explicitement.
#[derive(Debug, Deserialize)]
pub struct RulePayload {
    /// Identifiant stable. Absent à la création, il est dérivé du nom.
    #[serde(default)]
    pub uid: Option<String>,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    pub query: String,
    #[serde(default)]
    pub operator: Option<String>,
    #[serde(default)]
    pub threshold: Option<f64>,
    /// Seuil de retour au calme. Absent ou `null` : pas d'hystérésis.
    #[serde(default)]
    pub clear_threshold: Option<f64>,
    /// Signé volontairement : une valeur négative doit produire un message clair
    /// plutôt qu'un refus de désérialisation en anglais.
    #[serde(default)]
    pub for_secs: Option<i64>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub selector: Option<Value>,
    #[serde(default)]
    pub channels: Option<Vec<i64>>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub unit: Option<String>,
    /// Absent ou nul : aucun rappel. Zéro est traité de même.
    #[serde(default)]
    pub repeat_secs: Option<i64>,
    #[serde(default)]
    pub escalate_after_secs: Option<i64>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Réglages d'anomalie, chaque champ retombant sur la valeur du domaine.
///
/// L'interface n'expose qu'un ou deux de ces réglages selon l'écran ; exiger
/// l'objet complet la forcerait à recopier des constantes qui ne lui appartiennent
/// pas.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AnomalyParamsPayload {
    k: Option<f64>,
    alpha: Option<f64>,
    mad_floor_abs: Option<f64>,
    mad_floor_rel: Option<f64>,
    min_samples: Option<u32>,
    /// Règle « motif de journal » : fenêtre de comptage (secondes). L'alias est le
    /// nom sous lequel la règle est renvoyée : un `PUT` qui la renvoie telle quelle
    /// ne la remet pas à zéro.
    #[serde(alias = "log_window_secs")]
    window_secs: Option<u64>,
    /// Règle « motif de journal » : une alerte par cible, ou une seule.
    #[serde(alias = "log_per_target")]
    per_target: Option<bool>,
    forecast_window_days: Option<u32>,
    forecast_warning_days: Option<f64>,
    forecast_critical_days: Option<f64>,
    forecast_min_confidence: Option<f64>,
    forecast_limit: Option<f64>,
    drift_window_secs: Option<u64>,
    drift_weeks: Option<u32>,
    drift_min_samples: Option<u32>,
    drift_min_value: Option<f64>,
    drift_min_delta: Option<f64>,
    drift_direction: Option<DriftDirection>,
}

#[derive(Debug, Deserialize)]
pub struct EnablePayload {
    pub enabled: bool,
}

/// Acquittement soumis par l'interface ou l'assistant.
///
/// `until` et `duration_secs` s'excluent ; sans l'un ni l'autre, quatre heures.
/// Un `until` explicitement `null` (et pas de durée) lève l'acquittement, comme
/// le `DELETE` — d'où le double `Option` : absent et `null` ne disent pas la
/// même chose.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AckPayload {
    /// Fin de l'acquittement, RFC 3339 ou `YYYY-MM-DD HH:MM:SS` en UTC.
    #[serde(deserialize_with = "deserialize_explicit_null")]
    pub until: Option<Option<String>>,
    pub duration_secs: Option<i64>,
    pub note: Option<String>,
}

/// Distingue un champ absent (`None`) d'un champ à `null` (`Some(None)`).
fn deserialize_explicit_null<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Ce que l'acquittement doit devenir, une fois la saisie validée.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckRequest {
    /// Acquitter jusqu'à cet instant, avec une note éventuelle.
    Set { until: DateTime<Utc>, note: Option<String> },
    /// Lever l'acquittement.
    Clear,
}

impl AckPayload {
    pub fn validate(self, now: DateTime<Utc>) -> ApiResult<AckRequest> {
        let note = self.note.map(|note| note.trim().to_string()).filter(|note| !note.is_empty());
        if note.as_ref().is_some_and(|note| note.chars().count() > MAX_ACK_NOTE_LEN) {
            return Err(ApiError::BadRequest(format!(
                "\"note\" is limited to {MAX_ACK_NOTE_LEN} characters."
            )));
        }
        let until = match (self.until, self.duration_secs) {
            (Some(Some(_)), Some(_)) => {
                return Err(ApiError::BadRequest(
                    "Give either \"until\" or \"duration_secs\", not both.".into(),
                ));
            }
            (Some(None), None) => return Ok(AckRequest::Clear),
            (Some(Some(raw)), None) => parse_until(raw.trim())?,
            (_, Some(secs)) => {
                if secs <= 0 {
                    return Err(ApiError::BadRequest(
                        "\"duration_secs\" must be a strictly positive integer.".into(),
                    ));
                }
                now + TimeDelta::seconds(secs)
            }
            (None, None) => now + TimeDelta::seconds(DEFAULT_ACK_SECS),
        };
        if until <= now {
            return Err(ApiError::BadRequest("\"until\" must be in the future.".into()));
        }
        if until > now + TimeDelta::seconds(MAX_ACK_SECS) {
            return Err(ApiError::BadRequest(format!(
                "An alert can be acknowledged for at most {} days.",
                MAX_ACK_SECS / 86_400
            )));
        }
        Ok(AckRequest::Set { until, note })
    }
}

/// Lit une échéance : RFC 3339, sinon `YYYY-MM-DD HH:MM:SS` lu en UTC — la
/// forme sans suffixe des horodatages du serveur.
fn parse_until(raw: &str) -> ApiResult<DateTime<Utc>> {
    if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        return Ok(at.with_timezone(&Utc));
    }
    let naive = chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S"))
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M"))
        .map_err(|_| {
            ApiError::BadRequest(format!(
                "\"until\" must be an RFC 3339 date or a UTC timestamp like \
                 2026-08-31 12:00:00 (received: \"{raw}\")."
            ))
        })?;
    Ok(naive.and_utc())
}

/// Fenêtre de maintenance soumise par l'interface.
#[derive(Debug, Deserialize)]
pub struct SilencePayload {
    pub name: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub target_id: Option<i64>,
    #[serde(default)]
    pub matchers: BTreeMap<String, String>,
    /// Reçu en JSON brut pour que sa forme soit refusée en français plutôt que par
    /// le message de désérialisation de serde.
    pub schedule: Value,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// Filtres de `GET /api/alerts`.
#[derive(Debug, Default, Deserialize)]
pub struct ActiveQuery {
    /// Ne garde que les alertes de cet équipement.
    #[serde(default)]
    pub target_id: Option<i64>,
    /// `true` : seulement les alertes écartées (« j'ai compris ») encore en
    /// cours, pour les rétablir. Par défaut, elles sont exclues : la liste, la
    /// vue d'ensemble, le mur et les compteurs ne les voient plus.
    #[serde(default)]
    pub dismissed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Borne basse, au format RFC 3339. Par défaut, les sept derniers jours.
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    /// Faux par défaut : les lignes « effacées » (résolues, puis dégagées de la
    /// vue) sont tues, exactement ce qu'une vue active doit montrer. L'historique
    /// complet reste accessible en le passant à vrai — rien n'est jamais détruit
    /// par un « clear ».
    #[serde(default)]
    pub dismissed: Option<bool>,
}

mod active;
mod rules;
mod silences;

pub use active::*;
pub use rules::*;
pub use silences::*;

#[cfg(test)]
mod tests;
