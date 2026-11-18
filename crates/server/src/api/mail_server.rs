//! Serveur de courriel de l'instance (`crate::notify::mail_server`).
//!
//! - `GET /api/notify/mail-server` : le réglage en vigueur, sans le mot de
//!   passe (`has_password`), et s'il vient de l'environnement.
//! - `PUT /api/notify/mail-server` : l'enregistre. `secrets` absent garde le
//!   mot de passe, `secrets: {}` l'efface, `secrets: {"password": "…"}` le
//!   remplace. Refusé (409) quand `DUMBMONIT_SMTP_HOST` est renseignée.
//! - `POST /api/notify/mail-server/test` : envoie un courriel d'essai à une
//!   adresse saisie, avec le réglage du formulaire s'il est joint (sinon celui
//!   en vigueur), et rend l'erreur SMTP telle quelle, expurgée.
//!
//! Le mot de passe enregistré n'est gardé que si l'hôte et l'identifiant ne
//! changent pas : sans cela, un compte qui peut modifier le réglage enverrait
//! le mot de passe, sans jamais le lire, au serveur de son choix.

use std::collections::HashMap;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::AdminIdentity;
use crate::notify::mail_server::{self, MailSecurity, MailServer, PasswordChange};
use crate::notify::smtp::Smtp;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notify/mail-server", get(read).put(save))
        .route("/notify/mail-server/test", post(test))
}

/// Ce que l'interface affiche ; jamais le mot de passe.
#[derive(Debug, Serialize)]
pub struct MailServerView {
    pub host: String,
    pub port: Option<u16>,
    /// Le port réellement utilisé : `port`, ou celui du chiffrement.
    pub effective_port: u16,
    pub security: MailSecurity,
    pub username: String,
    pub has_password: bool,
    pub from_address: String,
    pub from_name: String,
    /// Assez renseigné pour envoyer (hôte et expéditeur).
    pub configured: bool,
    /// Décrit par `DUMBMONIT_SMTP_*` : lecture seule dans l'interface.
    pub locked_by_env: bool,
}

/// Le réglage saisi. Les secrets suivent la règle des canaux : absents, ils
/// sont gardés ; `{}` les efface.
#[derive(Debug, Default, Deserialize)]
pub struct MailServerUpdate {
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub security: MailSecurity,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub from_address: String,
    #[serde(default)]
    pub from_name: String,
    #[serde(default)]
    pub secrets: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
pub struct TestPayload {
    /// Destinataire du courriel d'essai.
    pub to: String,
    /// Réglage du formulaire, pas encore enregistré ; absent : celui en vigueur.
    #[serde(default)]
    pub server: Option<MailServerUpdate>,
}

#[derive(Debug, Serialize)]
pub struct TestReport {
    pub ok: bool,
    pub message: String,
}

fn view(resolved: &mail_server::Resolved) -> MailServerView {
    let server = &resolved.server;
    MailServerView {
        host: server.host.clone(),
        port: server.port,
        effective_port: server.effective_port(),
        security: server.security,
        username: server.username.clone(),
        has_password: resolved.password.as_deref().is_some_and(|p| !p.is_empty()),
        from_address: server.from_address.clone(),
        from_name: server.from_name.clone(),
        configured: server.is_configured(),
        locked_by_env: resolved.from_env,
    }
}

/// Le serveur saisi et ce que devient le mot de passe, après contrôle.
/// `stored` est le réglage enregistré, pour savoir si le mot de passe gardé
/// part bien vers le même serveur, avec le même identifiant.
fn checked(
    update: MailServerUpdate,
    stored: &MailServer,
    has_stored_password: bool,
) -> ApiResult<(MailServer, PasswordChange)> {
    let secrets = update.secrets;
    let server = MailServer {
        host: update.host,
        port: update.port,
        security: update.security,
        username: update.username,
        from_address: update.from_address,
        from_name: update.from_name,
    }
    .trimmed();
    server.validate().map_err(ApiError::BadRequest)?;
    let password = match secrets {
        None => PasswordChange::Keep,
        Some(secrets) => match secrets.get("password").map(String::as_str) {
            None | Some("") => PasswordChange::Clear,
            Some(password) => {
                mail_server::check_password(password).map_err(ApiError::BadRequest)?;
                PasswordChange::Set(password.to_string())
            }
        },
    };
    let same_host = server.host.eq_ignore_ascii_case(&stored.host);
    let moved = !same_host || server.username != stored.username;
    let sends_password = !server.username.is_empty();
    if password == PasswordChange::Keep && has_stored_password && moved && sends_password {
        return Err(ApiError::BadRequest(
            "The host or the username changed: enter the password again.".into(),
        ));
    }
    Ok((server, password))
}

/// `GET /api/notify/mail-server`.
async fn read(
    State(state): State<AppState>,
    AdminIdentity(_): AdminIdentity,
) -> ApiResult<Json<MailServerView>> {
    let resolved = mail_server::resolve(&state.pool, &state.cipher).await?;
    Ok(Json(view(&resolved)))
}

/// `PUT /api/notify/mail-server`.
async fn save(
    State(state): State<AppState>,
    AdminIdentity(me): AdminIdentity,
    ClientIp(ip): ClientIp,
    Json(update): Json<MailServerUpdate>,
) -> ApiResult<Json<MailServerView>> {
    if mail_server::from_env().is_some() {
        return Err(ApiError::Conflict(
            "The email server is set by the DUMBMONIT_SMTP_* environment variables.".into(),
        ));
    }
    let stored = mail_server::load_stored(&state.pool).await?.unwrap_or_default();
    let has_password = mail_server::load_password(&state.pool, &state.cipher)
        .await
        .ok()
        .flatten()
        .is_some_and(|p| !p.is_empty());
    let (server, password) = checked(update, &stored, has_password)?;
    mail_server::save(&state.pool, &state.cipher, &server, password).await?;
    audit::record(&state.pool, Some(&me.label()), "notify.mail_server", None, ip).await;
    // Les pages de statut proposent l'abonnement par courriel selon ce réglage.
    crate::api::status_pages::invalidate_cache();
    let resolved = mail_server::resolve(&state.pool, &state.cipher).await?;
    Ok(Json(view(&resolved)))
}

/// `POST /api/notify/mail-server/test`.
async fn test(
    State(state): State<AppState>,
    AdminIdentity(me): AdminIdentity,
    ClientIp(ip): ClientIp,
    Json(payload): Json<TestPayload>,
) -> ApiResult<Json<TestReport>> {
    if crate::notify::sending_disabled() {
        return Err(ApiError::BadRequest(crate::notify::DISABLED_MESSAGE.into()));
    }
    let to = payload.to.trim().to_string();
    if to.parse::<lettre::Address>().is_err() {
        return Err(ApiError::BadRequest("Enter a valid email address to send to.".into()));
    }
    let resolved = mail_server::resolve(&state.pool, &state.cipher).await?;
    let channel = match payload.server {
        // L'environnement l'emporte : le formulaire est en lecture seule.
        Some(update) if !resolved.from_env => {
            let (server, password) =
                checked(update, &resolved.server, resolved.password.is_some())?;
            let password = match password {
                PasswordChange::Keep => resolved.password.clone(),
                PasswordChange::Clear => None,
                PasswordChange::Set(password) => Some(password),
            };
            if !server.is_configured() {
                return Err(ApiError::BadRequest("Enter the email server first.".into()));
            }
            server.channel(password.as_deref())
        }
        _ if resolved.is_configured() => resolved.channel(),
        _ => return Err(ApiError::BadRequest("No email server is set yet.".into())),
    };
    let smtp = Smtp::sender(&channel).map_err(|error| ApiError::BadRequest(error.to_string()))?;
    let body = "This is a test email from DumbMonit.\n\n\
                If you can read it, the email server is set up: status page subscribers \
                and scheduled reports without a channel of their own are sent through it.\n"
        .to_string();
    audit::record(&state.pool, Some(&me.label()), "notify.mail_server.test", Some(&to), ip).await;
    // Déjà expurgée par le transport : le mot de passe n'y figure jamais.
    let sent = smtp.send_to(&to, "DumbMonit test email", body, None).await;
    sent.map_err(|error| ApiError::BadRequest(format!("Sending failed: {error}")))?;
    Ok(Json(TestReport { ok: true, message: format!("Test email sent to {to}.") }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(host: &str, username: &str) -> MailServerUpdate {
        MailServerUpdate {
            host: host.into(),
            username: username.into(),
            from_address: "status@example.org".into(),
            ..MailServerUpdate::default()
        }
    }

    fn stored() -> MailServer {
        MailServer {
            host: "smtp.example.org".into(),
            username: "robot".into(),
            from_address: "status@example.org".into(),
            ..MailServer::default()
        }
    }

    /// `ApiError` n'a pas de `Debug` : pas d'`unwrap`.
    fn change(result: ApiResult<(MailServer, PasswordChange)>) -> PasswordChange {
        match result {
            Ok((_, change)) => change,
            Err(_) => panic!("refused"),
        }
    }

    #[test]
    fn omitted_secrets_keep_the_password_and_empty_ones_clear_it() {
        let kept = change(checked(update("smtp.example.org", "robot"), &stored(), true));
        assert_eq!(kept, PasswordChange::Keep);
        let mut cleared = update("smtp.example.org", "robot");
        cleared.secrets = Some(HashMap::new());
        assert_eq!(change(checked(cleared, &stored(), true)), PasswordChange::Clear);
        let mut set = update("smtp.example.org", "robot");
        set.secrets = Some(HashMap::from([("password".to_string(), "pw".to_string())]));
        assert_eq!(change(checked(set, &stored(), true)), PasswordChange::Set("pw".into()));
    }

    #[test]
    fn a_kept_password_never_follows_another_host() {
        let Err(ApiError::BadRequest(why)) =
            checked(update("evil.example.net", "robot"), &stored(), true)
        else {
            panic!("the stored password would go to another host");
        };
        assert!(why.contains("password"));
        assert!(checked(update("smtp.example.org", "other"), &stored(), true).is_err());
        // Sans mot de passe enregistré, rien à protéger.
        assert!(checked(update("evil.example.net", "robot"), &stored(), false).is_ok());
        // Hôte saisi autrement, même serveur.
        assert!(checked(update(" SMTP.example.org ", "robot"), &stored(), true).is_ok());
    }
}
