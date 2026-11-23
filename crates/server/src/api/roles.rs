//! Rôles personnalisés : `/api/roles`.
//!
//! Réservé aux administrateurs connectés (extracteur [`AdminUser`]) ; les
//! jetons d'API n'y ont pas accès (`TOKEN_DENIED`) : décider de qui peut faire
//! quoi est un geste que l'on fait soi-même, dans l'interface. Le catalogue
//! des permissions est fixe, défini par le serveur
//! ([`crate::auth::permissions`]) : l'interface le lit ici et n'en suppose
//! rien. Gérer les comptes, les rôles, les jetons,
//! le SSO, les sauvegardes et les réglages de l'instance ne se délègue pas.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, put};
use axum::{Json, Router};
use serde::Serialize;

use crate::auth::client_ip::ClientIp;
use crate::auth::custom_roles::{self, CustomRole, RoleInput, WriteError};
use crate::auth::middleware::AdminUser;
use crate::auth::permissions::Permission;
use crate::auth::{AuthError, AuthResult, audit};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/roles", get(list).post(create))
        .route("/roles/permissions", get(catalog))
        .route("/roles/{id}", put(update).delete(remove))
}

/// Une route ouverte par une permission, pour que l'administrateur voie ce
/// qu'il délègue.
#[derive(Debug, Serialize)]
pub struct RouteView {
    pub method: &'static str,
    /// Gabarit sous `/api`, `*` = un segment.
    pub path: String,
}

/// Une permission du catalogue.
#[derive(Debug, Serialize)]
pub struct PermissionView {
    pub id: Permission,
    /// Nom et description en anglais, pour une permission que l'interface ne
    /// connaît pas encore.
    pub label: &'static str,
    pub description: &'static str,
    /// Toujours comprise dans un rôle personnalisé (`view`).
    pub implied: bool,
    pub routes: Vec<RouteView>,
}

/// `GET /api/roles/permissions` — le catalogue fixe.
pub async fn catalog(_: AdminUser) -> Json<Vec<PermissionView>> {
    Json(
        Permission::ALL
            .into_iter()
            .map(|permission| PermissionView {
                id: permission,
                label: permission.label(),
                description: permission.description(),
                implied: permission == Permission::View,
                routes: permission
                    .routes()
                    .iter()
                    .map(|&(method, path)| RouteView { method, path: format!("/api{path}") })
                    .collect(),
            })
            .collect(),
    )
}

/// `GET /api/roles`
pub async fn list(
    _: AdminUser,
    State(state): State<AppState>,
) -> AuthResult<Json<Vec<CustomRole>>> {
    Ok(Json(custom_roles::list(&state.pool).await?))
}

fn write_error(error: WriteError, name: &str) -> AuthError {
    match error {
        WriteError::NameTaken => AuthError::Conflict(format!("A role named \"{name}\" exists.")),
        WriteError::TooMany => AuthError::Conflict(format!(
            "This instance already has {} custom roles: remove one first.",
            custom_roles::MAX_ROLES
        )),
        WriteError::NotFound => not_found(),
        WriteError::InUse(users) => AuthError::Conflict(format!(
            "This role is still given to {users} account(s): give them another role first."
        )),
    }
}

fn not_found() -> AuthError {
    AuthError::NotFound("Role not found.".into())
}

async fn load(state: &AppState, id: i64) -> AuthResult<CustomRole> {
    custom_roles::get(&state.pool, id).await?.ok_or_else(not_found)
}

/// Le nom du rôle et ses permissions, pour le journal d'audit.
fn audit_subject(role: &CustomRole) -> String {
    let names: Vec<&str> = role.permissions.iter().map(|p| p.as_str()).collect();
    format!("{} [{}]", role.name, names.join(", "))
}

/// `POST /api/roles`
pub async fn create(
    AdminUser(me): AdminUser,
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(input): Json<RoleInput>,
) -> AuthResult<(StatusCode, Json<CustomRole>)> {
    let checked = input.check().map_err(AuthError::Invalid)?;
    let id = custom_roles::create(&state.pool, &checked)
        .await?
        .map_err(|error| write_error(error, &checked.name))?;
    let role = load(&state, id).await?;
    let subject = audit_subject(&role);
    audit::record(&state.pool, Some(&me.username), "role.created", Some(&subject), ip).await;
    Ok((StatusCode::CREATED, Json(role)))
}

/// `PUT /api/roles/{id}` — vaut tout de suite pour les comptes qui l'ont.
pub async fn update(
    AdminUser(me): AdminUser,
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
    Json(input): Json<RoleInput>,
) -> AuthResult<Json<CustomRole>> {
    let checked = input.check().map_err(AuthError::Invalid)?;
    custom_roles::update(&state.pool, id, &checked)
        .await?
        .map_err(|error| write_error(error, &checked.name))?;
    let role = load(&state, id).await?;
    let subject = audit_subject(&role);
    audit::record(&state.pool, Some(&me.username), "role.updated", Some(&subject), ip).await;
    Ok(Json(role))
}

/// `DELETE /api/roles/{id}` — 409 tant qu'un compte a ce rôle.
pub async fn remove(
    AdminUser(me): AdminUser,
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AuthResult<StatusCode> {
    let role = load(&state, id).await?;
    custom_roles::delete(&state.pool, id).await?.map_err(|error| write_error(error, &role.name))?;
    audit::record(&state.pool, Some(&me.username), "role.deleted", Some(&role.name), ip).await;
    Ok(StatusCode::NO_CONTENT)
}

/// Le catalogue n'a pas de trou : chaque permission, sauf `view`, ouvre au
/// moins une route, et aucune ne touche à l'administration.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_permission_reaches_administration() {
        // `alerts.operate` lance des actions de remédiation déjà définies, comme
        // un opérateur : il n'en définit aucune.
        const NEVER: &[&str] = &[
            "/users",
            "/roles",
            "/teams",
            "/tokens",
            "/auth",
            "/backup",
            "/clients",
            "/agent",
            "/instance",
            "/security",
            "/remediation/policies",
            "/notify/mail-server",
            "/scim",
            "/logs",
            "/system",
            "/update",
            "/config",
        ];
        for permission in Permission::ALL {
            let routes = permission.routes();
            assert_eq!(routes.is_empty(), permission == Permission::View, "{permission:?}");
            for (_, path) in routes {
                for prefix in NEVER {
                    let inside = *path == *prefix
                        || path.strip_prefix(*prefix).is_some_and(|rest| rest.starts_with('/'));
                    assert!(!inside, "{permission:?} ouvre {path}");
                }
            }
        }
        assert_eq!(crate::auth::permissions::normalize(&[]), vec![Permission::View]);
    }
}
