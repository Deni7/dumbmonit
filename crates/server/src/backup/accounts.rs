//! Section `accounts` du lot : réglages des liens de compte (migration 0294,
//! clé `auth.recovery`), invitations ouvertes, et notifications personnelles
//! de chaque compte (migration 0295).
//!
//! S'y ajoutent les rôles personnalisés (migration 0297), par leur nom, et
//! leurs attributions, par identifiant de compte : un rôle est restauré avant
//! les comptes, et un compte créé par la restauration reçoit le sien ; un
//! compte qui existait déjà n'est jamais modifié.
//!
//! Une invitation part avec l'empreinte de son lien : le lien déjà envoyé
//! fonctionne encore après restauration, jusqu'à son échéance. Ses équipes
//! sont désignées par leur nom, un compte par son identifiant. Les liens de
//! réinitialisation, eux, ne partent pas : ils durent une demi-heure, comme
//! des sessions. Le journal des envois personnels non plus : c'est un constat.

use std::collections::HashMap;

use anyhow::{Context, Result};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::auth::custom_roles::{self, RoleInput};
use crate::auth::permissions::Permission;
use crate::auth::recovery::{self, RecoverySettings};
use crate::auth::users::Role;
use crate::backup::import::{RestoreOutcome, SectionReport};
use crate::notify::personal::Prefs;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleAccounts {
    /// Réglages des liens ; absents tant qu'ils n'ont jamais été changés.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub invitations: Vec<BundleInvitation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notification_prefs: Vec<BundleNotificationPrefs>,
    /// Rôles personnalisés (migration 0297).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<BundleCustomRole>,
    /// Qui a quel rôle personnalisé.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub role_assignments: Vec<BundleRoleAssignment>,
}

impl BundleAccounts {
    pub fn is_empty(&self) -> bool {
        self.recovery.is_none()
            && self.invitations.is_empty()
            && self.notification_prefs.is_empty()
            && self.roles.is_empty()
            && self.role_assignments.is_empty()
    }
}

/// Un rôle personnalisé ; ses permissions par leur nom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleCustomRole {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Le rôle personnalisé d'un compte, l'un et l'autre par leur nom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleRoleAssignment {
    pub user: String,
    pub role: String,
}

/// Une invitation ouverte ; son lien par son identifiant et l'empreinte de son
/// secret (base64), jamais le secret lui-même.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleInvitation {
    pub email: String,
    pub role: String,
    #[serde(default)]
    pub teams: Vec<String>,
    #[serde(default)]
    pub invited_by: String,
    pub created_at: String,
    pub expires_at: String,
    #[serde(default)]
    pub last_sent_at: Option<String>,
    pub token_id: String,
    pub token_hash: String,
}

/// Les choix d'un compte, désigné par son identifiant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleNotificationPrefs {
    pub user: String,
    pub prefs: Value,
}

// --------------------------------------------------------------------------
// Export
// --------------------------------------------------------------------------

fn names_of(ids: &[i64], names: &HashMap<i64, String>) -> Vec<String> {
    ids.iter().filter_map(|id| names.get(id).cloned()).collect()
}

pub async fn collect(pool: &SqlitePool) -> Result<BundleAccounts> {
    let team_names: HashMap<i64, String> = sqlx::query("SELECT id, name FROM teams")
        .fetch_all(pool)
        .await
        .context("noms des équipes")?
        .iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("name")?)))
        .collect::<Result<_>>()?;
    let user_names: HashMap<i64, String> = sqlx::query("SELECT id, username FROM users")
        .fetch_all(pool)
        .await
        .context("noms des comptes")?
        .iter()
        .map(|row| Ok((row.try_get("id")?, row.try_get("username")?)))
        .collect::<Result<_>>()?;

    let invitations: Vec<BundleInvitation> = crate::auth::invitations::list_live(pool, Utc::now())
        .await?
        .into_iter()
        .map(|live| BundleInvitation {
            email: live.invitation.email,
            role: live.invitation.role.as_str().to_string(),
            teams: names_of(&live.invitation.teams, &team_names),
            invited_by: live.invitation.invited_by,
            created_at: live.invitation.created_at,
            expires_at: live.invitation.expires_at,
            last_sent_at: live.invitation.last_sent_at,
            token_id: live.token_id,
            token_hash: BASE64.encode(live.token_hash),
        })
        .collect();

    let notification_prefs: Vec<BundleNotificationPrefs> = crate::notify::personal::all(pool)
        .await?
        .into_iter()
        .filter_map(|(id, prefs)| {
            let user = user_names.get(&id)?.clone();
            Some(serde_json::to_value(prefs).map(|prefs| BundleNotificationPrefs { user, prefs }))
        })
        .collect::<Result<_, _>>()?;

    let roles: Vec<BundleCustomRole> = custom_roles::list(pool)
        .await?
        .into_iter()
        .map(|role| BundleCustomRole {
            name: role.name,
            description: role.description,
            permissions: role.permissions.iter().map(|p| p.as_str().to_string()).collect(),
        })
        .collect();
    let role_assignments: Vec<BundleRoleAssignment> = sqlx::query(
        "SELECT u.username AS username, r.name AS role_name
         FROM users u JOIN custom_roles r ON r.id = u.custom_role_id
         WHERE u.role = 'viewer' ORDER BY u.id",
    )
    .fetch_all(pool)
    .await
    .context("attributions des rôles personnalisés")?
    .iter()
    .map(|row| {
        Ok(BundleRoleAssignment {
            user: row.try_get("username")?,
            role: row.try_get("role_name")?,
        })
    })
    .collect::<Result<_>>()?;

    Ok(BundleAccounts {
        recovery: crate::auth::settings::get::<Value>(pool, recovery::SETTINGS_KEY).await?,
        invitations,
        notification_prefs,
        roles,
        role_assignments,
    })
}

// --------------------------------------------------------------------------
// Restauration
// --------------------------------------------------------------------------

/// Restaure les trois parties, chacune son compte rendu.
pub async fn restore(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
) -> Result<Vec<SectionReport>> {
    Ok(vec![
        restore_recovery(tx, bundle).await?,
        restore_invitations(tx, bundle).await?,
        restore_prefs(tx, bundle).await?,
    ])
}

/// Rôles personnalisés : créés, ou remplacés par ceux du lot quand ils
/// diffèrent. Avant les comptes, qui les citent ([`assigned_role`]). Une
/// permission inconnue de cette version (lot plus récent) est laissée de côté.
pub async fn restore_roles(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("custom_roles");
    for role in &bundle.roles {
        let (known, unknown): (Vec<String>, Vec<String>) = role
            .permissions
            .iter()
            .cloned()
            .partition(|name| Permission::parse(name.trim()).is_some());
        if !unknown.is_empty() {
            let unknown = unknown.join(", ");
            let note = format!("role \"{}\": unknown permissions left out ({unknown})", role.name);
            report.notes.push(note);
        }
        let input = RoleInput {
            name: role.name.clone(),
            description: role.description.clone(),
            permissions: known,
        };
        let checked = match input.check() {
            Ok(checked) => checked,
            Err(why) => {
                report.skipped += 1;
                report.notes.push(format!("role \"{}\" is unreadable: {why}", role.name));
                continue;
            }
        };
        let stored_permissions = custom_roles::stored(&checked.permissions);
        let existing = sqlx::query(
            "SELECT id, description, permissions FROM custom_roles WHERE name = ? COLLATE NOCASE",
        )
        .bind(&checked.name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche d'un rôle personnalisé")?;
        let outcome = match existing {
            Some(row) => {
                let description: String = row.try_get("description")?;
                let permissions: String = row.try_get("permissions")?;
                let same = description == checked.description
                    && crate::auth::permissions::parse_stored(&permissions) == checked.permissions;
                if same {
                    RestoreOutcome::Skipped
                } else {
                    sqlx::query(
                        "UPDATE custom_roles SET description = ?, permissions = ?,
                                updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
                         WHERE id = ?",
                    )
                    .bind(&checked.description)
                    .bind(&stored_permissions)
                    .bind(row.try_get::<i64, _>("id")?)
                    .execute(&mut **tx)
                    .await
                    .context("mise à jour d'un rôle personnalisé")?;
                    RestoreOutcome::Updated
                }
            }
            None => {
                sqlx::query(
                    "INSERT INTO custom_roles (name, description, permissions) VALUES (?, ?, ?)",
                )
                .bind(&checked.name)
                .bind(&checked.description)
                .bind(&stored_permissions)
                .execute(&mut **tx)
                .await
                .context("restauration d'un rôle personnalisé")?;
                RestoreOutcome::Created
            }
        };
        report.count(outcome);
    }
    Ok(report)
}

/// Le rôle personnalisé que le lot donne au compte `username`, s'il en a un et
/// s'il existe ici, pour un compte créé par la restauration avec le rôle livré
/// `role` (seul `viewer` en prend un).
pub async fn assigned_role(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
    username: &str,
    role: &str,
) -> Result<Option<i64>> {
    if role != "viewer" {
        return Ok(None);
    }
    let Some(assignment) =
        bundle.role_assignments.iter().find(|a| a.user.eq_ignore_ascii_case(username))
    else {
        return Ok(None);
    };
    sqlx::query_scalar::<_, i64>("SELECT id FROM custom_roles WHERE name = ? COLLATE NOCASE")
        .bind(assignment.role.trim())
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du rôle personnalisé d'un compte")
}

/// Réglages des liens : remplacés par ceux du lot.
async fn restore_recovery(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("account_links");
    let Some(raw) = &bundle.recovery else { return Ok(report) };
    let parsed = serde_json::from_value::<RecoverySettings>(raw.clone()).ok();
    let Some(settings) = parsed.filter(|settings| settings.validate().is_ok()) else {
        report.skipped += 1;
        report.notes.push("The account link settings of the backup are unreadable".into());
        return Ok(report);
    };
    let value = serde_json::to_vec(&settings)?;
    let existing: Option<Vec<u8>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(recovery::SETTINGS_KEY)
        .fetch_optional(&mut **tx)
        .await
        .context("lecture des réglages des liens de compte")?;
    let unchanged = match &existing {
        Some(stored) => serde_json::from_slice::<RecoverySettings>(stored).ok() == Some(settings),
        None => settings == RecoverySettings::default(),
    };
    if unchanged {
        report.skipped += 1;
        return Ok(report);
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(recovery::SETTINGS_KEY)
    .bind(&value)
    .execute(&mut **tx)
    .await
    .context("écriture des réglages des liens de compte")?;
    let outcome =
        if existing.is_some() { RestoreOutcome::Updated } else { RestoreOutcome::Created };
    report.count(outcome);
    Ok(report)
}

/// Invitations : créées si aucune n'attend déjà pour la même adresse. Une
/// invitation échue depuis la sauvegarde n'est pas recréée.
async fn restore_invitations(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("invitations");
    let now = crate::db::now();
    let teams: HashMap<String, i64> = sqlx::query("SELECT id, name FROM teams")
        .fetch_all(&mut **tx)
        .await
        .context("équipes de l'instance")?
        .iter()
        .map(|row| Ok((row.try_get("name")?, row.try_get("id")?)))
        .collect::<Result<_>>()?;
    for invitation in &bundle.invitations {
        let label = format!("invitation of {}", invitation.email);
        let email = crate::auth::users::normalize_email(&invitation.email).ok().flatten();
        let role = Role::parse(&invitation.role).filter(|role| !role.is_wall());
        let hash = BASE64.decode(invitation.token_hash.trim()).ok().filter(|hash| hash.len() == 32);
        let id = &invitation.token_id;
        let token_ok = id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit());
        let (Some(email), Some(role), Some(hash), true) = (email, role, hash, token_ok) else {
            report.skipped += 1;
            report.notes.push(format!("{label} is unreadable, left out"));
            continue;
        };
        if invitation.expires_at.as_str() <= now.as_str() {
            report.skipped += 1;
            report.notes.push(format!("{label} has expired since the backup, left out"));
            continue;
        }
        let open: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM user_invitations
             WHERE (email = ? COLLATE NOCASE AND accepted_at IS NULL AND revoked_at IS NULL)
                OR token_id = ?",
        )
        .bind(&email)
        .bind(&invitation.token_id)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche d'une invitation")?;
        let member: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM users WHERE email = ? COLLATE NOCASE OR username = ? COLLATE NOCASE",
        )
        .bind(&email)
        .bind(&email)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche d'un compte")?;
        if open.is_some() || member.is_some() {
            report.count(RestoreOutcome::Skipped);
            continue;
        }
        let mut ids = Vec::new();
        for name in &invitation.teams {
            match teams.get(name) {
                Some(id) => ids.push(*id),
                None => {
                    report.notes.push(format!("{label}: team \"{name}\" not found, left out"));
                }
            }
        }
        sqlx::query(
            "INSERT INTO user_invitations
                 (token_id, token_hash, email, role, teams, invited_by, created_at, expires_at,
                  last_sent_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(invitation.token_id.to_ascii_lowercase())
        .bind(hash)
        .bind(&email)
        .bind(role.as_str())
        .bind(serde_json::to_string(&ids)?)
        .bind(&invitation.invited_by)
        .bind(&invitation.created_at)
        .bind(&invitation.expires_at)
        .bind(&invitation.last_sent_at)
        .execute(&mut **tx)
        .await
        .context("restauration d'une invitation")?;
        report.count(RestoreOutcome::Created);
    }
    Ok(report)
}

/// Notifications personnelles : remplacées par celles du lot, compte par compte.
async fn restore_prefs(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &BundleAccounts,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("notification_prefs");
    for entry in &bundle.notification_prefs {
        let user: Option<i64> =
            sqlx::query_scalar("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
                .bind(&entry.user)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche d'un compte")?;
        let Some(user) = user else {
            report.skipped += 1;
            let note = format!("account \"{}\" not found, its notifications left out", entry.user);
            report.notes.push(note);
            continue;
        };
        let parsed = serde_json::from_value::<Prefs>(entry.prefs.clone()).ok();
        let Some(prefs) = parsed.and_then(|prefs| prefs.validated().ok()) else {
            report.skipped += 1;
            report.notes.push(format!("the notifications of \"{}\" are unreadable", entry.user));
            continue;
        };
        let stored: Option<String> =
            sqlx::query_scalar("SELECT prefs FROM user_notification_prefs WHERE user_id = ?")
                .bind(user)
                .fetch_optional(&mut **tx)
                .await
                .context("lecture des notifications personnelles")?;
        let same = stored
            .as_deref()
            .and_then(|raw| serde_json::from_str::<Prefs>(raw).ok())
            .is_some_and(|saved| saved == prefs);
        if same {
            report.count(RestoreOutcome::Skipped);
            continue;
        }
        sqlx::query(
            "INSERT INTO user_notification_prefs (user_id, prefs) VALUES (?, ?)
             ON CONFLICT(user_id) DO UPDATE SET prefs = excluded.prefs,
                 updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')",
        )
        .bind(user)
        .bind(serde_json::to_string(&prefs)?)
        .execute(&mut **tx)
        .await
        .context("restauration des notifications personnelles")?;
        let outcome =
            if stored.is_some() { RestoreOutcome::Updated } else { RestoreOutcome::Created };
        report.count(outcome);
    }
    Ok(report)
}
