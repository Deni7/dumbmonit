//! « Mes notifications » : les choix personnels du compte connecté
//! (`notify::personal`). Chacun ne lit et n'écrit que les siens, en session
//! seulement (`/auth/**` est fermé aux jetons d'API), quel que soit son rôle —
//! sauf l'écran mural, qui ne joint pas ces routes.

use std::collections::BTreeSet;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use sqlx::Row;

use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::Authenticated;
use crate::auth::teams::{self, TeamAccess};
use crate::auth::users::User;
use crate::auth::{AuthError, AuthResult, account_mail, audit};
use crate::notify::personal::{self, Prefs};
use crate::notify::webpush;
use crate::state::AppState;

/// Valeurs d'étiquettes proposées, au plus.
const MAX_TAG_OPTIONS: usize = 300;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/auth/notifications", get(get_prefs).put(put_prefs))
        .route("/auth/notifications/test", post(test))
}

/// Les choix, et ce qu'il faut pour les faire.
#[derive(Debug, Serialize)]
pub struct NotificationsView {
    pub prefs: Prefs,
    /// Adresse du compte, réglée par un administrateur.
    pub email: Option<String>,
    /// Le serveur de courriel de l'instance est réglé.
    pub mail_server: bool,
    /// Appareils abonnés au Web Push.
    pub push_devices: i64,
    /// Équipes du compte.
    pub teams: Vec<String>,
    /// Dossiers que le compte voit, pour le choix du périmètre.
    pub folders: Vec<String>,
    /// Étiquettes (`clé=valeur`) des équipements qu'il voit.
    pub tags: Vec<String>,
}

async fn view(
    state: &AppState,
    user: &User,
    access: &teams::Access,
) -> AuthResult<NotificationsView> {
    let pool = &state.pool;
    let folders: Vec<String> = teams::folders(pool)
        .await?
        .into_iter()
        .filter(|folder| access.folder_level(folder).is_some())
        .collect();
    let team_names: Vec<String> = sqlx::query_scalar(
        "SELECT t.name FROM teams t JOIN team_members m ON m.team_id = t.id
         WHERE m.user_id = ? ORDER BY t.name COLLATE NOCASE",
    )
    .bind(user.id)
    .fetch_all(pool)
    .await?;
    Ok(NotificationsView {
        prefs: personal::load(pool, user.id).await?,
        email: user.email.clone(),
        mail_server: account_mail::ready(pool, &state.cipher).await,
        push_devices: webpush::store::count_for_user(pool, user.id).await?,
        teams: team_names,
        folders,
        tags: tag_options(state, access).await?,
    })
}

/// Les étiquettes des équipements visibles, en `clé=valeur`.
async fn tag_options(state: &AppState, access: &teams::Access) -> AuthResult<Vec<String>> {
    let visible = access.visible_targets(&state.pool).await?;
    let rows = sqlx::query("SELECT id, tags FROM targets").fetch_all(&state.pool).await?;
    let mut out = BTreeSet::new();
    for row in &rows {
        let id: i64 = row.try_get("id")?;
        if visible.as_ref().is_some_and(|visible| !visible.contains(&id)) {
            continue;
        }
        let raw: String = row.try_get("tags")?;
        let tags: std::collections::BTreeMap<String, String> =
            serde_json::from_str(&raw).unwrap_or_default();
        for (key, value) in tags {
            out.insert(if value.is_empty() { key } else { format!("{key}={value}") });
            if out.len() >= MAX_TAG_OPTIONS {
                return Ok(out.into_iter().collect());
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// `GET /api/auth/notifications`
pub async fn get_prefs(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
    TeamAccess(access): TeamAccess,
) -> AuthResult<Json<NotificationsView>> {
    Ok(Json(view(&state, &user, &access).await?))
}

/// `PUT /api/auth/notifications` — remplace les choix ; un champ absent vaut
/// son défaut.
pub async fn put_prefs(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
    TeamAccess(access): TeamAccess,
    ClientIp(ip): ClientIp,
    Json(payload): Json<Prefs>,
) -> AuthResult<Json<NotificationsView>> {
    let prefs = payload.validated().map_err(AuthError::Invalid)?;
    if prefs.email && user.email.is_none() {
        return Err(AuthError::Invalid(
            "Your account has no email address: ask an administrator to add one.".into(),
        ));
    }
    personal::save(&state.pool, user.id, &prefs).await?;
    audit::record(&state.pool, Some(&user.username), "notifications.personal", None, ip).await;
    Ok(Json(view(&state, &user, &access).await?))
}

/// Ce qu'a donné un envoi d'essai.
#[derive(Debug, Serialize)]
pub struct TestReport {
    /// `None` : pas tenté (courriel non choisi, ou impossible).
    pub email: Option<Delivery>,
    pub push: Option<Delivery>,
}

#[derive(Debug, Serialize)]
pub struct Delivery {
    pub ok: bool,
    pub error: Option<String>,
}

/// `POST /api/auth/notifications/test` — un message d'essai par les moyens
/// choisis et enregistrés.
pub async fn test(
    State(state): State<AppState>,
    Authenticated(user): Authenticated,
) -> AuthResult<Json<TestReport>> {
    let prefs = personal::load(&state.pool, user.id).await?;
    if !prefs.active() {
        return Err(AuthError::Conflict(
            "Turn on email or push notifications and save first.".into(),
        ));
    }
    let message = crate::notify::test_message("My notifications");
    let email = match (prefs.email, user.email.as_deref()) {
        (true, Some(address)) => Some(test_email(&state, address, &message).await),
        _ => None,
    };
    let push = if prefs.webpush { Some(test_push(&state, &user, &message).await) } else { None };
    Ok(Json(TestReport { email, push }))
}

async fn test_email(state: &AppState, address: &str, message: &crate::notify::Message) -> Delivery {
    let mail = crate::notify::mail_server::channel(&state.pool, &state.cipher).await;
    let Some(mut channel) = mail else {
        return Delivery { ok: false, error: Some("No email server is configured.".into()) };
    };
    channel.settings["to"] = serde_json::json!([address]);
    let report = crate::notify::deliver(&reqwest::Client::new(), &channel, message).await;
    Delivery { ok: report.is_success(), error: report.error }
}

async fn test_push(state: &AppState, user: &User, message: &crate::notify::Message) -> Delivery {
    let store = webpush::Store { pool: &state.pool, cipher: &state.cipher };
    let audience = webpush::Audience::User { user_id: user.id, subscription: None };
    let settings = webpush::Settings::for_test();
    let client = webpush::client();
    match webpush::send(store, &client, &audience, &settings, message).await {
        Ok(outcome) if outcome.delivered > 0 => Delivery { ok: true, error: None },
        Ok(outcome) => {
            let error = outcome.last_error.unwrap_or_else(|| "No device is subscribed.".into());
            Delivery { ok: false, error: Some(error) }
        }
        Err(error) => Delivery { ok: false, error: Some(error.to_string()) },
    }
}
