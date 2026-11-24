//! Destinations hors site des sauvegardes locales (Settings → Backup).
//!
//! Mêmes règles que les autres routes `/backup` : un administrateur connecté
//! dans l'interface, jamais un jeton d'API. Les secrets n'en ressortent jamais
//! (`has_secret`, `saved_secrets`) : absents d'un `PUT`, ceux enregistrés
//! restent ; `secrets: {}` les efface. Une destination qui change de serveur
//! ou de compte exige qu'on les saisisse de nouveau : ils partiraient sinon
//! vers un serveur qu'ils n'ont jamais connu.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Extension, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::backup::admin;
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::CurrentPrincipal;
use crate::backup::offsite::{self, Kind, Secrets, Settings, TestOutcome, store};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/backup/destinations", get(list).post(create))
        .route("/backup/destinations/{id}", put(update).delete(remove))
        .route("/backup/destinations/{id}/test", post(test))
}

#[derive(Debug, Serialize)]
pub struct DestinationList {
    pub destinations: Vec<store::DestinationView>,
    /// Vrai pendant qu'un envoi est en cours : l'interface rafraîchit.
    pub uploading: bool,
}

#[derive(Debug, Deserialize)]
pub struct DestinationPayload {
    pub name: String,
    pub kind: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub keep: Option<u32>,
    /// Envoyer aussi `secret.key`. Faux par défaut.
    #[serde(default)]
    pub include_key: bool,
    #[serde(default)]
    pub settings: Value,
    /// Absent ou `null` à la modification : les secrets enregistrés restent.
    /// `{}` les efface.
    #[serde(default)]
    pub secrets: Option<Secrets>,
}

fn enabled_by_default() -> bool {
    true
}

struct Checked {
    name: String,
    keep: u32,
    settings: Settings,
    secrets: Option<Secrets>,
}

fn check(payload: &DestinationPayload) -> ApiResult<Checked> {
    let name = payload.name.trim().to_string();
    if name.is_empty()
        || name.chars().count() > offsite::MAX_NAME_LEN
        || name.chars().any(char::is_control)
    {
        return Err(ApiError::BadRequest(format!(
            "Give the destination a name of 1 to {} characters.",
            offsite::MAX_NAME_LEN
        )));
    }
    let kind = Kind::parse(&payload.kind).ok_or_else(|| {
        ApiError::BadRequest(format!(
            "Unknown destination kind {:?}: s3, webdav or sftp.",
            payload.kind
        ))
    })?;
    let settings = Settings::parse(kind, &payload.settings).map_err(ApiError::BadRequest)?;
    settings.validate().map_err(ApiError::BadRequest)?;
    let keep = payload.keep.unwrap_or(offsite::DEFAULT_KEEP);
    if keep == 0 || keep > offsite::MAX_KEEP {
        return Err(ApiError::BadRequest(format!(
            "The number of copies to keep must be between 1 and {}.",
            offsite::MAX_KEEP
        )));
    }
    let secrets = payload.secrets.clone().map(offsite::clean_secrets);
    Ok(Checked { name, keep, settings, secrets })
}

async fn one(state: &AppState, id: i64) -> ApiResult<store::Destination> {
    store::get(&state.pool, id)
        .await?
        .ok_or_else(|| ApiError::NotFound(format!("No backup destination {id}.")))
}

async fn name_taken(state: &AppState, name: &str, except: Option<i64>) -> ApiResult<()> {
    match store::id_by_name(&state.pool, name).await? {
        Some(other) if Some(other) != except => Err(ApiError::Conflict(format!(
            "A backup destination named \"{name}\" already exists."
        ))),
        _ => Ok(()),
    }
}

pub async fn list(
    State(state): State<AppState>,
    Extension(CurrentPrincipal(principal)): Extension<CurrentPrincipal>,
) -> ApiResult<Json<DestinationList>> {
    admin(&principal)?;
    let destinations =
        store::list(&state.pool).await?.iter().map(store::Destination::view).collect();
    Ok(Json(DestinationList { destinations, uploading: offsite::uploading() }))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(CurrentPrincipal(principal)): Extension<CurrentPrincipal>,
    ClientIp(ip): ClientIp,
    Json(payload): Json<DestinationPayload>,
) -> ApiResult<(StatusCode, Json<store::DestinationView>)> {
    let me = admin(&principal)?.username.clone();
    let checked = check(&payload)?;
    let secrets = checked.secrets.clone().unwrap_or_default();
    checked.settings.check_secrets(&secrets).map_err(ApiError::BadRequest)?;
    name_taken(&state, &checked.name, None).await?;
    let input = store::Input {
        name: &checked.name,
        enabled: payload.enabled,
        keep: checked.keep,
        include_key: payload.include_key,
        settings: &checked.settings,
    };
    let id = store::insert(&state.pool, &state.cipher, &input, &secrets).await?;
    audit::record(&state.pool, Some(&me), "backup.destination_created", Some(&checked.name), ip)
        .await;
    Ok((StatusCode::CREATED, Json(one(&state, id).await?.view())))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(CurrentPrincipal(principal)): Extension<CurrentPrincipal>,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(payload): Json<DestinationPayload>,
) -> ApiResult<Json<store::DestinationView>> {
    let me = admin(&principal)?.username.clone();
    let existing = one(&state, id).await?;
    let mut checked = check(&payload)?;

    // Un autre serveur SFTP : l'empreinte confirmée pour l'ancien ne vaut pas
    // pour lui, l'administrateur confirmera la sienne.
    if let (Settings::Sftp(old), Settings::Sftp(new)) = (&existing.settings, &mut checked.settings)
        && (old.host != new.host || old.port != new.port)
        && new.host_key == old.host_key
    {
        new.host_key = None;
    }
    if checked.secrets.is_none()
        && existing.has_secret
        && existing.settings.address() != checked.settings.address()
    {
        return Err(ApiError::BadRequest(
            "The server or the account changed: enter the password or key again.".into(),
        ));
    }
    let secrets = match &checked.secrets {
        Some(secrets) => secrets.clone(),
        None => store::secrets(&state.pool, &state.cipher, id).await?,
    };
    checked.settings.check_secrets(&secrets).map_err(ApiError::BadRequest)?;
    name_taken(&state, &checked.name, Some(id)).await?;

    let input = store::Input {
        name: &checked.name,
        enabled: payload.enabled,
        keep: checked.keep,
        include_key: payload.include_key,
        settings: &checked.settings,
    };
    store::update(&state.pool, &state.cipher, id, &input, checked.secrets.as_ref()).await?;
    audit::record(&state.pool, Some(&me), "backup.destination_updated", Some(&checked.name), ip)
        .await;
    Ok(Json(one(&state, id).await?.view()))
}

pub async fn remove(
    State(state): State<AppState>,
    Extension(CurrentPrincipal(principal)): Extension<CurrentPrincipal>,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let me = admin(&principal)?.username.clone();
    let existing = one(&state, id).await?;
    store::delete(&state.pool, id).await?;
    audit::record(&state.pool, Some(&me), "backup.destination_deleted", Some(&existing.name), ip)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Écrit, liste et supprime un petit fichier. Pour SFTP sans empreinte
/// confirmée, la réponse porte celle que le serveur a présentée.
pub async fn test(
    State(state): State<AppState>,
    Extension(CurrentPrincipal(principal)): Extension<CurrentPrincipal>,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<Json<TestOutcome>> {
    let me = admin(&principal)?.username.clone();
    let destination = one(&state, id).await?;
    let secrets = store::secrets(&state.pool, &state.cipher, id).await?;
    let outcome = offsite::test_destination(&destination.settings, &secrets).await;
    let subject = format!("{}: {}", destination.name, if outcome.ok { "ok" } else { "failed" });
    audit::record(&state.pool, Some(&me), "backup.destination_tested", Some(&subject), ip).await;
    Ok(Json(outcome))
}
