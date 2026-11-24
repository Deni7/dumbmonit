//! `GET /api/overview/widgets?range=1h|24h|7d|30d` : les chiffres en plus des
//! tableaux de bord de la vue d'ensemble (commande, NOC, hybride).
//!
//! Le reste de ces pages vient des routes existantes (alertes, silences, SLO,
//! échéances, pages de statut). La forme de la réponse est celle de
//! `web/src/lib/api/types/overview.ts` (`OverviewWidgets`), champ pour champ.
//!
//! Chaque partie se dégrade seule : sans VictoriaMetrics, les parties tirées de
//! SQLite (sites, alertes, agents, évènements) restent servies et les parties
//! métriques rendent des listes vides — jamais un 500 pour une pièce absente.
//! Un compte restreint par ses équipes ne voit que ses moniteurs, y compris
//! dans les requêtes de séries (filtre `extra_filters[]`).

mod agents;
mod alerts;
mod availability;
mod backups;
mod category;
mod events;
pub mod model;
mod services;
mod sites;
mod top;
mod vm;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use crate::alerting::model::TargetId;
use crate::api::{ApiError, ApiResult};
use crate::auth::teams::{self, TeamAccess};
use crate::state::AppState;

use model::OverviewWidgets;

/// Fenêtre des sparklines, des p95 et des séries du « top ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Range {
    Hour,
    Day,
    Week,
    Month,
}

impl Range {
    pub fn parse(raw: Option<&str>) -> Option<Self> {
        match raw.map(str::trim).filter(|raw| !raw.is_empty()) {
            None | Some("24h") => Some(Self::Day),
            Some("1h") => Some(Self::Hour),
            Some("7d") => Some(Self::Week),
            Some("30d") => Some(Self::Month),
            Some(_) => None,
        }
    }

    pub fn seconds(self) -> i64 {
        match self {
            Self::Hour => 3_600,
            Self::Day => 86_400,
            Self::Week => 7 * 86_400,
            Self::Month => 30 * 86_400,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct WidgetsQuery {
    #[serde(default)]
    pub range: Option<String>,
}

/// Ce que toutes les parties partagent : l'instant, la fenêtre, les moniteurs
/// visibles (et leur site), le filtre des séries.
struct Context {
    pub now: DateTime<Utc>,
    pub range: Range,
    /// `None` : aucune restriction d'équipe.
    pub visible: Option<HashSet<TargetId>>,
    /// Filtre `extra_filters[]` des requêtes de séries, `None` sans restriction.
    pub filter: Option<String>,
    pub fleet: sites::Fleet,
    pub vm: vm::Health,
}

impl Context {
    /// Vrai si l'équipement est visible du compte.
    pub fn sees(&self, id: TargetId) -> bool {
        self.visible.as_ref().is_none_or(|visible| visible.contains(&id))
    }
}

pub async fn get(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Query(query): Query<WidgetsQuery>,
) -> ApiResult<Json<OverviewWidgets>> {
    let Some(range) = Range::parse(query.range.as_deref()) else {
        return Err(ApiError::BadRequest(format!(
            "\"range\" must be 1h, 24h, 7d or 30d (received: \"{}\").",
            query.range.unwrap_or_default()
        )));
    };
    let now = Utc::now();
    let visible = access.visible_targets(&state.pool).await?;
    let filter = teams::series_filter(visible.as_ref());
    let vm = vm::Health::default();
    let fleet = sites::load(&state, visible.as_ref(), filter.as_deref(), &vm, now).await?;
    let ctx = Context { now, range, visible, filter, fleet, vm };

    let (top, services, availability, backups, alert_part, agents, silences) = tokio::join!(
        top::load(&state, &ctx),
        services::load(&state, &ctx),
        availability::load(&state, &ctx),
        backups::load(&state, &ctx),
        alerts::load(&state, &ctx),
        agents::load(&state, &ctx),
        sites::maintenance_silences(&state, &access),
    );
    let silences = silences.unwrap_or_else(|error| {
        tracing::warn!(?error, "overview widgets: maintenance windows not read");
        Vec::new()
    });
    let alert_part = alert_part.unwrap_or_else(|error| {
        tracing::warn!(?error, "overview widgets: alert history not read");
        alerts::AlertPart::default()
    });
    let agents = agents.unwrap_or_else(|error| {
        tracing::warn!(?error, "overview widgets: agents not read");
        agents::AgentPart::default()
    });

    let mut all_events = alert_part.events;
    all_events.extend(agents.events);
    all_events.extend(events::monitors_added(&ctx));
    all_events.extend(events::maintenance(&silences, &ctx));
    let events = events::newest(all_events, events::MAX_EVENTS);

    let sites = sites::build(&ctx, &silences, &backups);

    Ok(Json(OverviewWidgets {
        sites,
        top,
        backups,
        services,
        alert_stats: alert_part.stats,
        agents: agents.summary,
        events,
        availability,
        alerts_opened: alert_part.opened,
    }))
}

/// Horodatage tel que l'API le rend : UTC, sans suffixe.
pub(super) fn stamp(at: DateTime<Utc>) -> String {
    crate::db::timestamp(at)
}

/// Lit un horodatage de la base, avec ou sans suffixe (`…Z`, `+00:00`, ou
/// `AAAA-MM-JJ HH:MM:SS` en UTC).
pub(super) fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(at) = DateTime::parse_from_rfc3339(raw) {
        return Some(at.with_timezone(&Utc));
    }
    ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(raw, format).ok())
        .map(|at| at.and_utc())
}

/// Arrondi à `digits` décimales, pour des réponses lisibles.
pub(super) fn round(value: f64, digits: i32) -> f64 {
    let factor = 10f64.powi(digits);
    (value * factor).round() / factor
}
