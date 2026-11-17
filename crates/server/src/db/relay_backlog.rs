//! Curseur de rattrapage des relais (voir `crate::collectors::relay`) : jusqu'où
//! le serveur a reçu le tampon disque de chaque relais.
//!
//! Un état technique, pas un réglage : il n'est pas sauvegardé. Après une
//! restauration, un lot renvoyé serait rangé une seconde fois avec les mêmes
//! horodatages, ce que la déduplication de VictoriaMetrics absorbe.

use anyhow::{Context, Result};
use dumbmonit_proto::TargetId;
use sqlx::{Row, SqlitePool};

/// Ce que le serveur sait du tampon d'un relais.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BacklogCursor {
    pub spool_id: String,
    pub last_seq: u64,
    pub dropped: u64,
}

pub async fn cursor(pool: &SqlitePool, agent_id: TargetId) -> Result<Option<BacklogCursor>> {
    let row = sqlx::query(
        "SELECT spool_id, last_seq, dropped FROM relay_backlog_cursors WHERE agent_id = ?",
    )
    .bind(agent_id)
    .fetch_optional(pool)
    .await
    .context("lecture du curseur de rattrapage du relais")?;
    row.map(|row| {
        Ok(BacklogCursor {
            spool_id: row.try_get("spool_id")?,
            last_seq: row.try_get::<i64, _>("last_seq")?.max(0) as u64,
            dropped: row.try_get::<i64, _>("dropped")?.max(0) as u64,
        })
    })
    .transpose()
}

pub async fn save(pool: &SqlitePool, agent_id: TargetId, cursor: &BacklogCursor) -> Result<()> {
    sqlx::query(
        "INSERT INTO relay_backlog_cursors (agent_id, spool_id, last_seq, dropped, updated_at)
         VALUES (?, ?, ?, ?, datetime('now'))
         ON CONFLICT(agent_id) DO UPDATE SET
            spool_id = excluded.spool_id,
            last_seq = excluded.last_seq,
            dropped = excluded.dropped,
            updated_at = excluded.updated_at",
    )
    .bind(agent_id)
    .bind(&cursor.spool_id)
    .bind(i64::try_from(cursor.last_seq).unwrap_or(i64::MAX))
    .bind(i64::try_from(cursor.dropped).unwrap_or(i64::MAX))
    .execute(pool)
    .await
    .context("écriture du curseur de rattrapage du relais")?;
    Ok(())
}
