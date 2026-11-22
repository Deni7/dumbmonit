//! Invitations (`auth::invitations`).
//!
//! L'administrateur invite une adresse avec un rôle et des équipes ; le lien
//! part par le serveur de courriel de l'instance, et lui est rendu dans tous
//! les cas, à copier quand aucun serveur n'est réglé. Les équipes ne donnent
//! pas la gestion des comptes : seul un administrateur invite.
//!
//! Deux routes publiques pour la personne invitée, sous les mêmes règles que
//! la réinitialisation : lire l'invitation, l'accepter. Accepter crée le
//! compte avec un mot de passe local — ou sans, quand la personne choisit le
//! SSO : l'identifiant est alors son adresse, celle que le fournisseur
//! présentera vérifiée à sa première connexion (`oidc::flow::find_linkable`).

use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::oidc::Origin;
use super::recovery::{link_keys, refuse_link};
use crate::api::auth::guard_attempt;
use crate::auth::client_ip::ClientIp;
use crate::auth::invitations::{self, Acceptance, Invitation, NewInvitation, Refusal};
use crate::auth::link_token::LinkToken;
use crate::auth::middleware::{AdminUser, require_same_origin};
use crate::auth::users::{self, Role};
use crate::auth::{AuthError, AuthResult, AuthState, account_mail, audit, password, recovery};
use crate::state::AppState;

/// Longueur maximale d'un nom affiché choisi à l'acceptation.
const MAX_DISPLAY_NAME: usize = 120;

pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/auth/invitations/check", post(check))
        .route("/auth/invitations/accept", post(accept))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/users/invitations", get(list).post(create))
        .route("/users/invitations/{id}", delete(revoke))
        .route("/users/invitations/{id}/resend", post(resend))
}

/// Une équipe citée par une invitation.
#[derive(Debug, Clone, Serialize)]
pub struct TeamRef {
    pub id: i64,
    pub name: String,
}

/// Une invitation telle que l'administrateur la voit.
#[derive(Debug, Serialize)]
pub struct InvitationView {
    pub id: i64,
    pub email: String,
    pub role: Role,
    /// Équipes qui existent encore.
    pub teams: Vec<TeamRef>,
    pub invited_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_sent_at: Option<String>,
    /// Échue : elle ne s'accepte plus, mais peut être renvoyée.
    pub expired: bool,
}

async fn view(state: &AppState, invitation: Invitation) -> AuthResult<InvitationView> {
    let teams = crate::auth::teams::list(&state.pool).await?;
    Ok(view_with(&teams, invitation))
}

fn view_with(teams: &[crate::auth::teams::Team], invitation: Invitation) -> InvitationView {
    let expired = invitation.is_expired(Utc::now());
    InvitationView {
        teams: teams
            .iter()
            .filter(|team| invitation.teams.contains(&team.id))
            .map(|team| TeamRef { id: team.id, name: team.name.clone() })
            .collect(),
        id: invitation.id,
        email: invitation.email,
        role: invitation.role,
        invited_by: invitation.invited_by,
        created_at: invitation.created_at,
        expires_at: invitation.expires_at,
        last_sent_at: invitation.last_sent_at,
        expired,
    }
}

/// `GET /api/users/invitations` — les invitations ouvertes.
pub async fn list(
    State(state): State<AppState>,
    _: AdminUser,
) -> AuthResult<Json<Vec<InvitationView>>> {
    let teams = crate::auth::teams::list(&state.pool).await?;
    let open = invitations::list_open(&state.pool).await?;
    Ok(Json(open.into_iter().map(|invitation| view_with(&teams, invitation)).collect()))
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct CreatePayload {
    email: String,
    role: Role,
    #[serde(default)]
    teams: Vec<i64>,
    /// Envoyer le lien par courriel ; sinon il est seulement rendu.
    #[serde(default = "yes")]
    send: bool,
}

/// Résultat d'une invitation créée ou renvoyée : le lien, montré une fois.
#[derive(Debug, Serialize)]
pub struct Sent {
    pub invitation: InvitationView,
    pub url: String,
    pub emailed: bool,
    /// Pourquoi le courriel n'est pas parti, quand l'envoi a été tenté.
    pub email_error: Option<String>,
}

/// `POST /api/users/invitations`
pub async fn create(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    origin: Origin,
    Json(payload): Json<CreatePayload>,
) -> AuthResult<(StatusCode, Json<Sent>)> {
    let email = users::normalize_email(&payload.email)
        .map_err(AuthError::Invalid)?
        .ok_or_else(|| AuthError::Invalid("Enter the email address to invite.".into()))?;
    if !invitations::invitable(payload.role) {
        return Err(AuthError::Invalid(
            "A wall display is not invited: create its account from the list of users.".into(),
        ));
    }
    let known: Vec<i64> =
        crate::auth::teams::list(&state.pool).await?.into_iter().map(|team| team.id).collect();
    if let Some(missing) = payload.teams.iter().find(|id| !known.contains(*id)) {
        return Err(AuthError::Invalid(format!("Team {missing} does not exist.")));
    }
    let taken: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM users WHERE email = ? COLLATE NOCASE OR username = ? COLLATE NOCASE",
    )
    .bind(&email)
    .bind(&email)
    .fetch_optional(&state.pool)
    .await?;
    if taken.is_some() {
        return Err(AuthError::Conflict("An account already uses this email address.".into()));
    }
    let mut teams = payload.teams.clone();
    teams.sort_unstable();
    teams.dedup();
    let hours = recovery::load(&state.pool).await?.invite_hours;
    let new = NewInvitation {
        email: &email,
        role: payload.role,
        teams: &teams,
        invited_by: &me.username,
        hours,
    };
    let (invitation, token) = invitations::create(&state.pool, &new, Utc::now()).await?;
    audit::record(&state.pool, Some(&me.username), "invitation.created", Some(&email), ip).await;
    tracing::info!(role = payload.role.as_str(), "invitation created");
    let sent = deliver(&state, &me, &origin, invitation, &token, payload.send).await?;
    Ok((StatusCode::CREATED, Json(sent)))
}

#[derive(Debug, Deserialize)]
pub struct ResendPayload {
    #[serde(default = "yes")]
    send: bool,
}

/// `POST /api/users/invitations/{id}/resend` — un nouveau lien, une nouvelle
/// échéance ; l'ancien lien cesse de fonctionner.
pub async fn resend(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    origin: Origin,
    Path(id): Path<i64>,
    Json(payload): Json<ResendPayload>,
) -> AuthResult<Json<Sent>> {
    let hours = recovery::load(&state.pool).await?.invite_hours;
    let renewed = invitations::renew(&state.pool, id, hours, Utc::now()).await?;
    let Some((invitation, token)) = renewed else {
        return Err(AuthError::NotFound("No such open invitation.".into()));
    };
    let subject = invitation.email.clone();
    audit::record(&state.pool, Some(&me.username), "invitation.resent", Some(&subject), ip).await;
    Ok(Json(deliver(&state, &me, &origin, invitation, &token, payload.send).await?))
}

/// Construit le lien, l'envoie si demandé et possible, et le rend.
async fn deliver(
    state: &AppState,
    me: &users::User,
    origin: &Origin,
    invitation: Invitation,
    token: &LinkToken,
    send: bool,
) -> AuthResult<Sent> {
    recovery::remember_origin(&state.pool, origin.as_str()).await;
    let base = match recovery::link_base(&state.pool).await {
        Some(base) => base,
        None => origin.as_str().to_string(),
    };
    let url = format!("{base}/invite?token={}", token.value());
    let (emailed, email_error) = if send {
        let instance = account_mail::instance_name(&state.pool).await;
        let minutes = recovery::load(&state.pool).await?.invite_hours * 60;
        let role = invitation.role;
        let letter = account_mail::invite_letter(&instance, me.label(), role, &url, minutes);
        match account_mail::send(&state.pool, &state.cipher, &invitation.email, letter).await {
            Ok(()) => (true, None),
            Err(error) => (false, Some(error)),
        }
    } else {
        (false, None)
    };
    Ok(Sent { invitation: view(state, invitation).await?, url, emailed, email_error })
}

/// `DELETE /api/users/invitations/{id}`
pub async fn revoke(
    State(state): State<AppState>,
    AdminUser(me): AdminUser,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> AuthResult<StatusCode> {
    let Some(invitation) = invitations::get(&state.pool, id).await? else {
        return Err(AuthError::NotFound("No such open invitation.".into()));
    };
    if !invitations::revoke(&state.pool, id, Utc::now()).await? {
        return Err(AuthError::NotFound("No such open invitation.".into()));
    }
    let subject = invitation.email.as_str();
    audit::record(&state.pool, Some(&me.username), "invitation.revoked", Some(subject), ip).await;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Publiques
// ---------------------------------------------------------------------------

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

/// Ce que la page d'invitation montre avant que la personne ne choisisse.
#[derive(Debug, Serialize)]
pub struct InvitationCheck {
    pub email: String,
    pub role: Role,
    pub suggested_username: String,
    pub invited_by: String,
    pub expires_at: String,
    /// Le SSO (OIDC ou SAML) est actif : la page propose de s'en servir.
    pub sso: bool,
}

async fn sso_enabled(state: &AppState) -> AuthResult<bool> {
    let oidc = crate::auth::oidc::resolve(&state.pool, &state.cipher, &state.config.oidc).await?;
    let saml = crate::auth::saml::load(&state.pool).await?;
    Ok(oidc.config.enabled() || saml.usable())
}

/// `POST /api/auth/invitations/check` — publique.
pub async fn check(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<TokenPayload>,
) -> AuthResult<Json<InvitationCheck>> {
    require_same_origin(&headers)?;
    guard_attempt(&auth, &link_keys(ip)).await?;
    let refused = "invitation.invalid_link";
    let Some(token) = LinkToken::parse(&payload.token) else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let Some(invitation) = invitations::find_open(&state.pool, &token, Utc::now()).await? else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let inviter = users::by_username(&state.pool, &invitation.invited_by)
        .await?
        .map_or_else(|| invitation.invited_by.clone(), |user| user.label().to_string());
    Ok(Json(InvitationCheck {
        suggested_username: invitations::suggested_username(&invitation.email),
        email: invitation.email,
        role: invitation.role,
        invited_by: inviter,
        expires_at: invitation.expires_at,
        sso: sso_enabled(&state).await?,
    }))
}

#[derive(Deserialize)]
pub struct AcceptPayload {
    #[serde(default)]
    token: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    password: Option<String>,
    /// Pas de mot de passe : le compte attend sa première connexion SSO.
    #[serde(default)]
    sso: bool,
}

impl std::fmt::Debug for AcceptPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AcceptPayload {{ token: <redacted>, username: {:?}, password: <redacted>, sso: {} }}",
            self.username, self.sso
        )
    }
}

#[derive(Debug, Serialize)]
pub struct Accepted {
    pub username: String,
}

/// `POST /api/auth/invitations/accept` — publique : crée le compte.
pub async fn accept(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<AcceptPayload>,
) -> AuthResult<(StatusCode, Json<Accepted>)> {
    require_same_origin(&headers)?;
    guard_attempt(&auth, &link_keys(ip)).await?;
    let refused = "invitation.invalid_link";
    let Some(token) = LinkToken::parse(&payload.token) else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let Some(invitation) = invitations::find_open(&state.pool, &token, Utc::now()).await? else {
        return Err(refuse_link(&state, &auth, ip, refused).await);
    };
    let display_name = clean_display_name(&payload.display_name)?;
    let (username, hash) = if payload.sso {
        if !sso_enabled(&state).await? {
            return Err(AuthError::Invalid(
                "Single sign-on is not enabled on this instance: choose a password.".into(),
            ));
        }
        (invitation.email.clone(), None)
    } else {
        let username = payload.username.trim().to_string();
        users::validate_username(&username).map_err(AuthError::Invalid)?;
        let password = payload.password.unwrap_or_default();
        password::validate(&password)?;
        (username, Some(password::hash(password).await?))
    };
    let acceptance = Acceptance {
        username: &username,
        display_name: &display_name,
        password_hash: hash.as_deref(),
    };
    match invitations::accept(&state.pool, &token, &acceptance, Utc::now()).await? {
        Ok((_, invitation)) => {
            let email = invitation.email.as_str();
            let action = "invitation.accepted";
            audit::record(&state.pool, Some(&username), action, Some(email), ip).await;
            let role = invitation.role.as_str();
            tracing::info!(user = %username, role, "invitation accepted");
            Ok((StatusCode::CREATED, Json(Accepted { username })))
        }
        Err(Refusal::UsernameTaken) if payload.sso => Err(AuthError::Conflict(
            "An account already uses your email address as its username: ask your administrator."
                .into(),
        )),
        Err(Refusal::UsernameTaken) => Err(AuthError::Conflict(format!(
            "The username \"{username}\" is taken: choose another one."
        ))),
        Err(Refusal::Invalid) => Err(refuse_link(&state, &auth, ip, refused).await),
    }
}

/// Nom affiché choisi : du texte d'une ligne, 120 caractères au plus.
fn clean_display_name(raw: &str) -> AuthResult<String> {
    let name = raw.trim();
    if name.chars().count() > MAX_DISPLAY_NAME || name.chars().any(char::is_control) {
        return Err(AuthError::Invalid(format!(
            "The display name is limited to {MAX_DISPLAY_NAME} characters of plain text."
        )));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_leaks_a_token_or_a_password() {
        let rendered = format!(
            "{:?} {:?}",
            TokenPayload { token: "abc.s3cr3t-jeton".into() },
            AcceptPayload {
                token: "abc.s3cr3t-jeton".into(),
                username: "jane".into(),
                display_name: String::new(),
                password: Some("s3cr3t-mdp".into()),
                sso: false,
            },
        );
        assert!(!rendered.contains("s3cr3t"), "{rendered}");
        assert!(rendered.contains("jane"));
    }

    #[test]
    fn display_names_are_plain_and_short() {
        assert_eq!(clean_display_name("  Jane Doe ").ok().as_deref(), Some("Jane Doe"));
        assert!(clean_display_name("Jane\nDoe").is_err());
        assert!(clean_display_name(&"a".repeat(121)).is_err());
    }
}
