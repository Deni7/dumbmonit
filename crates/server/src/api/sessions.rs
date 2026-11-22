//! Sessions ouvertes et politique d'authentification.
//!
//! Chacun voit et ferme *ses* sessions — la courante exceptée, qui se ferme par
//! « Se déconnecter ». Un administrateur peut déconnecter un compte partout,
//! et régler la politique de l'instance (second facteur obligatoire pour les
//! administrateurs et les opérateurs, voir `auth::policy`). Toutes ces routes
//! sont fermées aux jetons d'API (`/auth/**`, `/users/**`).

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Serialize;

use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::{AdminUser, Authenticated, CurrentSession};
use crate::auth::policy::{self, Policy};
use crate::auth::session::{self, SessionView};
use crate::auth::users;
use crate::auth::{AuthError, AuthResult, audit};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/sessions", get(list))
        .route("/auth/sessions/revoke-others", post(revoke_others))
        .route("/auth/sessions/{id}", delete(revoke))
        .route("/users/{id}/sessions", delete(sign_out_user))
        .route("/auth/policy", get(get_policy).put(put_policy))
}

/// `GET /api/auth/sessions` — les sessions ouvertes du compte courant.
pub async fn list(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
    current: Option<Extension<CurrentSession>>,
) -> AuthResult<Json<Vec<SessionView>>> {
    let current = current.as_ref().map(|Extension(CurrentSession(token))| token.id());
    Ok(Json(session::list_for_user(&state.pool, user.id, current).await?))
}

/// `DELETE /api/auth/sessions/{id}` — ferme une autre session du compte.
pub async fn revoke(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
    ClientIp(ip): ClientIp,
    current: Option<Extension<CurrentSession>>,
    Path(id): Path<String>,
) -> AuthResult<StatusCode> {
    if current.as_ref().is_some_and(|Extension(CurrentSession(token))| token.id() == id) {
        return Err(AuthError::Conflict(
            "This is the session you are using: sign out instead.".into(),
        ));
    }
    if !session::delete_own(&state.pool, user.id, &id).await? {
        return Err(AuthError::NotFound("No such session.".into()));
    }
    audit::record(&state.pool, Some(&user.username), "session.revoked", None, ip).await;
    tracing::info!(user = %user.username, "session revoked");
    Ok(StatusCode::NO_CONTENT)
}

/// Combien de sessions ont été fermées.
#[derive(Debug, Serialize)]
pub struct Closed {
    pub closed: u64,
}

/// `POST /api/auth/sessions/revoke-others` — déconnecte le compte partout
/// ailleurs que dans ce navigateur.
pub async fn revoke_others(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
    ClientIp(ip): ClientIp,
    current: Option<Extension<CurrentSession>>,
) -> AuthResult<Json<Closed>> {
    let keep = current.as_ref().map(|Extension(CurrentSession(token))| token.id());
    let closed = session::delete_for_user(&state.pool, user.id, keep).await?;
    audit::record(&state.pool, Some(&user.username), "session.revoked_others", None, ip).await;
    tracing::info!(user = %user.username, sessions_closed = closed, "other sessions revoked");
    Ok(Json(Closed { closed }))
}

/// `DELETE /api/users/{id}/sessions` — un administrateur déconnecte un compte
/// partout. Pour son propre compte, la session qui fait la demande survit.
pub async fn sign_out_user(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    current: Option<Extension<CurrentSession>>,
    Path(id): Path<i64>,
) -> AuthResult<Json<Closed>> {
    let Some(user) = users::get(&state.pool, id).await? else {
        return Err(AuthError::NotFound("No such user.".into()));
    };
    let keep = if user.id == me.id {
        current.as_ref().map(|Extension(CurrentSession(token))| token.id())
    } else {
        None
    };
    let closed = session::delete_for_user(&state.pool, user.id, keep).await?;
    audit::record(&state.pool, Some(&me.username), "user.signed_out", Some(&user.username), ip)
        .await;
    tracing::info!(
        user = %user.username,
        by = %me.username,
        sessions_closed = closed,
        "user signed out everywhere"
    );
    Ok(Json(Closed { closed }))
}

/// `GET /api/auth/policy`
pub async fn get_policy(State(state): State<AppState>, _: AdminUser) -> AuthResult<Json<Policy>> {
    Ok(Json(policy::load(&state.pool).await?))
}

/// `PUT /api/auth/policy` — remplace la politique ; un champ absent vaut son
/// défaut (coupé).
pub async fn put_policy(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    Json(wanted): Json<Policy>,
) -> AuthResult<Json<Policy>> {
    policy::save(&state.pool, &wanted).await?;
    let subject = if wanted.require_totp { "require_totp=on" } else { "require_totp=off" };
    audit::record(&state.pool, Some(&me.username), "policy.updated", Some(subject), ip).await;
    tracing::info!(by = %me.username, require_totp = wanted.require_totp, "auth policy updated");
    Ok(Json(wanted))
}
