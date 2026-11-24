//! Mot de passe oublié, liens de réinitialisation et réglages des liens de
//! compte (`auth::recovery`).
//!
//! Trois routes publiques, comme la connexion : demander un lien, vérifier un
//! lien, l'utiliser. Elles exigent la preuve d'origine de l'interface
//! (`X-Requested-With`), sont limitées comme la connexion et ne disent jamais
//! si un compte existe : la demande répond toujours `202`, dans un délai
//! constant, et le courriel part en tâche de fond. Un lien inconnu, expiré ou
//! déjà utilisé reçoit une seule et même réponse.
//!
//! Côté administrateur : créer un lien pour un compte (envoyé par courriel, ou
//! rendu pour être transmis à la main), et régler les durées et le libre-service.

use std::time::Instant;

use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::oidc::Origin;
use crate::api::auth::{guard_attempt, limiter_keys};
use crate::auth::client_ip::ClientIp;
use crate::auth::link_token::LinkToken;
use crate::auth::middleware::{AdminUser, require_same_origin};
use crate::auth::rate_limit::Key;
use crate::auth::recovery::{self, RecoverySettings};
use crate::auth::users::{self, User};
use crate::auth::{AuthError, AuthResult, AuthState, account_mail, audit, password};
use crate::state::AppState;

/// Durée minimale d'une réponse à « mot de passe oublié » : qu'un compte
/// existe ou non, la réponse met le même temps.
const FORGOT_FLOOR: std::time::Duration = std::time::Duration::from_millis(400);

/// Comptes visés au plus par une même adresse.
const MAX_ACCOUNTS_PER_EMAIL: usize = 3;

/// Réponse unique à un lien qui ne mène nulle part.
pub const LINK_INVALID: &str = "This link is invalid, expired or already used. Ask for a new one.";

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/password-reset", post(forgot))
        .route("/auth/password-reset/check", post(check))
        .route("/auth/password-reset/complete", post(complete))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/recovery", get(get_settings).put(put_settings))
        .route("/users/{id}/reset-link", post(create_link))
}

/// Seaux d'une demande de lien : l'adresse du client, et le compte visé, toutes
/// adresses confondues — un même compte ne reçoit pas un courriel par seconde.
fn forgot_keys(ip: Option<std::net::IpAddr>, login: &str) -> Vec<Key> {
    vec![Key::User("\u{0}reset".to_string(), ip), Key::user(&format!("\u{0}reset:{login}"), None)]
}

/// Seau des essais de liens (réinitialisation, invitation) d'une adresse.
pub fn link_keys(ip: Option<std::net::IpAddr>) -> Vec<Key> {
    vec![Key::User("\u{0}link".to_string(), ip)]
}

/// Lien refusé : compté comme une tentative, consigné, et toujours la même
/// réponse.
pub async fn refuse_link(
    state: &AppState,
    auth: &AuthState,
    ip: Option<std::net::IpAddr>,
    action: &str,
) -> AuthError {
    auth.limiter().lock().await.record_failure(&link_keys(ip), Instant::now());
    audit::record(&state.pool, None, action, None, ip).await;
    AuthError::NotFound(LINK_INVALID.into())
}

#[derive(Debug, Deserialize)]
pub struct ForgotPayload {
    /// Identifiant ou adresse électronique.
    #[serde(default)]
    login: String,
}

#[derive(Debug, Serialize)]
pub struct Accepted {
    pub message: &'static str,
}

const FORGOT_ANSWER: &str = "If an account matches and can be reset by email, a link is on \
                             its way. Check your inbox and your spam folder.";

/// `POST /api/auth/password-reset` — publique. Toujours `202`, même réponse.
pub async fn forgot(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<ForgotPayload>,
) -> AuthResult<(StatusCode, Json<Accepted>)> {
    require_same_origin(&headers)?;
    let started = tokio::time::Instant::now();
    let login = payload.login.trim();
    if login.is_empty() || login.chars().count() > users::EMAIL_MAX_LEN {
        return Err(AuthError::Invalid("Enter your username or your email address.".into()));
    }
    let keys = forgot_keys(ip, login);
    guard_attempt(&auth, &keys).await?;
    // Toute demande compte, aboutie ou non : c'est elle qui fait partir un
    // courriel, et il n'y a pas de « succès » qui effacerait l'ardoise.
    auth.limiter().lock().await.record_failure(&keys, Instant::now());
    if let Err(error) = request_links(&state, login, ip).await {
        tracing::error!(?error, "password reset request failed");
    }
    tokio::time::sleep_until(started + FORGOT_FLOOR).await;
    Ok((StatusCode::ACCEPTED, Json(Accepted { message: FORGOT_ANSWER })))
}

/// Ce que fait une demande : créer et envoyer un lien à chaque compte qui y a
/// droit. Le courriel part en tâche de fond, pour que la durée de la réponse ne
/// dise rien.
async fn request_links(
    state: &AppState,
    login: &str,
    ip: Option<std::net::IpAddr>,
) -> anyhow::Result<()> {
    let pool = &state.pool;
    let subject: String = login.chars().filter(|c| !c.is_control()).take(64).collect();
    if !self_service_available(state).await {
        audit::record(pool, None, "password_reset.unavailable", Some(&subject), ip).await;
        return Ok(());
    }
    let settings = recovery::load(pool).await?;
    let Some(base) = recovery::link_base(pool).await else { return Ok(()) };
    let accounts = matching_accounts(pool, login).await?;
    if accounts.is_empty() {
        audit::record(pool, None, "password_reset.ignored", Some(&subject), ip).await;
        return Ok(());
    }
    let instance = account_mail::instance_name(pool).await;
    for user in accounts {
        let Some(email) = user.email.clone() else { continue };
        let minutes = settings.reset_minutes;
        let issued = recovery::issue(pool, user.id, "self", minutes, Utc::now()).await?;
        let url = recovery::reset_url(&base, &issued.token);
        let letter = account_mail::reset_letter(&instance, &user.username, &url, minutes);
        audit::record(pool, None, "password_reset.requested", Some(&user.username), ip).await;
        let state = state.clone();
        let username = user.username.clone();
        tokio::spawn(async move {
            if let Err(error) = account_mail::send(&state.pool, &state.cipher, &email, letter).await
            {
                tracing::warn!(user = %username, %error, "password reset email not sent");
            }
        });
    }
    Ok(())
}

/// Les comptes que vise une saisie : l'identifiant d'abord, sinon l'adresse
/// (plusieurs comptes peuvent la partager). Seuls ceux qui ont droit au lien
/// par courriel sont rendus.
async fn matching_accounts(pool: &sqlx::SqlitePool, login: &str) -> anyhow::Result<Vec<User>> {
    if let Some(user) = users::by_username(pool, login).await? {
        return Ok(if recovery::can_reset_by_email(&user) { vec![user] } else { Vec::new() });
    }
    if !login.contains('@') {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = ? COLLATE NOCASE ORDER BY id")
            .bind(login)
            .fetch_all(pool)
            .await?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(user) = users::get(pool, id).await?
            && recovery::can_reset_by_email(&user)
        {
            out.push(user);
        }
        if out.len() >= MAX_ACCOUNTS_PER_EMAIL {
            break;
        }
    }
    Ok(out)
}

/// Le libre-service est-il ouvert : activé, un serveur de courriel, et une
/// base sûre pour les liens.
pub async fn self_service_available(state: &AppState) -> bool {
    let enabled = recovery::load(&state.pool).await.is_ok_and(|settings| settings.self_service);
    enabled
        && account_mail::ready(&state.pool, &state.cipher).await
        && recovery::link_base(&state.pool).await.is_some()
}

#[derive(Deserialize)]
pub struct TokenPayload {
    #[serde(default)]
    token: String,
}

impl std::fmt::Debug for TokenPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TokenPayload { token: <redacted> }")
    }
}

#[derive(Debug, Serialize)]
pub struct ResetCheck {
    pub username: String,
    pub expires_at: String,
}

/// `POST /api/auth/password-reset/check` — publique : le lien est-il valide,
/// et pour quel compte.
pub async fn check(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<TokenPayload>,
) -> AuthResult<Json<ResetCheck>> {
    require_same_origin(&headers)?;
    guard_attempt(&auth, &link_keys(ip)).await?;
    let refused = "password_reset.invalid_link";
    let Some(token) = LinkToken::parse(&payload.token) else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let Some(pending) = recovery::find(&state.pool, &token, Utc::now()).await? else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let user = users::get(&state.pool, pending.user_id).await?.filter(recovery::can_reset);
    let Some(user) = user else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    Ok(Json(ResetCheck { username: user.username, expires_at: pending.expires_at }))
}

#[derive(Deserialize)]
pub struct CompletePayload {
    #[serde(default)]
    token: String,
    password: String,
}

impl std::fmt::Debug for CompletePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CompletePayload { token: <redacted>, password: <redacted> }")
    }
}

/// `POST /api/auth/password-reset/complete` — publique : nouveau mot de passe,
/// toutes les sessions fermées, lien consommé.
pub async fn complete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<CompletePayload>,
) -> AuthResult<StatusCode> {
    require_same_origin(&headers)?;
    guard_attempt(&auth, &link_keys(ip)).await?;
    let refused = "password_reset.invalid_link";
    let Some(token) = LinkToken::parse(&payload.token) else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    if recovery::find(&state.pool, &token, Utc::now()).await?.is_none() {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    }
    password::validate(&payload.password)?;
    let hash = password::hash(payload.password).await?;
    let Some(user_id) = recovery::complete(&state.pool, &token, &hash, Utc::now()).await? else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let username = users::get(&state.pool, user_id).await?.map(|user| user.username);
    // Le compte bloqué par des échecs de connexion depuis cette adresse repart
    // de zéro : son propriétaire vient de prouver qu'il reçoit ses courriels.
    auth.limiter().lock().await.record_success(&limiter_keys(ip, username.as_deref()));
    audit::record(&state.pool, username.as_deref(), "password_reset.completed", None, ip).await;
    tracing::info!(user = username.as_deref().unwrap_or("-"), "password reset by link");
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Administrateur
// ---------------------------------------------------------------------------

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct LinkPayload {
    /// Envoyer le lien par courriel quand c'est possible ; sinon il est
    /// seulement rendu, à transmettre à la main.
    #[serde(default = "yes")]
    send: bool,
}

/// Un lien créé par l'administrateur : montré une fois.
#[derive(Debug, Serialize)]
pub struct LinkView {
    pub url: String,
    pub expires_at: String,
    /// Le courriel est parti.
    pub emailed: bool,
    /// Pourquoi il n'est pas parti, quand l'envoi a été tenté.
    pub email_error: Option<String>,
}

/// `POST /api/users/{id}/reset-link` — un lien de réinitialisation pour ce
/// compte, envoyé à son adresse si possible, et rendu dans tous les cas.
pub async fn create_link(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    origin: Origin,
    Path(id): Path<i64>,
    Json(payload): Json<LinkPayload>,
) -> AuthResult<Json<LinkView>> {
    let Some(user) = users::get(&state.pool, id).await? else {
        return Err(AuthError::NotFound("No such user.".into()));
    };
    if user.role.is_wall() {
        return Err(AuthError::Conflict(
            "A wall display has no reset link: set its password here instead.".into(),
        ));
    }
    if !recovery::can_reset(&user) {
        return Err(AuthError::Conflict(
            "This account has no local password, or is disabled: it signs in through its \
             identity provider."
                .into(),
        ));
    }
    recovery::remember_origin(&state.pool, origin.as_str()).await;
    let base = match recovery::link_base(&state.pool).await {
        Some(base) => base,
        None => origin.as_str().to_string(),
    };
    let minutes = recovery::load(&state.pool).await?.reset_minutes;
    let issued = recovery::issue(&state.pool, user.id, &me.username, minutes, Utc::now()).await?;
    let url = recovery::reset_url(&base, &issued.token);
    let (emailed, email_error) = match user.email.as_deref() {
        Some(address) if payload.send => {
            let instance = account_mail::instance_name(&state.pool).await;
            let letter = account_mail::reset_letter(&instance, &user.username, &url, minutes);
            match account_mail::send(&state.pool, &state.cipher, address, letter).await {
                Ok(()) => (true, None),
                Err(error) => (false, Some(error)),
            }
        }
        _ => (false, None),
    };
    let action = "password_reset.link_created";
    audit::record(&state.pool, Some(&me.username), action, Some(&user.username), ip).await;
    tracing::info!(user = %user.username, emailed, "password reset link created");
    Ok(Json(LinkView {
        url,
        expires_at: crate::db::timestamp(issued.expires_at),
        emailed,
        email_error,
    }))
}

/// Les réglages, et ce qu'il en résulte.
#[derive(Debug, Serialize)]
pub struct SettingsView {
    #[serde(flatten)]
    pub settings: RecoverySettings,
    /// Le serveur de courriel de l'instance est réglé.
    pub mail_server: bool,
    /// Base des liens envoyés, `None` sans URL publique ni origine connue.
    pub link_base: Option<String>,
    /// « Mot de passe oublié ? » envoie bien un lien.
    pub self_service_available: bool,
}

async fn settings_view(state: &AppState) -> AuthResult<SettingsView> {
    Ok(SettingsView {
        settings: recovery::load(&state.pool).await?,
        mail_server: account_mail::ready(&state.pool, &state.cipher).await,
        link_base: recovery::link_base(&state.pool).await,
        self_service_available: self_service_available(state).await,
    })
}

/// `GET /api/auth/recovery` — l'origine de l'administrateur est retenue au
/// passage : c'est la base des liens quand aucune URL publique n'est réglée.
pub async fn get_settings(
    State(state): State<AppState>,
    _: AdminUser,
    origin: Origin,
) -> AuthResult<Json<SettingsView>> {
    recovery::remember_origin(&state.pool, origin.as_str()).await;
    Ok(Json(settings_view(&state).await?))
}

/// `PUT /api/auth/recovery`
pub async fn put_settings(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    origin: Origin,
    Json(payload): Json<RecoverySettings>,
) -> AuthResult<Json<SettingsView>> {
    payload.validate().map_err(AuthError::Invalid)?;
    recovery::save(&state.pool, &payload).await?;
    recovery::remember_origin(&state.pool, origin.as_str()).await;
    audit::record(&state.pool, Some(&me.username), "auth.recovery_settings", None, ip).await;
    Ok(Json(settings_view(&state).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_leaks_a_token_or_a_password() {
        let rendered = format!(
            "{:?} {:?}",
            TokenPayload { token: "abc.s3cr3t-jeton".into() },
            CompletePayload { token: "abc.s3cr3t-jeton".into(), password: "s3cr3t-mdp".into() },
        );
        assert!(!rendered.contains("s3cr3t"), "{rendered}");
    }

    #[test]
    fn reset_requests_have_their_own_buckets() {
        let ip: std::net::IpAddr = "192.168.1.10".parse().unwrap();
        let keys = forgot_keys(Some(ip), "Jane");
        assert!(!keys.contains(&Key::Ip(ip)), "never the sign-in bucket of the address");
        assert!(keys.contains(&Key::user("\u{0}reset:jane", None)), "one bucket per account");
        let mut buckets = crate::auth::rate_limit::Buckets::new();
        let now = Instant::now();
        for _ in 0..10 {
            buckets.record_failure(&keys, now);
        }
        assert!(buckets.check(&keys, now).is_err());
        assert!(buckets.check(&limiter_keys(Some(ip), Some("jane")), now).is_ok());
    }
}
