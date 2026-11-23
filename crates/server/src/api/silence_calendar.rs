//! `GET /api/alerts/silences/occurrences` : les fenêtres de maintenance déroulées
//! en occurrences datées, pour le calendrier de l'interface. Le serveur est le
//! seul à savoir dérouler « le dernier vendredi du mois » ou une nuit de
//! changement d'heure ; l'interface se contente de dessiner.

use axum::Json;
use axum::extract::{Query, State};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::api::alerts::visible_silences;
use crate::api::{ApiError, ApiResult};
use crate::auth::teams::TeamAccess;
use crate::state::AppState;

/// Étendue maximale demandée : un mois affiché, ses jours voisins, avec de la marge.
const MAX_SPAN: TimeDelta = TimeDelta::days(100);
/// Occurrences renvoyées au plus, toutes fenêtres confondues.
const MAX_OCCURRENCES: usize = 2000;

#[derive(Debug, Deserialize)]
pub struct RangeQuery {
    /// RFC 3339, inclusive.
    pub from: String,
    /// RFC 3339, exclusive.
    pub to: String,
}

/// État d'une occurrence par rapport à l'instant présent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceState {
    Active,
    Upcoming,
    Past,
}

#[derive(Debug, Serialize)]
pub struct OccurrenceView {
    pub silence_id: i64,
    pub name: String,
    pub starts_at: String,
    pub ends_at: String,
    pub state: OccurrenceState,
    /// Vrai pour une règle hebdomadaire ou mensuelle.
    pub recurring: bool,
    pub enabled: bool,
}

fn parse(value: &str, name: &str) -> ApiResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|at| at.with_timezone(&Utc))
        .map_err(|_| ApiError::BadRequest(format!("`{name}` must be an RFC 3339 timestamp")))
}

pub fn state_of(start: DateTime<Utc>, end: DateTime<Utc>, now: DateTime<Utc>) -> OccurrenceState {
    if now >= end {
        OccurrenceState::Past
    } else if now >= start {
        OccurrenceState::Active
    } else {
        OccurrenceState::Upcoming
    }
}

pub async fn occurrences(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Query(range): Query<RangeQuery>,
) -> ApiResult<Json<Vec<OccurrenceView>>> {
    let from = parse(&range.from, "from")?;
    let to = parse(&range.to, "to")?;
    if to <= from {
        return Err(ApiError::BadRequest("`to` must be after `from`".into()));
    }
    if to - from > MAX_SPAN {
        return Err(ApiError::BadRequest("range too long (100 days at most)".into()));
    }
    let now = Utc::now();
    let mut out = Vec::new();
    for silence in visible_silences(&access, &state).await? {
        let room = MAX_OCCURRENCES.saturating_sub(out.len());
        if room == 0 {
            break;
        }
        let recurring = matches!(
            silence.schedule,
            crate::alerting::Schedule::Weekly { .. } | crate::alerting::Schedule::Monthly { .. }
        );
        for (start, end) in silence.schedule.occurrences_between(from, to, room) {
            out.push(OccurrenceView {
                silence_id: silence.id,
                name: silence.name.clone(),
                starts_at: start.to_rfc3339(),
                ends_at: end.to_rfc3339(),
                state: state_of(start, end, now),
                recurring,
                enabled: silence.enabled,
            });
        }
    }
    out.sort_by(|a, b| a.starts_at.cmp(&b.starts_at).then(a.silence_id.cmp(&b.silence_id)));
    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).expect("valid ISO").with_timezone(&Utc)
    }

    #[test]
    fn l_etat_suit_l_instant_present() {
        let (s, e) = (at("2026-11-10T10:00:00Z"), at("2026-11-10T11:00:00Z"));
        assert_eq!(state_of(s, e, at("2026-11-10T09:00:00Z")), OccurrenceState::Upcoming);
        assert_eq!(state_of(s, e, at("2026-11-10T10:30:00Z")), OccurrenceState::Active);
        assert_eq!(state_of(s, e, at("2026-11-10T11:00:00Z")), OccurrenceState::Past);
    }
}
