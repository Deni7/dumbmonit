//! Persistance des SLO (`slos`, migration 0260).

use anyhow::{Context, Result};
use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use super::{MonitorRef, Slo, SloInput};
use crate::reports::layout::Scope;

const SELECT: &str = "SELECT id, name, description, objective, window_days, scope, enabled, \
                      created_at, updated_at FROM slos";

fn from_row(row: &SqliteRow) -> Result<Slo> {
    let scope: String = row.try_get("scope")?;
    Ok(Slo {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        objective: row.try_get("objective")?,
        window_days: row.try_get::<i64, _>("window_days")?.clamp(0, i64::from(u32::MAX)) as u32,
        scope: Scope::from_json(&scope),
        enabled: row.try_get::<i64, _>("enabled")? != 0,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Slo>> {
    let rows = sqlx::query(sqlx::AssertSqlSafe(format!("{SELECT} ORDER BY name, id")))
        .fetch_all(pool)
        .await
        .context("lecture des SLO")?;
    rows.iter().map(from_row).collect()
}

pub async fn get(pool: &SqlitePool, id: i64) -> Result<Option<Slo>> {
    let row = sqlx::query(sqlx::AssertSqlSafe(format!("{SELECT} WHERE id = ?")))
        .bind(id)
        .fetch_optional(pool)
        .await
        .context("lecture d'un SLO")?;
    row.as_ref().map(from_row).transpose()
}

pub async fn count(pool: &SqlitePool) -> Result<i64> {
    sqlx::query_scalar("SELECT COUNT(*) FROM slos")
        .fetch_one(pool)
        .await
        .context("décompte des SLO")
}

/// Vrai si un autre SLO que `except` porte déjà ce nom.
pub async fn name_taken(pool: &SqlitePool, name: &str, except: Option<i64>) -> Result<bool> {
    let found: Option<i64> =
        sqlx::query_scalar("SELECT id FROM slos WHERE name = ? AND id IS NOT ?")
            .bind(name)
            .bind(except)
            .fetch_optional(pool)
            .await
            .context("recherche du nom d'un SLO")?;
    Ok(found.is_some())
}

pub async fn insert(pool: &SqlitePool, input: &SloInput) -> Result<i64> {
    let id = sqlx::query_scalar(
        "INSERT INTO slos (name, description, objective, window_days, scope, enabled)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(&input.name)
    .bind(&input.description)
    .bind(input.objective)
    .bind(i64::from(input.window_days))
    .bind(serde_json::to_string(&input.scope)?)
    .bind(input.enabled)
    .fetch_one(pool)
    .await
    .context("création d'un SLO")?;
    Ok(id)
}

pub async fn update(pool: &SqlitePool, id: i64, input: &SloInput) -> Result<bool> {
    let done = sqlx::query(
        "UPDATE slos SET name = ?, description = ?, objective = ?, window_days = ?, scope = ?,
             enabled = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&input.name)
    .bind(&input.description)
    .bind(input.objective)
    .bind(i64::from(input.window_days))
    .bind(serde_json::to_string(&input.scope)?)
    .bind(input.enabled)
    .bind(id)
    .execute(pool)
    .await
    .context("mise à jour d'un SLO")?;
    Ok(done.rows_affected() > 0)
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let done = sqlx::query("DELETE FROM slos WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .context("suppression d'un SLO")?;
    Ok(done.rows_affected() > 0)
}

/// Les moniteurs, réduits à ce qu'il faut pour les rattacher et les mesurer :
/// pas d'identifiants à déchiffrer.
pub async fn monitors(pool: &SqlitePool) -> Result<Vec<MonitorRef>> {
    let rows = sqlx::query("SELECT id, kind, group_name, enabled FROM targets ORDER BY id")
        .fetch_all(pool)
        .await
        .context("lecture des moniteurs d'un SLO")?;
    rows.iter()
        .map(|row| {
            Ok(MonitorRef {
                id: row.try_get("id")?,
                kind: row.try_get("kind")?,
                group: row.try_get("group_name")?,
                enabled: row.try_get::<i64, _>("enabled")? != 0,
            })
        })
        .collect()
}
