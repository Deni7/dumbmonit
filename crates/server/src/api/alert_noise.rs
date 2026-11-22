//! `GET /alerts/noise` : le rapport de bruit des alertes.
//!
//! Sur les 7, 30 ou 90 derniers jours de l'historique : les règles qui ont le
//! plus déclenché et notifié, les alertes qui battent, et les délais moyens
//! d'acquittement (MTTA) et de résolution (MTTR), en tout, par règle et par
//! équipe. Un compte restreint par ses équipes ne voit que les épisodes de ses
//! équipements et les lignes de ses équipes.
//!
//! Réservé aux administrateurs et aux opérateurs : c'est un outil pour qui
//! règle les alertes ou y répond. Un jeton d'API le lit avec la portée `read`.

use std::collections::{HashMap, HashSet};

use axum::Json;
use axum::extract::{Query, State};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::alerting::model::TargetId;
use crate::alerting::noise::{self, NoiseStats, TeamTargets};
use crate::alerting::replay;
use crate::api::{ApiError, ApiResult};
use crate::auth::middleware::{Identity, Principal};
use crate::auth::teams::{self, Access, Team, TeamAccess};
use crate::db::{self, alert_insights};
use crate::state::AppState;

/// Fenêtres proposées, en jours.
pub const WINDOWS_DAYS: [i64; 3] = [7, 30, 90];
const DEFAULT_DAYS: i64 = 30;
/// Règles listées au plus, les plus bruyantes d'abord.
const MAX_RULES: usize = 20;
/// Alertes qui battent listées au plus.
const MAX_FLAPS: usize = 10;
/// Déclenchements à partir desquels une alerte est dite « qui bat ».
const MIN_FIRINGS: u32 = 3;
/// Épisodes lus au plus : au-delà, le rapport porte sur les plus récents et le dit.
const MAX_EPISODES: usize = 100_000;

#[derive(Debug, Default, Deserialize)]
pub struct NoiseQuery {
    #[serde(default)]
    pub days: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct RuleNoiseView {
    pub rule_uid: String,
    /// `None` : la règle a été supprimée depuis.
    pub rule_id: Option<i64>,
    pub rule_name: String,
    /// La règle se rejoue sur l'historique (« tester sur l'historique »).
    pub replayable: bool,
    #[serde(flatten)]
    pub stats: NoiseStats,
}

#[derive(Debug, Serialize)]
pub struct TeamNoiseView {
    pub team_id: i64,
    pub team_name: String,
    #[serde(flatten)]
    pub stats: NoiseStats,
}

#[derive(Debug, Serialize)]
pub struct FlappingView {
    pub fingerprint: String,
    pub rule_uid: String,
    pub rule_id: Option<i64>,
    pub rule_name: String,
    pub target_id: Option<TargetId>,
    /// `None` : série sans équipement, ou équipement supprimé.
    pub target_name: Option<String>,
    /// Transitions vers `firing` ou `resolved` dans la fenêtre.
    pub changes: u32,
    pub firings: u32,
}

#[derive(Debug, Serialize)]
pub struct NoiseView {
    pub days: i64,
    /// Début de la fenêtre, UTC sans suffixe.
    pub since: String,
    /// Vrai si la fenêtre comptait plus d'épisodes que le rapport n'en lit :
    /// il porte alors sur les plus récents.
    pub truncated: bool,
    pub overall: NoiseStats,
    pub rules: Vec<RuleNoiseView>,
    pub flapping: Vec<FlappingView>,
    pub teams: Vec<TeamNoiseView>,
}

pub async fn noise(
    Identity(principal): Identity,
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Query(query): Query<NoiseQuery>,
) -> ApiResult<Json<NoiseView>> {
    if let Principal::User(user) = &principal
        && !user.role.can_operate()
    {
        return Err(ApiError::Forbidden(
            "The alert noise report is for operators and admins.".into(),
        ));
    }
    let days = query.days.unwrap_or(DEFAULT_DAYS);
    if !WINDOWS_DAYS.contains(&days) {
        return Err(ApiError::BadRequest(format!(
            "\"days\" must be 7, 30 or 90 (received: {days})."
        )));
    }
    let pool = &state.pool;
    let since = Utc::now() - TimeDelta::days(days);
    let visible = access.visible_targets(pool).await?;
    let scope = visible.as_ref();

    let (episodes, cut) = alert_insights::load_episodes(pool, since, scope, MAX_EPISODES).await?;
    let flaps = alert_insights::load_flapping(pool, since, scope, MIN_FIRINGS, MAX_FLAPS).await?;
    let teams = team_targets(pool, &access, scope).await?;
    let report = noise::summarize(&episodes, &teams, MAX_RULES);

    let rules: HashMap<String, (i64, String, bool)> = db::alerts::list_rules(pool)
        .await?
        .into_iter()
        .map(|rule| {
            let replayable = replay::not_replayable(rule.kind).is_none();
            (rule.uid, (rule.id, rule.name, replayable))
        })
        .collect();
    let rule_of = |uid: &str| match rules.get(uid) {
        Some((id, name, replayable)) => (Some(*id), name.clone(), *replayable),
        None => (None, uid.to_string(), false),
    };
    let target_names: HashMap<TargetId, String> = db::alerts::list_target_nodes(pool)
        .await?
        .into_iter()
        .map(|node| (node.id, node.name))
        .collect();

    Ok(Json(NoiseView {
        days,
        since: db::timestamp(since),
        truncated: cut,
        overall: report.overall,
        rules: report
            .rules
            .into_iter()
            .map(|rule| {
                let (rule_id, rule_name, replayable) = rule_of(&rule.rule_uid);
                RuleNoiseView {
                    rule_uid: rule.rule_uid,
                    rule_id,
                    rule_name,
                    replayable,
                    stats: rule.stats,
                }
            })
            .collect(),
        flapping: flaps
            .into_iter()
            .map(|flap| {
                let (rule_id, rule_name, _) = rule_of(&flap.rule_uid);
                FlappingView {
                    target_name: flap.target_id.and_then(|id| target_names.get(&id).cloned()),
                    fingerprint: flap.fingerprint,
                    rule_uid: flap.rule_uid,
                    rule_id,
                    rule_name,
                    target_id: flap.target_id,
                    changes: flap.changes,
                    firings: flap.firings,
                }
            })
            .collect(),
        teams: report
            .teams
            .into_iter()
            .map(|team| TeamNoiseView {
                team_id: team.team_id,
                team_name: team.team_name,
                stats: team.stats,
            })
            .collect(),
    }))
}

/// Les équipes à présenter et les équipements que leurs droits couvrent :
/// toutes pour un compte sans restriction, les siennes seulement sinon, et
/// jamais au-delà des équipements visibles.
async fn team_targets(
    pool: &sqlx::SqlitePool,
    access: &Access,
    visible: Option<&HashSet<TargetId>>,
) -> ApiResult<Vec<TeamTargets>> {
    let own: Option<&[String]> =
        access.scope().filter(|_| access.is_scoped()).map(|scope| scope.teams.as_slice());
    let folders: Vec<(TargetId, String)> = sqlx::query_as("SELECT id, group_name FROM targets")
        .fetch_all(pool)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(teams::list(pool)
        .await?
        .into_iter()
        .filter(|team| own.is_none_or(|own| own.contains(&team.name)))
        .map(|team| TeamTargets {
            targets: folders
                .iter()
                .filter(|(id, folder)| {
                    visible.is_none_or(|visible| visible.contains(id)) && covers(&team, folder)
                })
                .map(|(id, _)| *id)
                .collect(),
            id: team.id,
            name: team.name,
        })
        .collect())
}

/// Vrai si un droit de l'équipe couvre un équipement rangé dans `folder`.
fn covers(team: &Team, folder: &str) -> bool {
    team.grants.iter().any(|grant| teams::folder_covers(&grant.folder, folder))
}
