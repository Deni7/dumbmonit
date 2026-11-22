//! Incidents plus anciens d'une page publique : la liste des « incidents
//! passés » s'arrête à trente jours, le visiteur en demande davantage.
//!
//! `GET /api/public/status/<slug>/incidents` renvoie une tranche d'annonces
//! terminées, de la plus récente à la plus ancienne. Le curseur `before`
//! (`<fin>_<id>`) est celui que la réponse précédente a rendu dans `next` ;
//! sans curseur, la tranche commence juste avant la fenêtre de trente jours
//! déjà dans le document de la page.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use chrono::{TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use super::{PAST_INCIDENTS_DAYS, PublicIncident, access, format_timestamp, to_public_incident};
use crate::api::{ApiError, ApiResult};
use crate::db;
use crate::db::status_pages::IncidentUpdate;
use crate::state::AppState;

/// Taille d'une tranche, et plafond de ce qu'un client peut demander.
const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 50;

#[derive(Debug, Deserialize)]
pub struct OlderQuery {
    /// Curseur `<fin>_<id>` rendu par la tranche précédente.
    #[serde(default)]
    before: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
struct Older {
    incidents: Vec<PublicIncident>,
    /// Curseur de la tranche suivante ; absent quand il n'y a plus rien.
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<String>,
}

/// Lit un curseur `<fin>_<id>`.
fn parse_cursor(raw: &str) -> Option<(String, i64)> {
    let (end, id) = raw.rsplit_once('_')?;
    let id = id.parse().ok()?;
    (!end.is_empty()).then(|| (end.to_string(), id))
}

fn cursor_of(end: &str, id: i64) -> String {
    format!("{end}_{id}")
}

pub async fn older(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OlderQuery>,
) -> ApiResult<Response> {
    let page = match db::status_pages::get_page_by_slug(&state.pool, &slug).await? {
        Some(page) if page.published => page,
        _ => return Err(ApiError::NotFound("This status page does not exist.".into())),
    };
    access::authorize(&state, &page, &headers).await?;
    let private = !page.is_public();

    let (before_end, before_id) = match query.before.as_deref().map(str::trim) {
        Some(raw) if !raw.is_empty() => parse_cursor(raw)
            .ok_or_else(|| ApiError::BadRequest("before is not a valid cursor.".into()))?,
        _ => (format_timestamp(Utc::now() - TimeDelta::days(PAST_INCIDENTS_DAYS)), i64::MAX),
    };
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let mut rows = db::status_pages::older_incidents_for_page(
        &state.pool,
        page.id,
        &before_end,
        before_id,
        limit + 1,
    )
    .await?;
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let next = match (more, rows.last()) {
        (true, Some(last)) => last.ends_at.as_deref().map(|end| cursor_of(end, last.id)),
        _ => None,
    };

    let ids: Vec<i64> = rows.iter().map(|incident| incident.id).collect();
    let mut updates: HashMap<i64, Vec<IncidentUpdate>> = HashMap::new();
    for update in db::status_pages::list_updates_for(&state.pool, &ids).await? {
        updates.entry(update.incident_id).or_default().push(update);
    }
    let mut postmortems = db::status_pages::published_postmortems(&state.pool).await?;
    let incidents = rows
        .into_iter()
        .map(|incident| {
            let updates = updates.remove(&incident.id).unwrap_or_default();
            let postmortem = postmortems.remove(&incident.id);
            to_public_incident(incident, updates, postmortem, Vec::new())
        })
        .collect();

    Ok(access::seal(
        ([(header::CACHE_CONTROL, "public, max-age=60")], Json(Older { incidents, next }))
            .into_response(),
        private,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cursor_round_trips() {
        let cursor = cursor_of("2026-09-15T10:00:00Z", 42);
        assert_eq!(parse_cursor(&cursor), Some(("2026-09-15T10:00:00Z".to_string(), 42)));
        assert_eq!(parse_cursor("nope"), None);
        assert_eq!(parse_cursor("_3"), None);
        assert_eq!(parse_cursor("2026-09-15T10:00:00Z_x"), None);
    }
}
