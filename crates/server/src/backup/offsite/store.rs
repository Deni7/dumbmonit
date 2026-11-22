//! Table `backup_destinations` (migration 0286) : réglages en clair, secrets
//! chiffrés par la clé d'instance, dernier résultat d'envoi.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::{Kind, Secrets, Settings};
use crate::crypto::Cipher;

/// Dernier envoi vers une destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LastResult {
    /// UTC sans suffixe.
    pub at: String,
    pub ok: bool,
    /// Nom de la sauvegarde envoyée.
    pub file: String,
    /// Octets envoyés (base et, si demandée, clé).
    pub bytes: i64,
    pub duration_ms: i64,
    /// Cause de l'échec ; sur un succès, une remarque (élagage raté).
    pub error: Option<String>,
}

/// Une destination telle que la base la garde.
#[derive(Debug, Clone)]
pub struct Destination {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub keep: u32,
    pub include_key: bool,
    pub settings: Settings,
    pub has_secret: bool,
    /// Noms des champs secrets enregistrés (jamais leurs valeurs).
    pub saved_secrets: Vec<String>,
    pub last: Option<LastResult>,
    pub last_success_at: Option<String>,
}

/// Ce que l'API rend d'une destination : aucun secret.
#[derive(Debug, Clone, Serialize)]
pub struct DestinationView {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    pub enabled: bool,
    pub keep: u32,
    pub include_key: bool,
    pub settings: Value,
    pub has_secret: bool,
    pub saved_secrets: Vec<String>,
    pub last: Option<LastResult>,
    pub last_success_at: Option<String>,
}

impl Destination {
    pub fn view(&self) -> DestinationView {
        DestinationView {
            id: self.id,
            name: self.name.clone(),
            kind: self.settings.kind(),
            enabled: self.enabled,
            keep: self.keep,
            include_key: self.include_key,
            settings: self.settings.to_value(),
            has_secret: self.has_secret,
            saved_secrets: self.saved_secrets.clone(),
            last: self.last.clone(),
            last_success_at: self.last_success_at.clone(),
        }
    }
}

/// Champs écrits par une création ou une modification.
pub struct Input<'a> {
    pub name: &'a str,
    pub enabled: bool,
    pub keep: u32,
    pub include_key: bool,
    pub settings: &'a Settings,
}

const COLUMNS: &str = "id, name, kind, enabled, keep, include_key, settings, secrets, \
                       secret_fields, last_at, last_ok, last_file, last_bytes, last_duration_ms, \
                       last_error, last_success_at";

fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Destination> {
    let kind: String = row.try_get("kind")?;
    let kind = Kind::parse(&kind).with_context(|| format!("unknown destination kind {kind}"))?;
    let raw: String = row.try_get("settings")?;
    let value: Value = serde_json::from_str(&raw).context("destination settings")?;
    let settings = Settings::parse(kind, &value).map_err(anyhow::Error::msg)?;
    let secrets: Option<Vec<u8>> = row.try_get("secrets")?;
    let fields: String = row.try_get("secret_fields")?;
    let last_at: Option<String> = row.try_get("last_at")?;
    let last = match last_at {
        Some(at) => Some(LastResult {
            at,
            ok: row.try_get::<Option<i64>, _>("last_ok")?.unwrap_or(0) != 0,
            file: row.try_get::<Option<String>, _>("last_file")?.unwrap_or_default(),
            bytes: row.try_get::<Option<i64>, _>("last_bytes")?.unwrap_or(0),
            duration_ms: row.try_get::<Option<i64>, _>("last_duration_ms")?.unwrap_or(0),
            error: row.try_get("last_error")?,
        }),
        None => None,
    };
    Ok(Destination {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        keep: row.try_get::<i64, _>("keep")?.clamp(1, i64::from(super::MAX_KEEP)) as u32,
        include_key: row.try_get::<i64, _>("include_key")? != 0,
        settings,
        has_secret: secrets.is_some_and(|sealed| !sealed.is_empty()),
        saved_secrets: fields.split(',').filter(|f| !f.is_empty()).map(str::to_string).collect(),
        last,
        last_success_at: row.try_get("last_success_at")?,
    })
}

/// Toutes les destinations, par nom. Une ligne illisible est sautée et
/// journalisée : elle ne doit pas priver les autres de leur copie.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Destination>> {
    let sql = format!("SELECT {COLUMNS} FROM backup_destinations ORDER BY name COLLATE NOCASE");
    let rows = sqlx::query(&sql)
        .fetch_all(pool)
        .await
        .context("lecture des destinations de sauvegarde")?;
    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        match from_row(row) {
            Ok(destination) => out.push(destination),
            Err(error) => tracing::warn!(%error, "unreadable backup destination skipped"),
        }
    }
    Ok(out)
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Destination>> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM backup_destinations WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("lecture d'une destination de sauvegarde")?;
    row.as_ref().map(from_row).transpose()
}

/// Identifiant d'une destination de ce nom (casse ignorée), s'il y en a une.
pub async fn id_by_name(pool: &SqlitePool, name: &str) -> Result<Option<i64>> {
    sqlx::query_scalar("SELECT id FROM backup_destinations WHERE name = ? COLLATE NOCASE")
        .bind(name)
        .fetch_optional(pool)
        .await
        .context("recherche d'une destination de sauvegarde")
}

pub async fn count_enabled(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM backup_destinations WHERE enabled = 1")
        .fetch_one(pool)
        .await
        .context("décompte des destinations de sauvegarde")
}

/// Secrets chiffrés et liste de leurs champs ; rien quand il n'y en a pas.
pub fn seal(cipher: &Cipher, secrets: &Secrets) -> Result<(Option<Vec<u8>>, String)> {
    if secrets.is_empty() {
        return Ok((None, String::new()));
    }
    let sealed = cipher.encrypt(&serde_json::to_vec(secrets)?)?;
    let fields = secrets.keys().cloned().collect::<Vec<_>>().join(",");
    Ok((Some(sealed), fields))
}

pub fn unseal(cipher: &Cipher, sealed: Option<&[u8]>) -> Result<Secrets> {
    match sealed {
        None | Some([]) => Ok(Secrets::new()),
        Some(sealed) => {
            let plain = cipher.decrypt(sealed).context("destination secrets")?;
            serde_json::from_slice(&plain).context("destination secrets")
        }
    }
}

pub async fn insert(
    pool: &SqlitePool,
    cipher: &Cipher,
    input: &Input<'_>,
    secrets: &Secrets,
) -> Result<i64> {
    let (sealed, fields) = seal(cipher, secrets)?;
    let id = sqlx::query(
        "INSERT INTO backup_destinations
             (name, kind, enabled, keep, include_key, settings, secrets, secret_fields)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.name)
    .bind(input.settings.kind().as_str())
    .bind(i64::from(input.enabled))
    .bind(i64::from(input.keep))
    .bind(i64::from(input.include_key))
    .bind(input.settings.to_value().to_string())
    .bind(sealed)
    .bind(fields)
    .execute(pool)
    .await
    .context("création d'une destination de sauvegarde")?
    .last_insert_rowid();
    Ok(id)
}

/// Modifie une destination ; `secrets` absent garde ceux qui sont enregistrés.
pub async fn update(
    pool: &SqlitePool,
    cipher: &Cipher,
    id: i64,
    input: &Input<'_>,
    secrets: Option<&Secrets>,
) -> Result<()> {
    sqlx::query(
        "UPDATE backup_destinations
         SET name = ?, kind = ?, enabled = ?, keep = ?, include_key = ?, settings = ?,
             updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
         WHERE id = ?",
    )
    .bind(input.name)
    .bind(input.settings.kind().as_str())
    .bind(i64::from(input.enabled))
    .bind(i64::from(input.keep))
    .bind(i64::from(input.include_key))
    .bind(input.settings.to_value().to_string())
    .bind(id)
    .execute(pool)
    .await
    .context("modification d'une destination de sauvegarde")?;
    if let Some(secrets) = secrets {
        let (sealed, fields) = seal(cipher, secrets)?;
        sqlx::query("UPDATE backup_destinations SET secrets = ?, secret_fields = ? WHERE id = ?")
            .bind(sealed)
            .bind(fields)
            .bind(id)
            .execute(pool)
            .await
            .context("secrets d'une destination de sauvegarde")?;
    }
    Ok(())
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let done = sqlx::query("DELETE FROM backup_destinations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("suppression d'une destination de sauvegarde")?;
    Ok(done.rows_affected() > 0)
}

/// Secrets déchiffrés d'une destination.
pub async fn secrets(pool: &SqlitePool, cipher: &Cipher, id: i64) -> Result<Secrets> {
    let sealed: Option<Option<Vec<u8>>> =
        sqlx::query_scalar("SELECT secrets FROM backup_destinations WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .context("lecture des secrets d'une destination")?;
    unseal(cipher, sealed.flatten().as_deref())
}

/// Consigne le résultat d'un envoi ; un succès date aussi `last_success_at`.
pub async fn record_result(pool: &SqlitePool, id: i64, result: &LastResult) -> Result<()> {
    sqlx::query(
        "UPDATE backup_destinations
         SET last_at = ?, last_ok = ?, last_file = ?, last_bytes = ?, last_duration_ms = ?,
             last_error = ?,
             last_success_at = CASE WHEN ? = 1 THEN ? ELSE last_success_at END
         WHERE id = ?",
    )
    .bind(&result.at)
    .bind(i64::from(result.ok))
    .bind(&result.file)
    .bind(result.bytes)
    .bind(result.duration_ms)
    .bind(&result.error)
    .bind(i64::from(result.ok))
    .bind(&result.at)
    .bind(id)
    .execute(pool)
    .await
    .context("consignation d'un envoi hors site")?;
    Ok(())
}

/// Nom et horodatage Unix du dernier succès de chaque destination qui en a
/// un, pour `/metrics`.
pub async fn last_successes(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, last_success_at FROM backup_destinations
         WHERE last_success_at IS NOT NULL ORDER BY name COLLATE NOCASE",
    )
    .fetch_all(pool)
    .await
    .context("lecture des envois hors site")?;
    Ok(rows
        .into_iter()
        .filter_map(|(name, at)| {
            chrono::NaiveDateTime::parse_from_str(&at, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|at| (name, at.and_utc().timestamp()))
        })
        .collect())
}
