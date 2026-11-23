//! Administration des comptes. Réservée aux administrateurs, comme celle des
//! rôles personnalisés qu'on leur donne (`roles.rs`).
//!
//! Une règle traverse toutes ces routes : il reste toujours au moins un
//! administrateur actif. Sans elle, une instance pourrait se retrouver avec des
//! lecteurs seulement — et personne pour la réparer.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Deserialize;

use crate::api::auth::UserView;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::AdminUser;
use crate::auth::users::{self, NewUser, Role, User};
use crate::auth::{AuthError, AuthResult, audit, custom_roles, ldap, oidc, password};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct CreatePayload {
    username: String,
    #[serde(default)]
    display_name: Option<String>,
    role: Role,
    /// Rôle personnalisé (`/api/roles`) ; le rôle livré est alors `viewer`.
    #[serde(default)]
    custom_role_id: Option<i64>,
    #[serde(default)]
    password: Option<String>,
    /// Facultative ; vide : aucune.
    #[serde(default)]
    email: Option<String>,
}

impl std::fmt::Debug for CreatePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CreatePayload {{ username: {:?}, role: {:?}, password: <redacted> }}",
            self.username, self.role
        )
    }
}

#[derive(Deserialize)]
pub struct UpdatePayload {
    #[serde(default)]
    display_name: Option<String>,
    /// Rôle livré : retire le rôle personnalisé du compte.
    #[serde(default)]
    role: Option<Role>,
    /// Rôle personnalisé à donner (le rôle livré devient `viewer`) ; pour le
    /// retirer, envoyer un `role`.
    #[serde(default)]
    custom_role_id: Option<i64>,
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    password: Option<String>,
    /// Absente : inchangée ; vide : effacée.
    #[serde(default)]
    email: Option<String>,
}

impl std::fmt::Debug for UpdatePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "UpdatePayload {{ role: {:?}, disabled: {:?}, password: <redacted> }}",
            self.role, self.disabled
        )
    }
}

/// `GET /api/users`
pub async fn list(State(state): State<AppState>, _: AdminUser) -> AuthResult<Json<Vec<UserView>>> {
    let users = users::list(&state.pool).await?;
    Ok(Json(users.into_iter().map(UserView::from).collect()))
}

/// `POST /api/users` — le mot de passe est facultatif seulement si le SSO est
/// actif : un compte sans mot de passe ni fournisseur ne pourrait jamais entrer.
pub async fn create(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    Json(payload): Json<CreatePayload>,
) -> AuthResult<(StatusCode, Json<UserView>)> {
    let username = payload.username.trim();
    users::validate_username(username).map_err(AuthError::Invalid)?;
    let custom_role =
        checked_custom_role(&state, Some(payload.role), payload.custom_role_id).await?;
    let email = users::normalize_email(payload.email.as_deref().unwrap_or(""))
        .map_err(AuthError::Invalid)?;

    let password = payload.password.filter(|password| !password.is_empty());
    let hash = match password {
        Some(password) => {
            password::validate(&password)?;
            Some(password::hash(password).await?)
        }
        None => {
            // Sans mot de passe, le compte attend sa première connexion SSO
            // (OIDC ou SAML) ou LDAP : l'un d'eux doit être actif.
            let sso = oidc::resolve(&state.pool, &state.cipher, &state.config.oidc).await?;
            let directory = ldap::resolve(&state.pool, &state.cipher, &state.config.ldap).await?;
            let saml = crate::auth::saml::load(&state.pool).await?;
            if !sso.config.enabled() && !directory.config.enabled() && !saml.usable() {
                return Err(AuthError::Invalid(
                    "Set a password: neither single sign-on nor LDAP is enabled, so this user could not sign in otherwise."
                        .into(),
                ));
            }
            None
        }
    };

    let created = users::insert(
        &state.pool,
        NewUser {
            username,
            display_name: payload.display_name.as_deref().unwrap_or("").trim(),
            role: payload.role,
            password_hash: hash.as_deref(),
            oidc: None,
        },
    )
    .await?;
    let Some(id) = created else {
        return Err(AuthError::Conflict(format!("A user named \"{username}\" already exists.")));
    };
    if email.is_some() {
        users::set_email(&state.pool, id, email.as_deref()).await?;
    }
    if custom_role.is_some() {
        let changes = users::AccountChanges { custom_role, ..Default::default() };
        users::apply_changes(&state.pool, id, &changes).await?;
    }
    let user = load(&state, id).await?;
    audit::record(&state.pool, Some(&me.username), "user.created", Some(&user.username), ip).await;
    tracing::info!(user = %user.username, role = user.role.as_str(), "user created");
    Ok((StatusCode::CREATED, Json(user.into())))
}

/// `PUT /api/users/{id}`
pub async fn update(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePayload>,
) -> AuthResult<Json<UserView>> {
    let user = load(&state, id).await?;
    let custom_role = checked_custom_role(&state, payload.role, payload.custom_role_id).await?;

    // Ce que l'on retire à un administrateur actif ne doit pas laisser l'instance
    // sans administrateur.
    let loses_admin = user.role.is_admin()
        && !user.disabled
        && (payload.role.is_some_and(|role| !role.is_admin())
            || custom_role.is_some()
            || payload.disabled == Some(true));
    if loses_admin && users::active_admin_count(&state.pool).await? <= 1 {
        return Err(AuthError::Conflict(
            "This is the last admin: promote another user first.".into(),
        ));
    }
    if payload.disabled == Some(true) && user.id == me.id {
        return Err(AuthError::Conflict("You cannot disable your own account.".into()));
    }

    // Tout ce qui peut être refusé l'est avant la première écriture : un mot de
    // passe invalide ne doit pas laisser un rôle déjà changé.
    let hash = match payload.password.filter(|password| !password.is_empty()) {
        Some(password) => {
            password::validate(&password)?;
            Some(password::hash(password).await?)
        }
        None => None,
    };
    let email = payload
        .email
        .as_deref()
        .map(users::normalize_email)
        .transpose()
        .map_err(AuthError::Invalid)?;
    let disabled = payload.disabled.filter(|disabled| *disabled != user.disabled);
    // Un rôle livré retire le rôle personnalisé, même s'il ne change pas
    // (`viewer` d'un compte à rôle personnalisé) ; avec un rôle personnalisé,
    // c'est celui-ci qui décide.
    let role_change = payload
        .role
        .filter(|role| custom_role.is_none() && (*role != user.role || user.custom_role.is_some()));
    let current_custom = user.custom_role.as_ref().map(|role| role.id);
    let custom_change = custom_role.filter(|wanted| current_custom != Some(*wanted));
    let changes = users::AccountChanges {
        display_name: payload.display_name.as_deref().map(str::trim),
        email: email.as_ref().map(Option::as_deref),
        role: role_change,
        custom_role: custom_change,
        // Un mot de passe remis par un administrateur ferme les sessions du compte :
        // c'est le geste qu'on fait quand on croit le compte compromis. Désactiver
        // le compte les ferme aussi.
        close_sessions: hash.is_some() || disabled == Some(true),
        password_hash: hash.as_deref(),
        disabled,
    };
    users::apply_changes(&state.pool, id, &changes).await?;

    let user = load(&state, id).await?;
    audit::record(&state.pool, Some(&me.username), "user.updated", Some(&user.username), ip).await;
    tracing::info!(user = %user.username, "user updated");
    Ok(Json(user.into()))
}

/// `DELETE /api/users/{id}` — pas soi-même, pas le dernier administrateur.
pub async fn delete(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AuthResult<StatusCode> {
    let user = load(&state, id).await?;
    if user.id == me.id {
        return Err(AuthError::Conflict(
            "You cannot delete your own account. Ask another admin to do it.".into(),
        ));
    }
    if user.role.is_admin() && !user.disabled && users::active_admin_count(&state.pool).await? <= 1
    {
        return Err(AuthError::Conflict(
            "This is the last admin: promote another user first.".into(),
        ));
    }
    // Les sessions suivent en cascade.
    users::delete(&state.pool, id).await?;
    audit::record(&state.pool, Some(&me.username), "user.deleted", Some(&user.username), ip).await;
    tracing::info!(user = %user.username, "user deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Le rôle personnalisé demandé, s'il existe. Il remplace le rôle livré, qui ne
/// peut alors être que `viewer` : un rôle personnalisé ne s'ajoute pas à un
/// rôle d'administrateur, d'opérateur ou d'écran mural.
async fn checked_custom_role(
    state: &AppState,
    role: Option<Role>,
    custom_role_id: Option<i64>,
) -> AuthResult<Option<i64>> {
    let Some(id) = custom_role_id else { return Ok(None) };
    if role.is_some_and(|role| role != Role::Viewer) {
        return Err(AuthError::Invalid(
            "A custom role replaces the built-in role: send it with the viewer role, or alone."
                .into(),
        ));
    }
    if custom_roles::get(&state.pool, id).await?.is_none() {
        return Err(AuthError::Invalid("This custom role does not exist.".into()));
    }
    Ok(Some(id))
}

async fn load(state: &AppState, id: i64) -> AuthResult<User> {
    users::get(&state.pool, id).await?.ok_or_else(|| AuthError::NotFound("No such user.".into()))
}
