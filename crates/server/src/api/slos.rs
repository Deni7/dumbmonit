//! Objectifs de niveau de service (SLO) : CRUD et état (disponibilité
//! atteinte, budget d'erreur, vitesses de consommation).
//!
//! Lecture ouverte à tous les rôles, écriture aux administrateurs (le garde de
//! session s'en charge). Les mesures viennent de VictoriaMetrics : s'il ne
//! répond pas, la liste s'affiche quand même, avec `status.measured = false`.

use std::collections::BTreeMap;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::reports::layout::Scope;
use crate::slo::{self, MAX_SLOS, MonitorRef, Slo, SloInput, SloStatus, measure, store};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/slos", get(list).post(create))
        .route("/slos/{id}", get(get_one).put(update).delete(remove))
}

/// Un SLO et son état.
#[derive(Debug, Serialize)]
pub struct SloView {
    #[serde(flatten)]
    pub slo: Slo,
    pub status: SloStatus,
}

/// Un SLO, son état, les moniteurs qu'il couvre et la courbe de son budget.
#[derive(Debug, Serialize)]
pub struct SloDetail {
    #[serde(flatten)]
    pub view: SloView,
    /// Moniteurs actifs couverts.
    pub members: Vec<i64>,
    /// `[secondes, % du budget restant]` sur la fenêtre ; vide sans mesure.
    pub burndown: Vec<[f64; 2]>,
}

/// SLO soumis. `description`, `scope` et `enabled` sont facultatifs.
#[derive(Debug, Deserialize)]
pub struct SloPayload {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub objective: f64,
    pub window_days: u32,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

fn not_found(id: i64) -> ApiError {
    ApiError::NotFound(format!("SLO {id} not found."))
}

async fn validate(state: &AppState, payload: SloPayload, id: Option<i64>) -> ApiResult<SloInput> {
    let input = SloInput {
        name: payload.name,
        description: payload.description,
        objective: payload.objective,
        window_days: payload.window_days,
        scope: payload.scope,
        enabled: payload.enabled,
    }
    .validate()
    .map_err(ApiError::BadRequest)?;
    if store::name_taken(&state.pool, &input.name, id).await? {
        return Err(ApiError::Conflict(format!("An SLO named \"{}\" already exists.", input.name)));
    }
    Ok(input)
}

/// État de chaque SLO, mesuré en une passe : toutes les fenêtres pour
/// l'union des moniteurs.
async fn views(state: &AppState, slos: Vec<Slo>, monitors: &[MonitorRef]) -> Vec<SloView> {
    let mut union: BTreeMap<i64, &MonitorRef> = BTreeMap::new();
    let mut windows: Vec<i64> = slo::BURN_WINDOWS.to_vec();
    for item in &slos {
        for member in slo::members(&item.scope, monitors) {
            union.insert(member.id, member);
        }
        windows.push(i64::from(item.window_days) * 86_400);
    }
    windows.sort_unstable();
    windows.dedup();
    let all: Vec<&MonitorRef> = union.into_values().collect();
    let measures = match measure::availability(&state.victoria, &all, &windows, Utc::now()).await {
        Ok(measures) => Some(measures),
        Err(error) => {
            tracing::warn!(%error, "SLO availability unavailable");
            None
        }
    };
    slos.into_iter()
        .map(|item| {
            let covered = slo::members(&item.scope, monitors);
            let status = slo::status_of(&item, &covered, measures.as_ref());
            SloView { slo: item, status }
        })
        .collect()
}

pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<SloView>>> {
    let slos = store::list(&state.pool).await?;
    let monitors = store::monitors(&state.pool).await?;
    Ok(Json(views(&state, slos, &monitors).await))
}

async fn detail(state: &AppState, id: i64) -> ApiResult<SloDetail> {
    let item = store::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    let monitors = store::monitors(&state.pool).await?;
    let covered = slo::members(&item.scope, &monitors);
    let members: Vec<i64> = covered.iter().map(|m| m.id).collect();
    let burndown = match measure::burndown_curve(
        &state.victoria,
        &covered,
        item.objective,
        item.window_days,
        Utc::now(),
    )
    .await
    {
        Ok(curve) => curve,
        Err(error) => {
            tracing::warn!(%error, slo = id, "SLO budget curve unavailable");
            Vec::new()
        }
    };
    let mut views = views(state, vec![item], &monitors).await;
    Ok(SloDetail { view: views.remove(0), members, burndown })
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<SloDetail>> {
    Ok(Json(detail(&state, id).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Json(payload): Json<SloPayload>,
) -> ApiResult<(StatusCode, Json<SloDetail>)> {
    let input = validate(&state, payload, None).await?;
    if store::count(&state.pool).await? >= MAX_SLOS {
        return Err(ApiError::Conflict(format!("At most {MAX_SLOS} SLOs are allowed.")));
    }
    let id = store::insert(&state.pool, &input).await?;
    tracing::info!(slo = id, name = %input.name, "SLO created");
    Ok((StatusCode::CREATED, Json(detail(&state, id).await?)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<SloPayload>,
) -> ApiResult<Json<SloDetail>> {
    store::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    let input = validate(&state, payload, Some(id)).await?;
    if !store::update(&state.pool, id, &input).await? {
        return Err(not_found(id));
    }
    Ok(Json(detail(&state, id).await?))
}

pub async fn remove(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if store::delete(&state.pool, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found(id))
    }
}
