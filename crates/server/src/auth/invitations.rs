//! Invitations : un administrateur invite une adresse avec un rôle (et des
//! équipes), et le lien reçu ouvre une page où la personne choisit son
//! identifiant, son nom et son mot de passe — ou passe par le SSO.
//!
//! Le lien est à usage unique et expire ([`super::recovery::RecoverySettings`]).
//! Inviter de nouveau la même adresse remplace l'invitation en cours ; la
//! renvoyer tire un nouveau lien (l'ancien cesse de fonctionner). Une
//! invitation acceptée ou retirée sort de la liste, et disparaît de la base
//! un mois plus tard.
//!
//! Jamais de rôle `wall` : un écran mural se crée depuis la liste des comptes.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::auth::link_token::LinkToken;
use crate::auth::users::Role;

/// Une invitation, sans son jeton.
#[derive(Debug, Clone, Serialize)]
pub struct Invitation {
    pub id: i64,
    pub email: String,
    pub role: Role,
    /// Équipes rejointes à l'acceptation (identifiants).
    pub teams: Vec<i64>,
    pub invited_by: String,
    pub created_at: String,
    pub expires_at: String,
    pub last_sent_at: Option<String>,
    pub accepted_at: Option<String>,
    pub revoked_at: Option<String>,
}

impl Invitation {
    pub fn is_open(&self) -> bool {
        self.accepted_at.is_none() && self.revoked_at.is_none()
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at <= stamp(now)
    }
}

/// Rôles qu'une invitation peut porter.
pub fn invitable(role: Role) -> bool {
    !role.is_wall()
}

fn stamp(at: DateTime<Utc>) -> String {
    crate::db::timestamp(at)
}

fn read(row: &SqliteRow) -> Result<Invitation> {
    let role: String = row.try_get("role")?;
    let teams: String = row.try_get("teams")?;
    Ok(Invitation {
        id: row.try_get("id")?,
        email: row.try_get("email")?,
        role: Role::parse(&role).with_context(|| format!("rôle inconnu : {role}"))?,
        teams: serde_json::from_str(&teams).unwrap_or_default(),
        invited_by: row.try_get("invited_by")?,
        created_at: row.try_get("created_at")?,
        expires_at: row.try_get("expires_at")?,
        last_sent_at: row.try_get("last_sent_at")?,
        accepted_at: row.try_get("accepted_at")?,
        revoked_at: row.try_get("revoked_at")?,
    })
}

/// Ce qu'il faut pour inviter.
pub struct NewInvitation<'a> {
    /// Déjà passée par `users::normalize_email`.
    pub email: &'a str,
    pub role: Role,
    pub teams: &'a [i64],
    pub invited_by: &'a str,
    pub hours: u32,
}

/// Crée l'invitation, et retire celle qui attendait encore pour la même
/// adresse. Le jeton n'existe en clair que dans le retour.
pub async fn create(
    pool: &SqlitePool,
    new: &NewInvitation<'_>,
    now: DateTime<Utc>,
) -> Result<(Invitation, LinkToken)> {
    let token = LinkToken::generate();
    let expires_at = now + TimeDelta::hours(i64::from(new.hours));
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    sqlx::query(
        "DELETE FROM user_invitations
         WHERE (accepted_at IS NOT NULL OR revoked_at IS NOT NULL OR expires_at < ?)
           AND created_at < ?",
    )
    .bind(stamp(now))
    .bind(stamp(now - TimeDelta::days(30)))
    .execute(&mut *tx)
    .await
    .context("purge des invitations closes")?;
    sqlx::query(
        "UPDATE user_invitations SET revoked_at = ?
         WHERE email = ? COLLATE NOCASE AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(stamp(now))
    .bind(new.email)
    .execute(&mut *tx)
    .await
    .context("retrait de l'invitation précédente")?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO user_invitations
             (token_id, token_hash, email, role, teams, invited_by, created_at, expires_at,
              last_sent_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(token.id())
    .bind(token.secret_hash())
    .bind(new.email)
    .bind(new.role.as_str())
    .bind(serde_json::to_string(new.teams)?)
    .bind(new.invited_by)
    .bind(stamp(now))
    .bind(stamp(expires_at))
    .bind(stamp(now))
    .fetch_one(&mut *tx)
    .await
    .context("création d'une invitation")?;
    tx.commit().await.context("validation de l'invitation")?;
    let invitation = get(pool, id).await?.context("invitation relue")?;
    Ok((invitation, token))
}

/// Tire un nouveau lien pour une invitation encore ouverte, avec une nouvelle
/// échéance. `None` si elle n'existe pas ou n'est plus ouverte.
pub async fn renew(
    pool: &SqlitePool,
    id: i64,
    hours: u32,
    now: DateTime<Utc>,
) -> Result<Option<(Invitation, LinkToken)>> {
    let token = LinkToken::generate();
    let expires_at = now + TimeDelta::hours(i64::from(hours));
    let done = sqlx::query(
        "UPDATE user_invitations
         SET token_id = ?, token_hash = ?, expires_at = ?, last_sent_at = ?
         WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(token.id())
    .bind(token.secret_hash())
    .bind(stamp(expires_at))
    .bind(stamp(now))
    .bind(id)
    .execute(pool)
    .await
    .context("renouvellement d'une invitation")?;
    if done.rows_affected() != 1 {
        return Ok(None);
    }
    Ok(get(pool, id).await?.map(|invitation| (invitation, token)))
}

/// Retire une invitation ouverte. Faux si elle n'existe pas ou est déjà close.
pub async fn revoke(pool: &SqlitePool, id: i64, now: DateTime<Utc>) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE user_invitations SET revoked_at = ?
         WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(stamp(now))
    .bind(id)
    .execute(pool)
    .await
    .context("retrait d'une invitation")?;
    Ok(done.rows_affected() == 1)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Invitation>> {
    let row = sqlx::query(
        "SELECT id, email, role, teams, invited_by, created_at, expires_at, last_sent_at,
                accepted_at, revoked_at
         FROM user_invitations WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("lecture d'une invitation")?;
    row.as_ref().map(read).transpose()
}

/// Les invitations ouvertes (expirées comprises : on peut les renvoyer), les
/// plus récentes d'abord.
pub async fn list_open(pool: &SqlitePool) -> Result<Vec<Invitation>> {
    let rows = sqlx::query(
        "SELECT id, email, role, teams, invited_by, created_at, expires_at, last_sent_at,
                accepted_at, revoked_at
         FROM user_invitations
         WHERE accepted_at IS NULL AND revoked_at IS NULL ORDER BY created_at DESC, id DESC",
    )
    .fetch_all(pool)
    .await
    .context("liste des invitations")?;
    rows.iter().map(read).collect()
}

/// Une invitation ouverte, avec ce qui la retrouve : c'est ce que la
/// sauvegarde emporte, pour que le lien envoyé fonctionne après restauration.
pub struct Live {
    pub invitation: Invitation,
    pub token_id: String,
    pub token_hash: Vec<u8>,
}

/// Toutes les invitations encore ouvertes et non expirées, pour la sauvegarde.
pub async fn list_live(pool: &SqlitePool, now: DateTime<Utc>) -> Result<Vec<Live>> {
    let rows = sqlx::query(
        "SELECT id, email, role, teams, invited_by, created_at, expires_at, last_sent_at,
                accepted_at, revoked_at, token_id, token_hash
         FROM user_invitations
         WHERE accepted_at IS NULL AND revoked_at IS NULL AND expires_at > ? ORDER BY id",
    )
    .bind(stamp(now))
    .fetch_all(pool)
    .await
    .context("invitations à sauvegarder")?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        out.push(Live {
            invitation: read(row)?,
            token_id: row.try_get("token_id")?,
            token_hash: row.try_get("token_hash")?,
        });
    }
    Ok(out)
}

/// L'invitation que désigne un lien, si elle est ouverte et non expirée.
pub async fn find_open(
    pool: &SqlitePool,
    token: &LinkToken,
    now: DateTime<Utc>,
) -> Result<Option<Invitation>> {
    let row = sqlx::query(
        "SELECT id, email, role, teams, invited_by, created_at, expires_at, last_sent_at,
                accepted_at, revoked_at, token_hash
         FROM user_invitations WHERE token_id = ?",
    )
    .bind(token.id())
    .fetch_optional(pool)
    .await
    .context("lecture d'une invitation")?;
    let Some(row) = row else { return Ok(None) };
    let stored: Vec<u8> = row.try_get("token_hash")?;
    let invitation = read(&row)?;
    let valid = token.matches(&stored) && invitation.is_open() && !invitation.is_expired(now);
    Ok(valid.then_some(invitation))
}

/// Ce que la personne invitée choisit.
pub struct Acceptance<'a> {
    pub username: &'a str,
    pub display_name: &'a str,
    /// `None` : compte sans mot de passe, qui attend sa première connexion SSO.
    pub password_hash: Option<&'a str>,
}

/// Pourquoi une acceptation n'a pas abouti.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Lien inconnu, déjà utilisé, retiré ou expiré : une seule réponse.
    Invalid,
    /// L'identifiant choisi est déjà pris ; l'invitation reste valable.
    UsernameTaken,
}

/// Accepte l'invitation : le compte est créé avec le rôle, l'adresse et les
/// équipes de l'invitation, et l'invitation est close — tout ou rien.
pub async fn accept(
    pool: &SqlitePool,
    token: &LinkToken,
    acceptance: &Acceptance<'_>,
    now: DateTime<Utc>,
) -> Result<Result<(i64, Invitation), Refusal>> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    let row = sqlx::query(
        "SELECT id, email, role, teams, invited_by, created_at, expires_at, last_sent_at,
                accepted_at, revoked_at, token_hash
         FROM user_invitations WHERE token_id = ?",
    )
    .bind(token.id())
    .fetch_optional(&mut *tx)
    .await
    .context("lecture d'une invitation")?;
    let Some(row) = row else { return Ok(Err(Refusal::Invalid)) };
    let stored: Vec<u8> = row.try_get("token_hash")?;
    let invitation = read(&row)?;
    if !token.matches(&stored) || !invitation.is_open() || invitation.is_expired(now) {
        return Ok(Err(Refusal::Invalid));
    }
    let inserted = sqlx::query(
        "INSERT INTO users (username, display_name, role, password_hash, email)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(username) DO NOTHING",
    )
    .bind(acceptance.username)
    .bind(acceptance.display_name)
    .bind(invitation.role.as_str())
    .bind(acceptance.password_hash)
    .bind(&invitation.email)
    .execute(&mut *tx)
    .await
    .context("création du compte invité")?;
    if inserted.rows_affected() != 1 {
        return Ok(Err(Refusal::UsernameTaken));
    }
    let user_id = inserted.last_insert_rowid();
    for team in &invitation.teams {
        // Une équipe supprimée depuis l'invitation est simplement ignorée.
        sqlx::query(
            "INSERT INTO team_members (team_id, user_id, source)
             SELECT ?, ?, 'manual' WHERE EXISTS (SELECT 1 FROM teams WHERE id = ?)
             ON CONFLICT(team_id, user_id) DO NOTHING",
        )
        .bind(team)
        .bind(user_id)
        .bind(team)
        .execute(&mut *tx)
        .await
        .context("inscription dans une équipe")?;
    }
    let closed = sqlx::query(
        "UPDATE user_invitations SET accepted_at = ?, accepted_user_id = ?
         WHERE id = ? AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(stamp(now))
    .bind(user_id)
    .bind(invitation.id)
    .execute(&mut *tx)
    .await
    .context("clôture de l'invitation")?;
    if closed.rows_affected() != 1 {
        return Ok(Err(Refusal::Invalid));
    }
    tx.commit().await.context("validation de l'acceptation")?;
    Ok(Ok((user_id, invitation)))
}

/// Identifiant proposé pour une adresse : sa partie locale, sans espace ni
/// caractère de contrôle, 64 caractères au plus.
pub fn suggested_username(email: &str) -> String {
    let local = email.rsplit_once('@').map_or(email, |(local, _)| local);
    local.chars().filter(|c| !c.is_whitespace() && !c.is_control()).take(64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_suggested_username_is_the_local_part() {
        assert_eq!(suggested_username("jane.doe@example.org"), "jane.doe");
        assert_eq!(suggested_username("no-at-sign"), "no-at-sign");
        assert_eq!(suggested_username(&format!("{}@x.org", "a".repeat(80))).len(), 64);
    }

    #[test]
    fn wall_displays_are_never_invited() {
        assert!(invitable(Role::Viewer));
        assert!(invitable(Role::Operator));
        assert!(invitable(Role::Admin));
        assert!(!invitable(Role::Wall));
    }
}
