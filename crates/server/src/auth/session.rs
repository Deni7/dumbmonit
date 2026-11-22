//! Sessions opaques.
//!
//! Le cookie ne porte aucune information : ni identité, ni date, ni signature —
//! seulement `<identifiant>.<jeton>`. Tout le reste vit en base, ce qui permet de
//! révoquer une session immédiatement, chose qu'un jeton auto-porté (JWT) ne sait
//! pas faire sans réintroduire… une table de sessions.
//!
//! La partie secrète n'est jamais stockée : la base ne contient que son empreinte
//! SHA-256. Une copie de `dumbmonit.db` ne donne donc aucune session utilisable.
//!
//! Chaque session garde aussi de quoi la reconnaître dans la liste « mes
//! sessions » : navigateur (`User-Agent`, tronqué), adresse, façon dont elle a
//! été ouverte, et dernière activité — rafraîchie au plus toutes les
//! [`TOUCH_INTERVAL_SECS`] secondes, pour ne pas écrire à chaque requête.

use std::net::IpAddr;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use axum::http::request::Parts;
use axum::http::{HeaderMap, header};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use subtle::ConstantTimeEq;

/// Durée de vie d'une session, par défaut. Trente jours : assez long pour ne pas
/// redemander le mot de passe à chaque visite d'un tableau de bord qu'on garde
/// ouvert, assez court pour qu'un poste oublié finisse par se refermer.
pub const DEFAULT_LIFETIME_DAYS: i64 = 30;
/// Bornes de `DUMBMONIT_SESSION_DAYS`.
pub const MIN_LIFETIME_DAYS: i64 = 1;
pub const MAX_LIFETIME_DAYS: i64 = 365;
/// Variable d'environnement qui règle la durée de vie.
pub const LIFETIME_ENV: &str = "DUMBMONIT_SESSION_DAYS";

/// Délai minimal entre deux mises à jour de la dernière activité d'une session.
pub const TOUCH_INTERVAL_SECS: i64 = 300;

/// Longueur gardée du `User-Agent` : de quoi reconnaître un navigateur, pas de
/// quoi remplir la base avec ce qu'un client voudra bien envoyer.
const USER_AGENT_MAX: usize = 256;

/// Lue une fois par processus : la valeur ne change qu'au redémarrage.
static LIFETIME: LazyLock<i64> = LazyLock::new(|| {
    let raw = crate::config::env_var(LIFETIME_ENV);
    let days = parse_lifetime(raw.as_deref());
    if let Some(raw) = raw.as_deref().map(str::trim).filter(|raw| !raw.is_empty())
        && raw != days.to_string()
    {
        tracing::warn!(
            value = raw,
            days,
            "{LIFETIME_ENV}: expected a number of days between {MIN_LIFETIME_DAYS} and \
             {MAX_LIFETIME_DAYS}; using {days}"
        );
    }
    days
});

/// Durée de vie des nouvelles sessions, en jours (`DUMBMONIT_SESSION_DAYS`).
pub fn lifetime_days() -> i64 {
    *LIFETIME
}

/// Lit la durée voulue : absente ou illisible, le défaut ; hors bornes, la
/// borne la plus proche.
pub fn parse_lifetime(raw: Option<&str>) -> i64 {
    match raw.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => DEFAULT_LIFETIME_DAYS,
        Some(raw) => raw
            .parse::<i64>()
            .map(|days| days.clamp(MIN_LIFETIME_DAYS, MAX_LIFETIME_DAYS))
            .unwrap_or(DEFAULT_LIFETIME_DAYS),
    }
}

/// Façon dont une session a été ouverte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Via {
    /// Mot de passe local, avec ou sans second facteur.
    Password,
    /// Clé d'accès (WebAuthn, vérification de l'utilisateur exigée).
    Passkey,
    Ldap,
    Oidc,
    Saml,
}

impl Via {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Passkey => "passkey",
            Self::Ldap => "ldap",
            Self::Oidc => "oidc",
            Self::Saml => "saml",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "password" => Some(Self::Password),
            "passkey" => Some(Self::Passkey),
            "ldap" => Some(Self::Ldap),
            "oidc" => Some(Self::Oidc),
            "saml" => Some(Self::Saml),
            _ => None,
        }
    }

    /// Vrai quand le second facteur se joue ailleurs que dans notre TOTP : chez
    /// le fournisseur d'identité (OIDC, SAML, annuaire), ou dans la clé d'accès
    /// elle-même (possession et vérification de l'utilisateur). La politique
    /// « 2FA obligatoire » ne vise que les connexions par mot de passe local.
    pub fn exempt_from_totp_policy(self) -> bool {
        !matches!(self, Self::Password)
    }
}

/// D'où vient une session : de quoi la reconnaître plus tard dans la liste.
#[derive(Debug, Clone, Copy)]
pub struct SignIn<'a> {
    pub via: Via,
    pub ip: Option<IpAddr>,
    pub user_agent: Option<&'a str>,
}

/// `User-Agent` d'une requête, nettoyé et tronqué.
pub fn user_agent(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::USER_AGENT)?.to_str().ok()?;
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).take(USER_AGENT_MAX).collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

/// Extracteur : le `User-Agent` de la requête, s'il y en a un.
#[derive(Debug, Clone)]
pub struct UserAgent(pub Option<String>);

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for UserAgent {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(user_agent(&parts.headers)))
    }
}

/// Octets d'entropie de la partie secrète du cookie. À 32 octets, la deviner est
/// hors de portée, et la limitation des tentatives ne sert que pour le mot de passe.
const TOKEN_BYTES: usize = 32;
/// Octets de l'identifiant public. Il n'a pas à être secret, seulement unique.
const ID_BYTES: usize = 16;

/// Jeton de session, tel qu'il transite dans le cookie.
///
/// `Debug` est écrit à la main : ce type finit dans des messages de trace, et une
/// dérivation laisserait fuiter des sessions valides dans les journaux.
#[derive(Clone)]
pub struct SessionToken {
    /// Partie publique : sert à retrouver la ligne en base.
    id: String,
    /// Partie secrète : sert à prouver la session, jamais stockée en clair.
    secret: String,
}

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SessionToken {{ id: {:?}, secret: <redacted> }}", self.id)
    }
}

impl SessionToken {
    fn generate() -> Self {
        Self {
            id: hex::encode(rand::random::<[u8; ID_BYTES]>()),
            secret: hex::encode(rand::random::<[u8; TOKEN_BYTES]>()),
        }
    }

    /// Relit la valeur d'un cookie. Rend `None` sur toute forme inattendue plutôt
    /// que de propager une erreur : un cookie mal formé n'est pas un incident, il
    /// signifie simplement « pas authentifié ».
    pub fn parse(value: &str) -> Option<Self> {
        let (id, secret) = value.split_once('.')?;
        if id.is_empty() || secret.is_empty() {
            return None;
        }
        Some(Self { id: id.to_string(), secret: secret.to_string() })
    }

    /// Valeur à placer dans le cookie.
    pub fn cookie_value(&self) -> String {
        format!("{}.{}", self.id, self.secret)
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    fn secret_hash(&self) -> Vec<u8> {
        Sha256::digest(self.secret.as_bytes()).to_vec()
    }
}

fn to_sql(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Ouvre une session pour un compte et renvoie le jeton à confier au navigateur.
///
/// La purge des sessions expirées est faite ici : les connexions sont rares, et
/// c'est le seul moment où la table grossit. Cela évite une tâche de fond dédiée.
pub async fn create(pool: &SqlitePool, user_id: i64, origin: &SignIn<'_>) -> Result<SessionToken> {
    let token = SessionToken::generate();
    let now = Utc::now();
    let expires_at = now + Duration::days(lifetime_days());
    let user_agent: Option<String> =
        origin.user_agent.map(|ua| ua.chars().take(USER_AGENT_MAX).collect());

    sqlx::query(
        "INSERT INTO auth_sessions
             (id, token_hash, created_at, expires_at, user_id, last_seen_at, ip, user_agent, via)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&token.id)
    .bind(token.secret_hash())
    .bind(to_sql(now))
    .bind(to_sql(expires_at))
    .bind(user_id)
    .bind(to_sql(now))
    .bind(origin.ip.map(|ip| ip.to_canonical().to_string()))
    .bind(user_agent)
    .bind(origin.via.as_str())
    .execute(pool)
    .await
    .context("ouverture de session")?;

    purge_expired(pool).await?;
    Ok(token)
}

/// Session vérifiée : le compte, et ce qu'il faut savoir d'elle pour la suite
/// de la requête.
#[derive(Debug, Clone)]
pub struct Verified {
    pub user_id: i64,
    /// `None` pour une session ouverte avant que la façon ne soit retenue.
    pub via: Option<Via>,
    pub last_seen_at: Option<String>,
}

impl Verified {
    /// Vrai quand la dernière activité notée date de plus de
    /// [`TOUCH_INTERVAL_SECS`] — ou n'a jamais été notée.
    pub fn needs_touch(&self, now: DateTime<Utc>) -> bool {
        self.last_seen_at
            .as_deref()
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .is_none_or(|at| (now - at.with_timezone(&Utc)).num_seconds() >= TOUCH_INTERVAL_SECS)
    }
}

/// Vérifie un jeton et renvoie le compte auquel la session appartient.
///
/// La recherche se fait sur la partie publique ; le secret, lui, est comparé en
/// temps constant, pour qu'aucune mesure de durée ne permette de reconstituer une
/// empreinte octet par octet. Une session sans compte — reliquat théorique de la
/// migration — vaut « pas authentifié ».
pub async fn authenticate(pool: &SqlitePool, token: &SessionToken) -> Result<Option<Verified>> {
    let row = sqlx::query(
        "SELECT token_hash, expires_at, user_id, via, last_seen_at FROM auth_sessions WHERE id = ?",
    )
    .bind(&token.id)
    .fetch_optional(pool)
    .await
    .context("lecture de la session")?;

    let Some(row) = row else { return Ok(None) };

    let stored: Vec<u8> = row.try_get("token_hash")?;
    let expires_at: String = row.try_get("expires_at")?;
    let user_id: Option<i64> = row.try_get("user_id")?;

    let matches: bool = stored.ct_eq(&token.secret_hash()).into();
    if !matches {
        return Ok(None);
    }

    // Une session expirée est refusée puis effacée : sans cela, un jeton volé
    // resterait indéfiniment en base à attendre la prochaine purge.
    let expired = DateTime::parse_from_rfc3339(&expires_at)
        .map(|at| at.with_timezone(&Utc) <= Utc::now())
        .unwrap_or(true);
    if expired {
        delete(pool, &token.id).await?;
        return Ok(None);
    }

    let Some(user_id) = user_id else { return Ok(None) };
    let via: Option<String> = row.try_get("via")?;
    Ok(Some(Verified {
        user_id,
        via: via.as_deref().and_then(Via::parse),
        last_seen_at: row.try_get("last_seen_at")?,
    }))
}

/// Note l'activité d'une session : l'heure, et l'adresse d'où elle vient
/// maintenant (un portable change de réseau). À n'appeler que si
/// [`Verified::needs_touch`] le demande.
pub async fn touch(pool: &SqlitePool, id: &str, ip: Option<IpAddr>) -> Result<()> {
    sqlx::query("UPDATE auth_sessions SET last_seen_at = ?, ip = COALESCE(?, ip) WHERE id = ?")
        .bind(to_sql(Utc::now()))
        .bind(ip.map(|ip| ip.to_canonical().to_string()))
        .bind(id)
        .execute(pool)
        .await
        .context("activité de la session")?;
    Ok(())
}

/// Une session telle que son propriétaire la voit. Jamais l'empreinte.
#[derive(Debug, Clone, Serialize)]
pub struct SessionView {
    /// Partie publique du cookie : elle désigne la session, elle ne l'ouvre pas.
    pub id: String,
    pub created_at: String,
    pub last_seen_at: Option<String>,
    pub expires_at: String,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    /// Navigateur et système reconnus dans le `User-Agent`, quand ils le sont.
    pub browser: Option<&'static str>,
    pub os: Option<&'static str>,
    pub via: Option<Via>,
    /// La session qui fait la demande.
    pub current: bool,
}

/// Les sessions ouvertes d'un compte, la plus récemment active d'abord.
pub async fn list_for_user(
    pool: &SqlitePool,
    user_id: i64,
    current: Option<&str>,
) -> Result<Vec<SessionView>> {
    let rows = sqlx::query(
        "SELECT id, created_at, last_seen_at, expires_at, ip, user_agent, via
         FROM auth_sessions
         WHERE user_id = ? AND expires_at > ?
         ORDER BY COALESCE(last_seen_at, created_at) DESC",
    )
    .bind(user_id)
    .bind(to_sql(Utc::now()))
    .fetch_all(pool)
    .await
    .context("liste des sessions")?;
    rows.iter()
        .map(|row| {
            let id: String = row.try_get("id")?;
            let user_agent: Option<String> = row.try_get("user_agent")?;
            let via: Option<String> = row.try_get("via")?;
            let (browser, os) = user_agent.as_deref().map(describe_user_agent).unwrap_or_default();
            Ok(SessionView {
                current: current == Some(id.as_str()),
                id,
                created_at: row.try_get("created_at")?,
                last_seen_at: row.try_get("last_seen_at")?,
                expires_at: row.try_get("expires_at")?,
                ip: row.try_get("ip")?,
                user_agent,
                browser,
                os,
                via: via.as_deref().and_then(Via::parse),
            })
        })
        .collect()
}

/// Ferme une session du compte, et seulement du compte : `false` si elle
/// n'existe pas ou appartient à quelqu'un d'autre — les deux cas se ressemblent
/// de l'extérieur.
pub async fn delete_own(pool: &SqlitePool, user_id: i64, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM auth_sessions WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await
        .context("fermeture d'une session")?;
    Ok(result.rows_affected() > 0)
}

/// Reconnaît, sans prétention, le navigateur et le système d'un `User-Agent`.
/// Seulement de quoi dire « Firefox sur Linux » dans une liste ; le texte
/// complet reste affiché pour qui veut le détail.
pub fn describe_user_agent(ua: &str) -> (Option<&'static str>, Option<&'static str>) {
    let has = |needle: &str| ua.contains(needle);
    // L'ordre compte : Edge et Opera se disent aussi Chrome, Chrome se dit
    // aussi Safari, et Android se dit aussi Linux.
    let browser = if has("Edg/") || has("EdgA/") || has("EdgiOS/") {
        Some("Edge")
    } else if has("OPR/") || has("Opera") {
        Some("Opera")
    } else if has("Firefox/") || has("FxiOS/") {
        Some("Firefox")
    } else if has("Chrome/") || has("CriOS/") || has("Chromium/") {
        Some("Chrome")
    } else if has("Safari/") && has("Version/") {
        Some("Safari")
    } else if ua.starts_with("curl/") {
        Some("curl")
    } else {
        None
    };
    let os = if has("Android") {
        Some("Android")
    } else if has("iPhone") || has("iPod") {
        Some("iOS")
    } else if has("iPad") {
        Some("iPadOS")
    } else if has("Windows") {
        Some("Windows")
    } else if has("CrOS") {
        Some("ChromeOS")
    } else if has("Mac OS X") || has("Macintosh") {
        Some("macOS")
    } else if has("Linux") || has("X11") {
        Some("Linux")
    } else {
        None
    };
    (browser, os)
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM auth_sessions WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("fermeture de session")?;
    Ok(())
}

/// Ferme toutes les sessions d'un compte, sauf celle passée en argument.
///
/// C'est le geste du changement de mot de passe : on déconnecte partout ailleurs
/// sans se déconnecter soi-même, sinon l'utilisateur qui vient de sécuriser son
/// compte se retrouverait dehors. Sans `keep`, c'est la révocation complète —
/// compte désactivé, mot de passe remis par un administrateur.
pub async fn delete_for_user(pool: &SqlitePool, user_id: i64, keep: Option<&str>) -> Result<u64> {
    let result = match keep {
        Some(id) => {
            sqlx::query("DELETE FROM auth_sessions WHERE user_id = ? AND id <> ?")
                .bind(user_id)
                .bind(id)
                .execute(pool)
                .await
        }
        None => {
            sqlx::query("DELETE FROM auth_sessions WHERE user_id = ?")
                .bind(user_id)
                .execute(pool)
                .await
        }
    }
    .context("fermeture des autres sessions")?;
    Ok(result.rows_affected())
}

pub async fn purge_expired(pool: &SqlitePool) -> Result<u64> {
    let result = sqlx::query("DELETE FROM auth_sessions WHERE expires_at <= ?")
        .bind(to_sql(Utc::now()))
        .execute(pool)
        .await
        .context("purge des sessions expirées")?;
    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_survives_a_round_trip_through_the_cookie() {
        let token = SessionToken::generate();
        let parsed = SessionToken::parse(&token.cookie_value()).expect("jeton relu");
        assert_eq!(parsed.id, token.id);
        assert_eq!(parsed.secret, token.secret);
    }

    #[test]
    fn a_malformed_cookie_is_not_an_error() {
        assert!(SessionToken::parse("").is_none());
        assert!(SessionToken::parse("sans-point").is_none());
        assert!(SessionToken::parse(".secret").is_none());
        assert!(SessionToken::parse("identifiant.").is_none());
    }

    #[test]
    fn two_tokens_never_collide() {
        let first = SessionToken::generate();
        let second = SessionToken::generate();
        assert_ne!(first.id, second.id);
        assert_ne!(first.secret, second.secret);
        // 32 octets d'entropie, rendus en hexadécimal.
        assert_eq!(first.secret.len(), TOKEN_BYTES * 2);
    }

    #[test]
    fn the_lifetime_defaults_to_thirty_days_and_stays_within_bounds() {
        assert_eq!(parse_lifetime(None), 30);
        assert_eq!(parse_lifetime(Some("")), 30);
        assert_eq!(parse_lifetime(Some(" 7 ")), 7);
        assert_eq!(parse_lifetime(Some("0")), 1);
        assert_eq!(parse_lifetime(Some("-4")), 1);
        assert_eq!(parse_lifetime(Some("1000")), 365);
        assert_eq!(parse_lifetime(Some("trente")), 30);
    }

    #[test]
    fn the_last_activity_is_written_at_most_every_five_minutes() {
        let now = Utc::now();
        let seen = |at: Option<DateTime<Utc>>| Verified {
            user_id: 1,
            via: Some(Via::Password),
            last_seen_at: at.map(to_sql),
        };
        assert!(seen(None).needs_touch(now));
        assert!(!seen(Some(now - Duration::seconds(30))).needs_touch(now));
        assert!(seen(Some(now - Duration::seconds(TOUCH_INTERVAL_SECS))).needs_touch(now));
        let garbled = Verified { last_seen_at: Some("hier".into()), ..seen(None) };
        assert!(garbled.needs_touch(now));
    }

    #[test]
    fn user_agents_are_summarised_without_pretending() {
        let firefox = "Mozilla/5.0 (X11; Linux x86_64; rv:133.0) Gecko/20100101 Firefox/133.0";
        assert_eq!(describe_user_agent(firefox), (Some("Firefox"), Some("Linux")));
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like \
                    Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";
        assert_eq!(describe_user_agent(edge), (Some("Edge"), Some("Windows")));
        let safari = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 \
                      (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
        assert_eq!(describe_user_agent(safari), (Some("Safari"), Some("iOS")));
        let android = "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) \
                       Chrome/131.0.0.0 Mobile Safari/537.36";
        assert_eq!(describe_user_agent(android), (Some("Chrome"), Some("Android")));
        assert_eq!(describe_user_agent("curl/8.11.0"), (Some("curl"), None));
        assert_eq!(describe_user_agent("quelque chose"), (None, None));
    }

    #[test]
    fn sign_in_methods_round_trip_and_only_passwords_fall_under_the_policy() {
        for via in [Via::Password, Via::Passkey, Via::Ldap, Via::Oidc, Via::Saml] {
            assert_eq!(Via::parse(via.as_str()), Some(via));
        }
        assert!(!Via::Password.exempt_from_totp_policy());
        assert!(Via::Oidc.exempt_from_totp_policy());
        assert!(Via::Saml.exempt_from_totp_policy());
        assert!(Via::Ldap.exempt_from_totp_policy());
        assert!(Via::Passkey.exempt_from_totp_policy());
    }

    #[test]
    fn the_user_agent_is_cleaned_and_bounded() {
        let mut headers = HeaderMap::new();
        assert_eq!(user_agent(&headers), None);
        headers.insert(header::USER_AGENT, "  ".parse().unwrap());
        assert_eq!(user_agent(&headers), None);
        headers.insert(header::USER_AGENT, "x".repeat(1000).parse().unwrap());
        assert_eq!(user_agent(&headers).map(|ua| ua.len()), Some(USER_AGENT_MAX));
    }

    /// Garde-fou : une régression ici publierait des sessions valides dans les
    /// journaux, où elles seraient réutilisables telles quelles.
    #[test]
    fn debug_never_leaks_a_secret() {
        let token = SessionToken::generate();
        let rendered = format!("{token:?}");
        assert!(!rendered.contains(&token.secret), "le jeton a fuité dans : {rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
    }
}
