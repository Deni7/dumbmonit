//! Comptes utilisateurs : lecture et écriture de la table `users`.
//!
//! Trois rôles, ordonnés : `viewer` regarde, `operator` regarde et traite les
//! alertes (acquitter, ignorer, mettre en sourdine), `admin` fait tout. Un
//! quatrième, `wall`, est à part : un écran mural, qui ne voit que le mode mur
//! (`/wall`) et ne lit que les routes dont il a besoin (`wall::WALL_ROUTES`).
//! Un compte `viewer` peut aussi recevoir un rôle personnalisé
//! ([`super::custom_roles`]) : des permissions choisies dans un catalogue fixe
//! ([`super::permissions`]), relues avec le compte à chaque requête.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use crate::auth::custom_roles::Assigned;
use crate::auth::permissions;

/// Rôle d'un compte. Sérialisé en minuscules, tel qu'il est stocké et exposé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Operator,
    Viewer,
    /// Écran mural : le mode mur et rien d'autre, en lecture.
    Wall,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
            Self::Wall => "wall",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "operator" => Some(Self::Operator),
            "viewer" => Some(Self::Viewer),
            "wall" => Some(Self::Wall),
            _ => None,
        }
    }

    pub fn is_admin(self) -> bool {
        matches!(self, Self::Admin)
    }

    /// Compte d'écran mural : le garde d'API ne lui ouvre que `WALL_ROUTES`.
    pub fn is_wall(self) -> bool {
        matches!(self, Self::Wall)
    }

    /// Peut traiter les alertes : opérateur ou administrateur. Ce que cela
    /// ouvre exactement est la liste `OPERATOR_ROUTES` du garde d'API.
    pub fn can_operate(self) -> bool {
        matches!(self, Self::Admin | Self::Operator)
    }
}

/// Un compte, tel qu'il est lu en base. L'empreinte du mot de passe n'est jamais
/// exposée au-delà de ce module et de la vérification de connexion.
#[derive(Debug, Clone)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub password_hash: Option<String>,
    pub oidc_subject: Option<String>,
    pub oidc_issuer: Option<String>,
    pub created_at: String,
    pub last_login_at: Option<String>,
    pub disabled: bool,
    /// Secret TOTP chiffré, `None` sans enrôlement (voir [`crate::auth::totp`]).
    pub totp_secret: Option<Vec<u8>>,
    /// Second facteur exigé à la connexion : l'enrôlement a été confirmé.
    pub totp_enabled: bool,
    /// Identité stable dans l'annuaire LDAP (voir [`crate::auth::ldap`]).
    pub ldap_id: Option<String>,
    /// Dernier DN connu de l'entrée, pour le diagnostic.
    pub ldap_dn: Option<String>,
    /// Désactivé par la connexion LDAP (groupes perdus), pas par un administrateur.
    pub disabled_by_ldap: bool,
    /// Identité SAML : `NameID` persistant et `entityID` de l'IdP qui l'a émis
    /// (voir [`crate::auth::saml`]).
    pub saml_subject: Option<String>,
    pub saml_issuer: Option<String>,
    /// Adresse électronique facultative, pour savoir qui joindre. Jamais un
    /// moyen de se connecter.
    pub email: Option<String>,
    /// Rôle personnalisé, seulement pour un compte dont le rôle livré est
    /// `viewer` : sur un autre rôle, il est ignoré à la lecture.
    pub custom_role: Option<Assigned>,
}

impl User {
    /// Nom à afficher : le nom choisi, sinon l'identifiant.
    pub fn label(&self) -> &str {
        if self.display_name.trim().is_empty() { &self.username } else { &self.display_name }
    }

    /// Méthode de connexion annoncée à l'interface : « password » dès qu'un mot
    /// de passe existe, « ldap » pour un compte de l'annuaire, « saml » pour un
    /// compte lié à un IdP SAML seulement, « oidc » sinon.
    pub fn auth_method(&self) -> &'static str {
        if self.password_hash.is_some() {
            "password"
        } else if self.ldap_id.is_some() {
            "ldap"
        } else if self.saml_subject.is_some() && self.oidc_subject.is_none() {
            "saml"
        } else {
            "oidc"
        }
    }
}

/// Requête de lecture des comptes, rôle personnalisé compris, suivie de `$rest`
/// (les colonnes de `users` y sont préfixées par `u.`).
macro_rules! select_users {
    ($rest:literal) => {
        concat!(
            "SELECT u.id AS id, u.username AS username, u.display_name AS display_name,
                    u.role AS role, u.password_hash AS password_hash,
                    u.oidc_subject AS oidc_subject, u.oidc_issuer AS oidc_issuer,
                    u.created_at AS created_at, u.last_login_at AS last_login_at,
                    u.disabled AS disabled, u.totp_secret AS totp_secret,
                    u.totp_enabled AS totp_enabled, u.ldap_id AS ldap_id, u.ldap_dn AS ldap_dn,
                    u.disabled_by_ldap AS disabled_by_ldap, u.saml_subject AS saml_subject,
                    u.saml_issuer AS saml_issuer, u.email AS email,
                    r.id AS custom_role_id, r.name AS custom_role_name,
                    r.permissions AS custom_role_permissions
             FROM users u LEFT JOIN custom_roles r ON r.id = u.custom_role_id ",
            $rest
        )
    };
}

fn read(row: SqliteRow) -> Result<User> {
    let role: String = row.try_get("role")?;
    let role = Role::parse(&role).with_context(|| format!("rôle inconnu en base : {role}"))?;
    let custom_id: Option<i64> = row.try_get("custom_role_id")?;
    let custom_role = match custom_id {
        Some(id) if role == Role::Viewer => {
            let raw: Option<String> = row.try_get("custom_role_permissions")?;
            Some(Assigned {
                id,
                name: row.try_get::<Option<String>, _>("custom_role_name")?.unwrap_or_default(),
                permissions: permissions::parse_stored(raw.as_deref().unwrap_or("[]")),
            })
        }
        _ => None,
    };
    Ok(User {
        id: row.try_get("id")?,
        username: row.try_get("username")?,
        display_name: row.try_get("display_name")?,
        role,
        password_hash: row.try_get("password_hash")?,
        oidc_subject: row.try_get("oidc_subject")?,
        oidc_issuer: row.try_get("oidc_issuer")?,
        created_at: row.try_get("created_at")?,
        last_login_at: row.try_get("last_login_at")?,
        disabled: row.try_get::<i64, _>("disabled")? != 0,
        totp_secret: row.try_get("totp_secret")?,
        totp_enabled: row.try_get::<i64, _>("totp_enabled")? != 0,
        ldap_id: row.try_get("ldap_id")?,
        ldap_dn: row.try_get("ldap_dn")?,
        disabled_by_ldap: row.try_get::<i64, _>("disabled_by_ldap")? != 0,
        saml_subject: row.try_get("saml_subject")?,
        saml_issuer: row.try_get("saml_issuer")?,
        email: row.try_get("email")?,
        custom_role,
    })
}

pub async fn count(pool: &SqlitePool) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM users")
        .fetch_one(pool)
        .await
        .context("comptage des comptes")?;
    Ok(row.try_get("n")?)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<User>> {
    let rows = sqlx::query(select_users!("ORDER BY u.username COLLATE NOCASE"))
    .fetch_all(pool)
    .await
    .context("liste des comptes")?;
    rows.into_iter().map(read).collect()
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<User>> {
    let row = sqlx::query(select_users!("WHERE u.id = ?"))
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("lecture du compte")?;
    row.map(read).transpose()
}

pub async fn by_username(pool: &SqlitePool, username: &str) -> Result<Option<User>> {
    let row = sqlx::query(select_users!("WHERE u.username = ?"))
    .bind(username)
    .fetch_optional(pool)
    .await
    .context("recherche du compte par identifiant")?;
    row.map(read).transpose()
}

pub async fn by_oidc(pool: &SqlitePool, issuer: &str, subject: &str) -> Result<Option<User>> {
    let row = sqlx::query(select_users!("WHERE u.oidc_issuer = ? AND u.oidc_subject = ?"))
    .bind(issuer)
    .bind(subject)
    .fetch_optional(pool)
    .await
    .context("recherche du compte par identité OIDC")?;
    row.map(read).transpose()
}

pub async fn by_saml(pool: &SqlitePool, issuer: &str, subject: &str) -> Result<Option<User>> {
    let row = sqlx::query(select_users!("WHERE u.saml_issuer = ? AND u.saml_subject = ?"))
    .bind(issuer)
    .bind(subject)
    .fetch_optional(pool)
    .await
    .context("recherche du compte par identité SAML")?;
    row.map(read).transpose()
}

/// Rattache un compte existant à une identité SAML.
pub async fn link_saml(pool: &SqlitePool, id: i64, issuer: &str, subject: &str) -> Result<()> {
    sqlx::query("UPDATE users SET saml_issuer = ?, saml_subject = ? WHERE id = ?")
        .bind(issuer)
        .bind(subject)
        .bind(id)
        .execute(pool)
        .await
        .context("rattachement SAML")?;
    Ok(())
}

/// Crée un compte sans mot de passe, lié d'emblée à son identité SAML quand
/// elle est stable (`subject`). `None` si l'identifiant est déjà pris.
pub async fn insert_saml(
    pool: &SqlitePool,
    username: &str,
    display_name: &str,
    role: Role,
    identity: Option<(&str, &str)>,
) -> Result<Option<i64>> {
    let (issuer, subject) = identity.unzip();
    let result = sqlx::query(
        "INSERT INTO users (username, display_name, role, saml_issuer, saml_subject)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(username) DO NOTHING",
    )
    .bind(username)
    .bind(display_name)
    .bind(role.as_str())
    .bind(issuer)
    .bind(subject)
    .execute(pool)
    .await
    .context("création du compte SAML")?;
    Ok((result.rows_affected() == 1).then(|| result.last_insert_rowid()))
}

pub async fn by_ldap(pool: &SqlitePool, ldap_id: &str) -> Result<Option<User>> {
    let row = sqlx::query(select_users!("WHERE u.ldap_id = ?"))
    .bind(ldap_id)
    .fetch_optional(pool)
    .await
    .context("recherche du compte par identité LDAP")?;
    row.map(read).transpose()
}

/// Ce que l'annuaire dit d'un compte à chaque connexion.
pub struct LdapSync<'a> {
    pub ldap_id: &'a str,
    pub ldap_dn: &'a str,
    /// Vide : le nom affiché du compte reste tel quel.
    pub display_name: &'a str,
    /// `None` : les groupes ne décident pas du rôle, celui du compte reste.
    pub role: Option<Role>,
}

/// Rattache un compte à son entrée d'annuaire et le met à jour : identité, DN,
/// nom affiché, rôle. Réactive un compte que seule la perte de ses groupes
/// avait désactivé ; un compte désactivé par un administrateur le reste.
pub async fn sync_ldap(pool: &SqlitePool, id: i64, sync: &LdapSync<'_>) -> Result<()> {
    sqlx::query(
        "UPDATE users SET ldap_id = ?, ldap_dn = ?,
                display_name = CASE WHEN ? = '' THEN display_name ELSE ? END,
                role = COALESCE(?, role),
                custom_role_id = CASE WHEN COALESCE(?, role) = 'viewer'
                                      THEN custom_role_id ELSE NULL END,
                disabled = CASE WHEN disabled_by_ldap = 1 THEN 0 ELSE disabled END,
                disabled_by_ldap = 0
         WHERE id = ?",
    )
    .bind(sync.ldap_id)
    .bind(sync.ldap_dn)
    .bind(sync.display_name)
    .bind(sync.display_name)
    .bind(sync.role.map(Role::as_str))
    .bind(sync.role.map(Role::as_str))
    .bind(id)
    .execute(pool)
    .await
    .context("mise à jour du compte LDAP")?;
    Ok(())
}

/// Désactive un compte LDAP qui n'appartient plus à aucun groupe autorisé, et
/// ferme ses sessions dans la même transaction.
pub async fn disable_by_ldap(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    sqlx::query(
        "UPDATE users SET disabled = 1, disabled_by_ldap = 1 WHERE id = ? AND disabled = 0",
    )
    .bind(id)
    .execute(&mut *tx)
    .await
    .context("désactivation du compte LDAP")?;
    sqlx::query("DELETE FROM auth_sessions WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await
        .context("fermeture des sessions")?;
    tx.commit().await.context("validation de la transaction")?;
    Ok(())
}

/// Le seul compte local, s'il n'y en a qu'un : c'est ce qui permet à l'ancien
/// formulaire de connexion (mot de passe seul) de continuer à fonctionner.
pub async fn sole_password_user(pool: &SqlitePool) -> Result<Option<User>> {
    let rows = sqlx::query(select_users!("WHERE u.password_hash IS NOT NULL LIMIT 2"))
    .fetch_all(pool)
    .await
    .context("recherche du compte local unique")?;
    if rows.len() != 1 {
        return Ok(None);
    }
    rows.into_iter().next().map(read).transpose()
}

/// Ce qu'il faut pour créer un compte.
pub struct NewUser<'a> {
    pub username: &'a str,
    pub display_name: &'a str,
    pub role: Role,
    pub password_hash: Option<&'a str>,
    pub oidc: Option<(&'a str, &'a str)>,
}

/// Crée un compte et renvoie son identifiant, ou `None` si l'identifiant est
/// déjà pris. Le conflit est laissé à la contrainte d'unicité : c'est la seule
/// façon de fermer la fenêtre entre « vérifier » et « écrire ».
pub async fn insert(pool: &SqlitePool, user: NewUser<'_>) -> Result<Option<i64>> {
    let (issuer, subject) = match user.oidc {
        Some((issuer, subject)) => (Some(issuer), Some(subject)),
        None => (None, None),
    };
    let result = sqlx::query(
        "INSERT INTO users (username, display_name, role, password_hash, oidc_issuer, oidc_subject)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(username) DO NOTHING",
    )
    .bind(user.username)
    .bind(user.display_name)
    .bind(user.role.as_str())
    .bind(user.password_hash)
    .bind(issuer)
    .bind(subject)
    .execute(pool)
    .await
    .context("création du compte")?;

    Ok((result.rows_affected() == 1).then(|| result.last_insert_rowid()))
}

/// Crée le tout premier compte, seulement si la table est vide.
///
/// La condition est dans la requête même : deux premières configurations
/// simultanées ne peuvent pas créer deux administrateurs.
pub async fn insert_first_admin(pool: &SqlitePool, username: &str, hash: &str) -> Result<bool> {
    let result = sqlx::query(
        "INSERT INTO users (username, display_name, role, password_hash)
         SELECT ?, '', 'admin', ?
         WHERE NOT EXISTS (SELECT 1 FROM users)",
    )
    .bind(username)
    .bind(hash)
    .execute(pool)
    .await
    .context("création du premier administrateur")?;
    Ok(result.rows_affected() == 1)
}

pub async fn set_password_hash(pool: &SqlitePool, id: i64, hash: &str) -> Result<()> {
    sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(pool)
        .await
        .context("mise à jour du mot de passe")?;
    Ok(())
}

pub async fn set_display_name(pool: &SqlitePool, id: i64, display_name: &str) -> Result<()> {
    sqlx::query("UPDATE users SET display_name = ? WHERE id = ?")
        .bind(display_name)
        .bind(id)
        .execute(pool)
        .await
        .context("mise à jour du nom affiché")?;
    Ok(())
}

/// Adresse électronique du compte, déjà passée par [`normalize_email`] ;
/// `None` l'efface.
pub async fn set_email(pool: &SqlitePool, id: i64, email: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE users SET email = ? WHERE id = ?")
        .bind(email)
        .bind(id)
        .execute(pool)
        .await
        .context("mise à jour de l'adresse électronique")?;
    Ok(())
}

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

/// Rattache un compte existant à son identité chez le fournisseur.
pub async fn link_oidc(pool: &SqlitePool, id: i64, issuer: &str, subject: &str) -> Result<()> {
    sqlx::query("UPDATE users SET oidc_issuer = ?, oidc_subject = ? WHERE id = ?")
        .bind(issuer)
        .bind(subject)
        .bind(id)
        .execute(pool)
        .await
        .context("rattachement de l'identité OIDC")?;
    Ok(())
}

/// Dépose un secret TOTP chiffré, en attente de confirmation : le second
/// facteur n'est pas encore exigé.
pub async fn set_totp_pending(pool: &SqlitePool, id: i64, secret: &[u8]) -> Result<()> {
    sqlx::query("UPDATE users SET totp_secret = ?, totp_enabled = 0 WHERE id = ?")
        .bind(secret)
        .bind(id)
        .execute(pool)
        .await
        .context("enrôlement du second facteur")?;
    Ok(())
}

/// Confirme l'enrôlement : le second facteur est désormais exigé.
pub async fn set_totp_enabled(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query("UPDATE users SET totp_enabled = 1 WHERE id = ? AND totp_secret IS NOT NULL")
        .bind(id)
        .execute(pool)
        .await
        .context("activation du second facteur")?;
    Ok(())
}

/// Retire le second facteur, secret et codes de secours compris.
pub async fn clear_totp(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    sqlx::query("UPDATE users SET totp_secret = NULL, totp_enabled = 0 WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await.context("désactivation du second facteur")?;
    Ok(())
}

/// Remplace les codes de secours d'un compte par un nouveau jeu d'empreintes.
pub async fn replace_recovery_codes(pool: &SqlitePool, id: i64, hashes: &[Vec<u8>]) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    sqlx::query("DELETE FROM totp_recovery_codes WHERE user_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    for hash in hashes {
        sqlx::query("INSERT INTO totp_recovery_codes (user_id, code_hash) VALUES (?, ?)")
            .bind(id)
            .bind(hash)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await.context("enregistrement des codes de secours")?;
    Ok(())
}

/// Consomme un code de secours s'il est inutilisé : `true` quand il a servi.
///
/// La condition est dans la requête : deux connexions simultanées avec le même
/// code ne peuvent pas passer toutes les deux.
pub async fn consume_recovery_code(pool: &SqlitePool, id: i64, hash: &[u8]) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE totp_recovery_codes
         SET used_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = (
             SELECT id FROM totp_recovery_codes
             WHERE user_id = ? AND code_hash = ? AND used_at IS NULL
             LIMIT 1
         )",
    )
    .bind(id)
    .bind(hash)
    .execute(pool)
    .await
    .context("consommation d'un code de secours")?;
    Ok(result.rows_affected() == 1)
}

/// Nombre de codes de secours encore utilisables.
pub async fn recovery_codes_left(pool: &SqlitePool, id: i64) -> Result<i64> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS n FROM totp_recovery_codes WHERE user_id = ? AND used_at IS NULL",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .context("comptage des codes de secours")?;
    Ok(row.try_get("n")?)
}

pub async fn touch_login(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE users SET last_login_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("horodatage de la connexion")?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<()> {
    // Les sessions suivent par `ON DELETE CASCADE`.
    sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("suppression du compte")?;
    Ok(())
}

/// Nombre d'administrateurs actifs : c'est lui que protège la règle du « dernier
/// administrateur » — une instance sans admin ne s'administre plus.
pub async fn active_admin_count(pool: &SqlitePool) -> Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM users WHERE role = 'admin' AND disabled = 0")
        .fetch_one(pool)
        .await
        .context("comptage des administrateurs")?;
    Ok(row.try_get("n")?)
}

/// Efface tous les comptes et toutes les sessions.
///
/// Réservé au démarrage, sur demande explicite de l'environnement : c'est l'issue
/// de secours d'une instance dont plus personne n'a le mot de passe, et elle exige
/// d'avoir la main sur le conteneur — ce qui vaut bien un mot de passe. Les
/// réglages OIDC, eux, survivent.
pub async fn reset_all(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    sqlx::query("DELETE FROM auth_sessions").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM users").execute(&mut *tx).await?;
    tx.commit().await.context("réinitialisation des comptes")?;
    Ok(())
}

/// Vérifie la forme d'un identifiant : court, sans espace, en ASCII imprimable.
///
/// La règle est volontairement souple — un courriel passe — parce qu'un
/// fournisseur OIDC peut envoyer l'un ou l'autre et qu'on voudra les faire
/// correspondre.
pub fn validate_username(username: &str) -> Result<(), String> {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return Err("Enter a username.".into());
    }
    if trimmed.chars().count() > 64 {
        return Err("The username must be 64 characters or fewer.".into());
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The username cannot contain spaces.".into());
    }
    Ok(())
}

/// Longueur maximale d'une adresse électronique (RFC 5321 : 254 utiles).
pub const EMAIL_MAX_LEN: usize = 254;

/// Vérifie et normalise une adresse électronique saisie : vide vaut « aucune ».
///
/// La vérification reste celle qu'un humain attend — une partie locale, un `@`,
/// un domaine avec un point, ni espace ni caractère de contrôle — sans prétendre
/// valider la RFC 5322 : l'adresse ne sert qu'à savoir qui joindre.
pub fn normalize_email(raw: &str) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let invalid = || format!("\"{trimmed}\" is not a valid email address.");
    if trimmed.chars().count() > EMAIL_MAX_LEN {
        return Err(format!("The email address must be {EMAIL_MAX_LEN} characters or fewer."));
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    let Some((local, domain)) = trimmed.rsplit_once('@') else { return Err(invalid()) };
    let domain_ok = domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !domain.contains('@');
    if local.is_empty() || local.contains('@') || !domain_ok {
        return Err(invalid());
    }
    Ok(Some(format!("{local}@{}", domain.to_lowercase())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_optional_checked_and_normalised() {
        assert_eq!(normalize_email(""), Ok(None));
        assert_eq!(normalize_email("   "), Ok(None));
        assert_eq!(
            normalize_email(" Jane.Doe@Example.ORG "),
            Ok(Some("Jane.Doe@example.org".to_string()))
        );
        for bad in [
            "jane",
            "jane@",
            "@example.org",
            "jane@localhost",
            "ja ne@example.org",
            "jane@example..org",
            "jane@.example.org",
            "a@b@example.org",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
        assert!(normalize_email(&format!("{}@example.org", "a".repeat(250))).is_err());
    }

    #[test]
    fn roles_round_trip_through_their_text_form() {
        for role in [Role::Admin, Role::Operator, Role::Viewer, Role::Wall] {
            assert_eq!(Role::parse(role.as_str()), Some(role));
            assert_eq!(serde_json::to_value(role).unwrap(), serde_json::json!(role.as_str()));
        }
        assert_eq!(Role::parse("root"), None);
        assert!(Role::Admin.is_admin());
        assert!(!Role::Viewer.is_admin());
        assert!(Role::Wall.is_wall());
        assert!(!Role::Wall.is_admin() && !Role::Wall.can_operate());
        assert!(!Role::Viewer.is_wall());
    }

    #[test]
    fn usernames_are_short_and_without_spaces() {
        assert!(validate_username("admin").is_ok());
        assert!(validate_username("jane.doe@example.org").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("jane doe").is_err());
        assert!(validate_username(&"a".repeat(65)).is_err());
    }
}
