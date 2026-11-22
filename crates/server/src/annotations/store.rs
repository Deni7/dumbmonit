//! Table `annotations` (migration 0288).

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqlitePool};

use super::{Annotation, Draft, RETENTION_DAYS, Scope, TargetInfo};
use crate::db;

const COLUMNS: &str = "id, at, until, title, text, link, tags, scope, source, author, created_at";

fn read(row: &SqliteRow) -> Result<Annotation> {
    let tags: String = row.try_get("tags")?;
    let scope: String = row.try_get("scope")?;
    Ok(Annotation {
        id: row.try_get("id")?,
        at: row.try_get("at")?,
        until: row.try_get("until")?,
        title: row.try_get("title")?,
        text: row.try_get("text")?,
        link: row.try_get("link")?,
        tags: serde_json::from_str(&tags).unwrap_or_default(),
        // Un périmètre illisible ne doit pas élargir la portée : il devient
        // une liste vide de moniteurs, qui ne concerne personne.
        scope: serde_json::from_str(&scope).unwrap_or(Scope::Monitors { targets: Vec::new() }),
        source: row.try_get("source")?,
        author: row.try_get("author")?,
        created_at: row.try_get("created_at")?,
    })
}

pub async fn count(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM annotations")
        .fetch_one(pool)
        .await
        .context("comptage des repères")
}

pub async fn insert(pool: &SqlitePool, draft: &Draft, author: &str) -> Result<i64> {
    sqlx::query_scalar(
        "INSERT INTO annotations (at, until, title, text, link, tags, scope, source, author)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(db::timestamp(draft.at))
    .bind(draft.until.map(db::timestamp))
    .bind(&draft.title)
    .bind(&draft.text)
    .bind(&draft.link)
    .bind(serde_json::to_string(&draft.tags)?)
    .bind(serde_json::to_string(&draft.scope)?)
    .bind(&draft.source)
    .bind(author)
    .fetch_one(pool)
    .await
    .context("création d'un repère")
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Annotation>> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM annotations WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("lecture d'un repère")?;
    row.as_ref().map(read).transpose()
}

/// Les repères qui touchent la fenêtre `[from, to]` (un instant ou une fenêtre
/// qui la recoupe), du plus ancien au plus récent ; les `limit` plus récents
/// quand il y en a davantage. Bornes absentes : ouvertes.
pub async fn list(
    pool: &SqlitePool,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<Annotation>> {
    let rows = sqlx::query(&format!(
        "SELECT {COLUMNS} FROM annotations
         WHERE (?1 IS NULL OR COALESCE(until, at) >= ?1) AND (?2 IS NULL OR at <= ?2)
         ORDER BY at DESC, id DESC LIMIT ?3"
    ))
    .bind(from.map(db::timestamp))
    .bind(to.map(db::timestamp))
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("lecture des repères")?;
    let mut found = rows.iter().map(read).collect::<Result<Vec<_>>>()?;
    found.reverse();
    Ok(found)
}

pub async fn update(pool: &SqlitePool, id: i64, draft: &Draft) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE annotations SET at = ?, until = ?, title = ?, text = ?, link = ?, tags = ?,
             scope = ?, source = ? WHERE id = ?",
    )
    .bind(db::timestamp(draft.at))
    .bind(draft.until.map(db::timestamp))
    .bind(&draft.title)
    .bind(&draft.text)
    .bind(&draft.link)
    .bind(serde_json::to_string(&draft.tags)?)
    .bind(serde_json::to_string(&draft.scope)?)
    .bind(&draft.source)
    .bind(id)
    .execute(pool)
    .await
    .context("modification d'un repère")?;
    Ok(done.rows_affected() > 0)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let done = sqlx::query("DELETE FROM annotations WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("suppression d'un repère")?;
    Ok(done.rows_affected() > 0)
}

/// Supprime les repères terminés depuis plus de [`RETENTION_DAYS`] jours.
pub async fn purge(pool: &SqlitePool, now: DateTime<Utc>) -> Result<u64> {
    let cutoff = db::timestamp(now - TimeDelta::days(RETENTION_DAYS));
    let done = sqlx::query("DELETE FROM annotations WHERE COALESCE(until, at) < ?")
        .bind(cutoff)
        .execute(pool)
        .await
        .context("purge des repères")?;
    Ok(done.rows_affected())
}

/// Dossier et étiquettes de chaque moniteur, pour résoudre les périmètres.
pub async fn target_infos(pool: &SqlitePool) -> Result<Vec<TargetInfo>> {
    let rows = sqlx::query("SELECT id, group_name, tags FROM targets")
        .fetch_all(pool)
        .await
        .context("lecture des moniteurs pour les repères")?;
    rows.iter()
        .map(|row| {
            let tags: String = row.try_get("tags")?;
            let parsed: serde_json::Value = serde_json::from_str(&tags).unwrap_or_default();
            let tags = parsed
                .as_object()
                .map(|map| {
                    map.iter()
                        .map(|(key, value)| {
                            (key.clone(), value.as_str().unwrap_or_default().to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(TargetInfo { id: row.try_get("id")?, folder: row.try_get("group_name")?, tags })
        })
        .collect()
}
