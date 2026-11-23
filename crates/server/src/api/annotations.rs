//! Repères de changement : `POST/GET /annotations`, `PATCH/DELETE
//! /annotations/{id}` (voir [`crate::annotations`]).
//!
//! La lecture est ouverte à tous les rôles. L'écriture l'est aux
//! administrateurs, aux opérateurs et aux jetons `write` (le cas d'un
//! pipeline CI qui documente un déploiement). Un compte restreint par ses
//! équipes ne voit que les repères globaux et ceux qui touchent un moniteur
//! visible ; il n'écrit que des repères dont chaque moniteur concerné est à
//! son niveau `operate` (jamais un repère global).

use std::collections::HashSet;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, patch};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Deserializer};

use crate::annotations::{
    Annotation, MAX_ANNOTATIONS, RawDraft, Scope, TargetInfo, parse_time, store,
};
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::{OperatorIdentity, Principal};
use crate::auth::teams::{Access, Level, TeamAccess};
use crate::state::AppState;

/// Repères rendus au plus par une lecture.
const MAX_LIST: usize = 500;
/// Lignes lues avant le filtrage par moniteur.
const SCAN: i64 = 5000;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/annotations", get(list).post(create))
        .route("/annotations/{id}", patch(update).delete(remove))
}

/// Repère soumis.
#[derive(Debug, Deserialize)]
pub struct CreatePayload {
    pub title: String,
    /// Absent : maintenant.
    #[serde(default)]
    pub at: Option<String>,
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub source: Option<String>,
}

/// Distingue un champ absent (`None`) d'un champ à `null` (`Some(None)`).
fn nullable<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(de).map(Some)
}

/// Modification partielle : seuls les champs présents changent ; `until: null`
/// retire la fenêtre.
#[derive(Debug, Deserialize)]
pub struct UpdatePayload {
    pub title: Option<String>,
    pub at: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub until: Option<Option<String>>,
    pub text: Option<String>,
    pub link: Option<String>,
    pub tags: Option<Vec<String>>,
    pub scope: Option<Scope>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub from: Option<String>,
    pub to: Option<String>,
    /// Ne garde que les repères qui concernent ce moniteur.
    pub target: Option<i64>,
    pub limit: Option<usize>,
}

fn not_found(id: i64) -> ApiError {
    ApiError::NotFound(format!("Marker {id} not found."))
}

fn bad_date(error: String) -> ApiError {
    ApiError::BadRequest(error)
}

/// Vrai si un compte restreint peut voir ce repère. Appelé pour chacun des
/// repères lus : on s'arrête au premier moniteur visible, sans construire
/// l'ensemble des moniteurs couverts.
fn visible_to_scoped(scope: &Scope, all: &[TargetInfo], visible: &HashSet<i64>) -> bool {
    matches!(scope, Scope::Global)
        || all.iter().any(|target| visible.contains(&target.id) && scope.matches(target))
}

/// Un compte restreint n'écrit qu'un repère dont chaque moniteur concerné est à
/// son niveau `operate`, et qui en concerne au moins un.
async fn require_write(
    state: &AppState,
    access: &Access,
    scope: &Scope,
    all: &[TargetInfo],
) -> ApiResult<()> {
    let Some(operable) = access.targets_at(&state.pool, Level::Operate).await? else {
        return Ok(());
    };
    let covered = scope.covered(all);
    if !matches!(scope, Scope::Global) && !covered.is_empty() && covered.is_subset(&operable) {
        return Ok(());
    }
    Err(ApiError::Forbidden(
        "Your teams only let you add markers to monitors you can handle, not to the whole \
         instance."
            .into(),
    ))
}

/// Chaque moniteur nommé existe : un repère ne vise pas dans le vide.
fn require_existing(scope: &Scope, all: &[TargetInfo]) -> ApiResult<()> {
    if let Scope::Monitors { targets } = scope
        && let Some(missing) = targets.iter().find(|id| !all.iter().any(|t| t.id == **id))
    {
        return Err(ApiError::BadRequest(format!("Monitor {missing} does not exist.")));
    }
    Ok(())
}

/// Un repère visible du compte : 404 sinon.
async fn visible_annotation(
    state: &AppState,
    access: &Access,
    id: i64,
    all: &[TargetInfo],
) -> ApiResult<Annotation> {
    let found = store::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    if let Some(visible) = access.visible_targets(&state.pool).await?
        && !visible_to_scoped(&found.scope, all, &visible)
    {
        return Err(not_found(id));
    }
    Ok(found)
}

fn default_source(principal: &Principal) -> &'static str {
    match principal {
        Principal::User(_) => "ui",
        Principal::Token(_) => "api",
    }
}

pub async fn list(
    State(state): State<AppState>,
    TeamAccess(access): TeamAccess,
    Query(query): Query<ListQuery>,
) -> ApiResult<Json<Vec<Annotation>>> {
    let from = query.from.as_deref().map(parse_time).transpose().map_err(bad_date)?;
    let to = query.to.as_deref().map(parse_time).transpose().map_err(bad_date)?;
    let limit = query.limit.unwrap_or(MAX_LIST).clamp(1, MAX_LIST);
    let all = store::target_infos(&state.pool).await?;
    let wanted = match query.target {
        Some(id) => {
            Some(all.iter().find(|t| t.id == id).ok_or_else(|| ApiError::monitor_not_found(&id))?)
        }
        None => None,
    };
    let visible = access.visible_targets(&state.pool).await?;
    if let (Some(visible), Some(target)) = (&visible, wanted)
        && !visible.contains(&target.id)
    {
        return Err(ApiError::monitor_not_found(&target.id));
    }
    let mut found = store::list(&state.pool, from, to, SCAN).await?;
    found.retain(|item| {
        wanted.is_none_or(|target| item.scope.matches(target))
            && visible.as_ref().is_none_or(|visible| visible_to_scoped(&item.scope, &all, visible))
    });
    let skipped = found.len().saturating_sub(limit);
    Ok(Json(found.split_off(skipped)))
}

pub async fn create(
    State(state): State<AppState>,
    TeamAccess(access): TeamAccess,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Json(payload): Json<CreatePayload>,
) -> ApiResult<(StatusCode, Json<Annotation>)> {
    let raw = RawDraft {
        at: payload.at,
        until: payload.until,
        title: payload.title,
        text: payload.text,
        link: payload.link,
        tags: payload.tags,
        scope: payload.scope,
        source: payload.source,
    };
    let draft =
        raw.validate(Utc::now(), default_source(&principal)).map_err(ApiError::BadRequest)?;
    let all = store::target_infos(&state.pool).await?;
    require_existing(&draft.scope, &all)?;
    require_write(&state, &access, &draft.scope, &all).await?;
    if store::count(&state.pool).await? >= MAX_ANNOTATIONS {
        return Err(ApiError::Conflict(format!(
            "At most {MAX_ANNOTATIONS} markers are kept: delete old ones first."
        )));
    }
    let author = principal.label();
    let id = store::insert(&state.pool, &draft, &author).await?;
    audit::record(&state.pool, Some(&author), "annotation.created", Some(&id.to_string()), ip)
        .await;
    let created = store::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn update(
    State(state): State<AppState>,
    TeamAccess(access): TeamAccess,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePayload>,
) -> ApiResult<Json<Annotation>> {
    let all = store::target_infos(&state.pool).await?;
    let existing = visible_annotation(&state, &access, id, &all).await?;
    require_write(&state, &access, &existing.scope, &all).await?;
    let raw = RawDraft {
        at: Some(payload.at.unwrap_or(existing.at)),
        until: match payload.until {
            Some(until) => until,
            None => existing.until,
        },
        title: payload.title.unwrap_or(existing.title),
        text: payload.text.unwrap_or(existing.text),
        link: payload.link.unwrap_or(existing.link),
        tags: payload.tags.unwrap_or(existing.tags),
        scope: payload.scope.unwrap_or(existing.scope),
        source: Some(existing.source),
    };
    let draft = raw.validate(Utc::now(), "api").map_err(ApiError::BadRequest)?;
    require_existing(&draft.scope, &all)?;
    require_write(&state, &access, &draft.scope, &all).await?;
    if !store::update(&state.pool, id, &draft).await? {
        return Err(not_found(id));
    }
    let author = principal.label();
    audit::record(&state.pool, Some(&author), "annotation.updated", Some(&id.to_string()), ip)
        .await;
    Ok(Json(store::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?))
}

pub async fn remove(
    State(state): State<AppState>,
    TeamAccess(access): TeamAccess,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let all = store::target_infos(&state.pool).await?;
    let existing = visible_annotation(&state, &access, id, &all).await?;
    require_write(&state, &access, &existing.scope, &all).await?;
    if !store::delete(&state.pool, id).await? {
        return Err(not_found(id));
    }
    let author = principal.label();
    audit::record(&state.pool, Some(&author), "annotation.deleted", Some(&id.to_string()), ip)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn info(id: i64, folder: &str) -> TargetInfo {
        TargetInfo { id, folder: folder.into(), tags: BTreeMap::new() }
    }

    #[test]
    fn a_scoped_account_sees_markers_that_touch_one_visible_monitor() {
        let all = vec![info(1, "Lyon"), info(2, "Paris"), info(3, "Paris")];
        let visible = HashSet::from([2]);
        assert!(visible_to_scoped(&Scope::Global, &all, &visible));
        let paris = Scope::Folder { folder: "Paris".into() };
        assert!(visible_to_scoped(&paris, &all, &visible));
        let lyon = Scope::Folder { folder: "Lyon".into() };
        assert!(!visible_to_scoped(&lyon, &all, &visible));
        assert!(visible_to_scoped(&Scope::Monitors { targets: vec![1, 2] }, &all, &visible));
        assert!(!visible_to_scoped(&Scope::Monitors { targets: vec![1, 3] }, &all, &visible));
        // Un moniteur supprimé ne rend rien visible.
        assert!(!visible_to_scoped(&Scope::Monitors { targets: vec![9] }, &all, &visible));
    }
}
