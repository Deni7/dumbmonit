//! Changements de rôle, d'état et de profil d'un compte, appliqués en transaction.

use anyhow::{Context, Result};
use sqlx::SqlitePool;

use super::Role;

/// Ferme les sessions d'un écran mural qui quitte ce rôle, avant le
/// changement. Sa session se renouvelle tant qu'il affiche le mur : laissée
/// ouverte, elle donnerait au premier passant devant l'écran tout ce que le
/// nouveau rôle permet. Sans effet sur un autre compte.
const CLOSE_WALL_SESSIONS: &str = "DELETE FROM auth_sessions WHERE user_id = ?
     AND EXISTS (SELECT 1 FROM users WHERE id = ? AND role = 'wall')";

/// Pose un rôle livré, pour une décision des groupes d'un fournisseur
/// d'identité : le rôle personnalisé du compte ne survit qu'à `viewer` (celui
/// qu'un administrateur lui a donné tient tant que les groupes ne disent rien
/// de plus que « lecteur »).
const SET_BUILT_IN_ROLE: &str = "UPDATE users SET role = ?,
         custom_role_id = CASE WHEN ? = 'viewer' THEN custom_role_id ELSE NULL END
     WHERE id = ?";

pub async fn set_role(pool: &SqlitePool, id: i64, role: Role) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    if !role.is_wall() {
        sqlx::query(CLOSE_WALL_SESSIONS)
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("fermeture des sessions de l'écran mural")?;
    }
    sqlx::query(SET_BUILT_IN_ROLE)
        .bind(role.as_str())
        .bind(role.as_str())
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("mise à jour du rôle")?;
    tx.commit().await.context("validation de la transaction")?;
    Ok(())
}

pub async fn set_disabled(pool: &SqlitePool, id: i64, disabled: bool) -> Result<()> {
    sqlx::query(
        "UPDATE users SET disabled = ?, disabled_by_ldap = 0, disabled_by_scim = 0
         WHERE id = ?",
    )
    .bind(i64::from(disabled))
    .bind(id)
    .execute(pool)
    .await
    .context("activation ou désactivation du compte")?;
    Ok(())
}

/// Changements d'un compte appliqués d'un seul bloc.
#[derive(Default)]
pub struct AccountChanges<'a> {
    pub display_name: Option<&'a str>,
    /// `Some(None)` efface l'adresse ; `None` n'y touche pas.
    pub email: Option<Option<&'a str>>,
    /// Rôle livré choisi par un administrateur : retire le rôle personnalisé.
    pub role: Option<Role>,
    /// Rôle personnalisé choisi par un administrateur : le rôle livré devient
    /// `viewer`. Appliqué après `role`.
    pub custom_role: Option<i64>,
    pub password_hash: Option<&'a str>,
    pub disabled: Option<bool>,
    /// Ferme les sessions du compte dans la même transaction.
    pub close_sessions: bool,
}

/// Applique tous les changements ou aucun : un échec en cours de route ne laisse
/// pas un compte à moitié modifié.
pub async fn apply_changes(pool: &SqlitePool, id: i64, changes: &AccountChanges<'_>) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    if let Some(display_name) = changes.display_name {
        sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
            .bind(display_name)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("mise à jour du nom affiché")?;
    }
    if let Some(email) = changes.email {
        sqlx::query("UPDATE users SET email = ? WHERE id = ?")
            .bind(email)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("mise à jour de l'adresse électronique")?;
    }
    if let Some(role) = changes.role {
        if !role.is_wall() {
            sqlx::query(CLOSE_WALL_SESSIONS)
                .bind(id)
                .bind(id)
                .execute(&mut *tx)
                .await
                .context("fermeture des sessions de l'écran mural")?;
        }
        sqlx::query("UPDATE users SET role = ?, custom_role_id = NULL WHERE id = ?")
            .bind(role.as_str())
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("mise à jour du rôle")?;
    }
    if let Some(custom) = changes.custom_role {
        sqlx::query(CLOSE_WALL_SESSIONS)
            .bind(id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("fermeture des sessions de l'écran mural")?;
        sqlx::query("UPDATE users SET role = 'viewer', custom_role_id = ? WHERE id = ?")
            .bind(custom)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("attribution du rôle personnalisé")?;
    }
    if let Some(hash) = changes.password_hash {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("mise à jour du mot de passe")?;
    }
    if let Some(disabled) = changes.disabled {
        sqlx::query(
            "UPDATE users SET disabled = ?, disabled_by_ldap = 0, disabled_by_scim = 0
             WHERE id = ?",
        )
        .bind(i64::from(disabled))
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("activation ou désactivation du compte")?;
    }
    if changes.close_sessions {
        sqlx::query("DELETE FROM auth_sessions WHERE user_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("fermeture des sessions")?;
    }
    tx.commit().await.context("validation de la transaction")?;
    Ok(())
}
