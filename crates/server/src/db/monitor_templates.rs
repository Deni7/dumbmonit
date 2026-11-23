//! Modèles de moniteur (migration 0283) : type, options et étiquettes sans
//! secret, période et canaux, de quoi préremplir le formulaire d'ajout.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MonitorTemplate {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub interval_secs: u64,
    /// Options du type et étiquettes libres ; jamais un secret.
    pub tags: BTreeMap<String, String>,
    /// Canaux de notification du moniteur créé ; vide : ceux de ses règles.
    pub channels: Vec<i64>,
    /// UTC, sans suffixe.
    pub created_at: String,
    pub updated_at: String,
}

/// Ce qu'un modèle enregistre.
#[derive(Debug, Clone)]
pub struct TemplateInput {
    pub name: String,
    pub kind: String,
    pub interval_secs: u64,
    pub tags: BTreeMap<String, String>,
    pub channels: Vec<i64>,
}

fn from_row(row: &SqliteRow, channels: Vec<i64>) -> Result<MonitorTemplate> {
    let id: i64 = row.try_get("id")?;
    let tags: String = row.try_get("tags")?;
    Ok(MonitorTemplate {
        id,
        name: row.try_get("name")?,
        kind: row.try_get("kind")?,
        interval_secs: row.try_get::<i64, _>("interval_secs")?.max(0) as u64,
        tags: serde_json::from_str(&tags)
            .with_context(|| format!("options illisibles pour le modèle {id}"))?,
        channels,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

async fn channels_by_template(pool: &SqlitePool) -> Result<HashMap<i64, Vec<i64>>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT template_id, channel_id FROM monitor_template_channels
         ORDER BY template_id, channel_id",
    )
    .fetch_all(pool)
    .await
    .context("lecture des canaux des modèles")?;
    let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
    for (template, channel) in rows {
        out.entry(template).or_default().push(channel);
    }
    Ok(out)
}

/// Tous les modèles, par nom.
pub async fn list(pool: &SqlitePool) -> Result<Vec<MonitorTemplate>> {
    let rows = sqlx::query(
        "SELECT id, name, kind, interval_secs, tags, created_at, updated_at
         FROM monitor_templates ORDER BY name COLLATE NOCASE, id",
    )
    .fetch_all(pool)
    .await
    .context("liste des modèles de moniteur")?;
    let mut channels = channels_by_template(pool).await?;
    rows.iter()
        .map(|row| {
            let id: i64 = row.try_get("id")?;
            from_row(row, channels.remove(&id).unwrap_or_default())
        })
        .collect()
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<MonitorTemplate>> {
    let row = sqlx::query(
        "SELECT id, name, kind, interval_secs, tags, created_at, updated_at
         FROM monitor_templates WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("lecture d'un modèle de moniteur")?;
    let Some(row) = row else { return Ok(None) };
    let channels: Vec<i64> = sqlx::query_scalar(
        "SELECT channel_id FROM monitor_template_channels WHERE template_id = ?
         ORDER BY channel_id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .context("lecture des canaux d'un modèle")?;
    from_row(&row, channels).map(Some)
}

/// Crée un modèle ; un nom déjà pris remonte l'erreur `UNIQUE` de la base.
pub async fn create(pool: &SqlitePool, input: &TemplateInput) -> Result<i64> {
    let mut tx = pool.begin().await.context("ouverture de la transaction du modèle")?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO monitor_templates (name, kind, interval_secs, tags) VALUES (?, ?, ?, ?)
         RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.kind)
    .bind(input.interval_secs as i64)
    .bind(serde_json::to_string(&input.tags)?)
    .fetch_one(&mut *tx)
    .await
    .context("création d'un modèle de moniteur")?;
    for channel in &input.channels {
        sqlx::query(
            "INSERT OR IGNORE INTO monitor_template_channels (template_id, channel_id)
             VALUES (?, ?)",
        )
        .bind(id)
        .bind(channel)
        .execute(&mut *tx)
        .await
        .context("rattachement d'un canal à un modèle")?;
    }
    tx.commit().await.context("validation du modèle")?;
    Ok(id)
}

/// Renomme un modèle. Faux s'il n'existe pas.
pub async fn rename(pool: &SqlitePool, id: i64, name: &str) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE monitor_templates SET name = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%S', 'now')
         WHERE id = ?",
    )
    .bind(name)
    .bind(id)
    .execute(pool)
    .await
    .context("renommage d'un modèle de moniteur")?;
    Ok(done.rows_affected() > 0)
}

/// Supprime un modèle (ses canaux partent avec lui). Faux s'il n'existait pas.
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let done = sqlx::query("DELETE FROM monitor_templates WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("suppression d'un modèle de moniteur")?;
    Ok(done.rows_affected() > 0)
}
