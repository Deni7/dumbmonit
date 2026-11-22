//! Gestion des jetons d'API depuis l'interface : liste, création, changement
//! (nom, réseaux, échéance), révocation.
//!
//! Ce sont des routes d'administration ordinaires, protégées par la session et
//! fermées à tout jeton (voir `middleware::TOKEN_DENIED`) : un jeton ne fabrique
//! jamais un autre jeton, et donc jamais un plus puissant que lui. Le jeton
//! lui-même n'est présenté qu'aux routes de l'API et au serveur MCP.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::{AdminIdentity, Authenticated};
use crate::auth::token::{
    self as token, MAX_EXPIRY_DAYS, NewToken, Scope, TokenChanges, TokenRecord,
};
use crate::state::AppState;

/// Longueur maximale d'un nom de jeton : il s'affiche dans une liste et dans les
/// journaux, il n'a pas à être un paragraphe.
const MAX_NAME_LEN: usize = 80;

/// Réponse à la création : le seul moment où le secret est visible.
#[derive(Debug, Serialize)]
pub struct CreatedToken {
    #[serde(flatten)]
    pub token: TokenRecord,
    pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct TokenPayload {
    /// Facultatif : absent ou vide, un nom générique daté (UTC) est généré.
    #[serde(default)]
    pub name: String,
    /// `read` par défaut : c'est le choix sûr, et celui que l'interface présélectionne.
    #[serde(default)]
    pub scope: Option<String>,
    /// Durée de validité en jours ; absent ou `null` : le jeton n'expire pas.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
    /// Réseaux d'où le jeton sera accepté (CIDR ou adresse seule) ; vide : partout.
    #[serde(default)]
    pub allowed_networks: Vec<String>,
}

/// Corps de `PATCH /api/tokens/{id}` : un champ absent ne change pas.
#[derive(Debug, Deserialize)]
pub struct UpdatePayload {
    #[serde(default)]
    pub name: Option<String>,
    /// Remplace la liste ; vide : partout.
    #[serde(default)]
    pub allowed_networks: Option<Vec<String>>,
    /// Nouvelle échéance, comptée d'aujourd'hui ; `null` : n'expire plus.
    #[serde(default, deserialize_with = "double_option")]
    pub expires_in_days: Option<Option<i64>>,
}

/// Distingue un champ absent (`None`) d'un `null` explicite (`Some(None)`).
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Réservé aux administrateurs : nom, portée, propriétaire et réseaux autorisés
/// de chaque jeton sont une reconnaissance utile à qui voudrait en voler un.
pub async fn list(
    _: AdminIdentity,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<TokenRecord>>> {
    Ok(Json(token::list(&state.pool).await?))
}

/// Nom fourni (nettoyé) ou, à défaut, un nom générique daté.
fn api_token_name(provided: &str, today: chrono::NaiveDate) -> String {
    let provided = provided.trim();
    if provided.is_empty() {
        format!("API token · {}", today.format("%Y-%m-%d"))
    } else {
        provided.to_string()
    }
}

/// Refuse un nom trop long.
fn check_name_length(name: &str) -> ApiResult<()> {
    if name.chars().count() > MAX_NAME_LEN {
        return Err(ApiError::BadRequest(format!(
            "Token name is limited to {MAX_NAME_LEN} characters."
        )));
    }
    Ok(())
}

/// Échéance à `days` jours d'aujourd'hui, dans les mêmes bornes qu'à la
/// création ; `None` : jamais. Une échéance n'est donc jamais dans le passé.
fn expiry_from_days(days: Option<i64>, now: DateTime<Utc>) -> ApiResult<Option<DateTime<Utc>>> {
    match days {
        None => Ok(None),
        Some(days) if (1..=MAX_EXPIRY_DAYS).contains(&days) => {
            Ok(Some(now + TimeDelta::days(days)))
        }
        Some(_) => Err(ApiError::BadRequest(format!(
            "\"expires_in_days\" must be between 1 and {MAX_EXPIRY_DAYS}, or null for a token that \
             never expires."
        ))),
    }
}

pub async fn create(
    State(state): State<AppState>,
    Authenticated(me): Authenticated,
    ClientIp(ip): ClientIp,
    Json(payload): Json<TokenPayload>,
) -> ApiResult<(StatusCode, Json<CreatedToken>)> {
    let name = api_token_name(&payload.name, chrono::Utc::now().date_naive());
    check_name_length(&name)?;
    let scope = match payload.scope.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Scope::Read,
        Some(raw) => Scope::parse(raw).ok_or_else(|| {
            ApiError::BadRequest(format!("Unknown scope \"{raw}\" (expected: read, write)."))
        })?,
    };
    let expires_at = expiry_from_days(payload.expires_in_days, Utc::now())?;
    let allowed_networks =
        token::parse_networks(&payload.allowed_networks).map_err(ApiError::BadRequest)?;

    let new = NewToken { name, scope, user_id: Some(me.id), expires_at, allowed_networks };
    let (record, secret) = token::create(&state.pool, &new).await?;
    tracing::info!(
        token = %record.name,
        token_id = record.id,
        scope = scope.as_str(),
        expires_at = record.expires_at.as_deref().unwrap_or("never"),
        networks = record.allowed_networks.len(),
        by = %me.username,
        "API token created"
    );
    audit::record(&state.pool, Some(&me.username), "token.created", Some(&record.name), ip).await;
    Ok((StatusCode::CREATED, Json(CreatedToken { token: record, secret })))
}

/// `PATCH /api/tokens/{id}` — renomme, change les réseaux autorisés ou
/// l'échéance, avec les mêmes contrôles qu'à la création. Le secret ne change
/// pas : les clients qui l'utilisent continuent de fonctionner.
pub async fn update(
    State(state): State<AppState>,
    Authenticated(me): Authenticated,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePayload>,
) -> ApiResult<Json<TokenRecord>> {
    let name = match payload.name.as_deref().map(str::trim) {
        None => None,
        Some("") => return Err(ApiError::BadRequest("Enter a name for the token.".into())),
        Some(name) => {
            check_name_length(name)?;
            Some(name.to_string())
        }
    };
    let allowed_networks = payload
        .allowed_networks
        .as_deref()
        .map(token::parse_networks)
        .transpose()
        .map_err(ApiError::BadRequest)?;
    let expires_at =
        payload.expires_in_days.map(|days| expiry_from_days(days, Utc::now())).transpose()?;
    let changes = TokenChanges { name, allowed_networks, expires_at };
    let Some(record) = token::update(&state.pool, id, &changes).await? else {
        return Err(ApiError::NotFound(format!("Token {id} not found or revoked.")));
    };
    tracing::info!(
        token = %record.name,
        token_id = record.id,
        expires_at = record.expires_at.as_deref().unwrap_or("never"),
        networks = record.allowed_networks.len(),
        by = %me.username,
        "API token updated"
    );
    audit::record(&state.pool, Some(&me.username), "token.updated", Some(&record.name), ip).await;
    Ok(Json(record))
}

pub async fn revoke(
    State(state): State<AppState>,
    Authenticated(me): Authenticated,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if token::revoke(&state.pool, id).await? {
        tracing::info!(token_id = id, by = %me.username, "API token revoked");
        audit::record(&state.pool, Some(&me.username), "token.revoked", Some(&id.to_string()), ip)
            .await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Token {id} not found or already revoked.")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_expiry_is_never_in_the_past_and_never_beyond_the_maximum() {
        let now = Utc::now();
        assert_eq!(expiry_from_days(None, now).ok(), Some(None));
        assert_eq!(expiry_from_days(Some(30), now).ok(), Some(Some(now + TimeDelta::days(30))));
        for days in [0, -1, MAX_EXPIRY_DAYS + 1] {
            assert!(expiry_from_days(Some(days), now).is_err(), "{days}");
        }
    }

    #[test]
    fn a_patch_tells_an_absent_expiry_from_a_null_one() {
        let absent: UpdatePayload = serde_json::from_str(r#"{ "name": "x" }"#).unwrap();
        assert_eq!(absent.expires_in_days, None);
        let never: UpdatePayload = serde_json::from_str(r#"{ "expires_in_days": null }"#).unwrap();
        assert_eq!(never.expires_in_days, Some(None));
        let days: UpdatePayload = serde_json::from_str(r#"{ "expires_in_days": 90 }"#).unwrap();
        assert_eq!(days.expires_in_days, Some(Some(90)));
    }

    #[test]
    fn api_token_name_is_generated_when_blank_and_kept_otherwise() {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        assert_eq!(api_token_name("", day), "API token · 2026-11-02");
        assert_eq!(api_token_name("  ", day), "API token · 2026-11-02");
        assert_eq!(api_token_name(" laptop ", day), "laptop");
    }
}
