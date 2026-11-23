//! L'historique des alertes vu comme une suite d'épisodes, pour le rapport de
//! bruit ([`crate::alerting::noise`]).
//!
//! Un épisode est une ligne `to_phase = 'firing'` d'`alert_history`. Deux
//! colonnes le complètent (migration 0284) : `acked_at`, posée au premier
//! acquittement ou écartement de l'épisode en cours, et `notifications`,
//! incrémentée à chaque notification que le moteur prend en charge pour lui —
//! déclenchement, rappels, escalades, résolution. Les lignes plus anciennes
//! n'ont que le drapeau `notified`, qui sert alors de décompte.

use std::collections::HashSet;

use anyhow::{Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use sqlx::{Row, SqlitePool};

use crate::alerting::model::TargetId;
use crate::alerting::noise::Episode;

/// Même format que les horodatages d'`alert_history` (voir `db::alerts`).
fn to_sql(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn from_sql(raw: Option<String>) -> Option<DateTime<Utc>> {
    raw.and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .map(|value| value.with_timezone(&Utc))
}

/// Un décompte lu en base, borné à ce qu'un `u32` tient.
fn count(raw: i64) -> u32 {
    u32::try_from(raw.max(0)).unwrap_or(u32::MAX)
}

/// Restriction aux équipements visibles, à coller dans une clause `WHERE`.
/// Les identifiants sont des entiers lus en base, jamais une saisie.
fn scope_clause(visible: Option<&HashSet<TargetId>>) -> String {
    match visible {
        None => String::new(),
        Some(visible) if visible.is_empty() => " AND target_id IN (-1)".to_string(),
        Some(visible) => {
            let mut ids: Vec<TargetId> = visible.iter().copied().collect();
            ids.sort_unstable();
            let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            format!(" AND target_id IN ({ids})")
        }
    }
}

/// Date le premier acquittement (ou écartement) de l'épisode en cours d'une
/// empreinte. Sans effet si la dernière transition de l'empreinte n'est pas un
/// déclenchement (alerte encore en attente) ou si l'épisode a déjà été
/// acquitté : c'est la première réaction qui compte.
pub async fn stamp_acknowledged(
    pool: &SqlitePool,
    fingerprint: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    sqlx::query(
        "UPDATE alert_history SET acked_at = ?
         WHERE id = (SELECT id FROM alert_history WHERE fingerprint = ?
                     ORDER BY at DESC, id DESC LIMIT 1)
           AND to_phase = 'firing' AND acked_at IS NULL",
    )
    .bind(to_sql(now))
    .bind(fingerprint)
    .execute(pool)
    .await
    .context("date de l'acquittement dans l'historique")?;
    Ok(())
}

/// [`stamp_acknowledged`] au mieux : un échec ne coûte qu'un point du délai
/// moyen d'acquittement, il ne doit pas faire échouer le geste.
pub async fn note_acknowledged(pool: &SqlitePool, fingerprint: &str) {
    if let Err(error) = stamp_acknowledged(pool, fingerprint, Utc::now()).await {
        tracing::warn!(?error, fingerprint, "acquittement non daté dans l'historique");
    }
}

/// Compte une notification pour l'épisode en cours de chaque empreinte : le
/// dernier déclenchement de l'empreinte dans l'historique.
pub async fn count_notifications(pool: &SqlitePool, fingerprints: &HashSet<String>) -> Result<()> {
    if fingerprints.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await?;
    for fingerprint in fingerprints {
        sqlx::query(
            "UPDATE alert_history SET notifications = notifications + 1
             WHERE id = (SELECT id FROM alert_history
                         WHERE fingerprint = ? AND to_phase = 'firing'
                         ORDER BY at DESC, id DESC LIMIT 1)",
        )
        .bind(fingerprint)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await.context("décompte des notifications par épisode")
}

/// Les épisodes ouverts depuis `since`, les plus récents d'abord, au plus
/// `limit`. Le booléen dit si la limite a coupé la liste.
pub async fn load_episodes(
    pool: &SqlitePool,
    since: DateTime<Utc>,
    visible: Option<&HashSet<TargetId>>,
    limit: usize,
) -> Result<(Vec<Episode>, bool)> {
    let scope = scope_clause(visible).replace("target_id", "f.target_id");
    let sql = format!(
        "SELECT f.rule_uid, f.target_id, f.at, f.acked_at,
                CASE WHEN f.notifications > 0 THEN f.notifications ELSE f.notified END
                    AS notifications,
                (SELECT MIN(r.at) FROM alert_history r
                  WHERE r.fingerprint = f.fingerprint AND r.to_phase = 'resolved'
                    AND r.at >= f.at) AS resolved_at
         FROM alert_history f
         WHERE f.to_phase = 'firing' AND f.at >= ?{scope}
         ORDER BY f.at DESC, f.id DESC
         LIMIT ?"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(to_sql(since))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX).saturating_add(1))
        .fetch_all(pool)
        .await
        .context("lecture des épisodes d'alerte")?;
    let truncated = rows.len() > limit;
    let mut episodes = Vec::with_capacity(rows.len().min(limit));
    for row in rows.iter().take(limit) {
        let Some(fired_at) = from_sql(row.try_get("at")?) else { continue };
        episodes.push(Episode {
            rule_uid: row.try_get("rule_uid")?,
            target_id: row.try_get("target_id")?,
            fired_at,
            acked_at: from_sql(row.try_get("acked_at")?),
            resolved_at: from_sql(row.try_get("resolved_at")?),
            notifications: count(row.try_get("notifications")?),
        });
    }
    Ok((episodes, truncated))
}

/// Une alerte qui bat : déclenchée puis résolue plusieurs fois dans la fenêtre.
#[derive(Debug, Clone, PartialEq)]
pub struct Flapping {
    pub fingerprint: String,
    pub rule_uid: String,
    pub target_id: Option<TargetId>,
    /// Transitions vers `firing` ou `resolved`.
    pub changes: u32,
    pub firings: u32,
}

/// Les empreintes qui ont déclenché au moins `min_firings` fois depuis
/// `since`, celles qui ont le plus changé d'état d'abord.
pub async fn load_flapping(
    pool: &SqlitePool,
    since: DateTime<Utc>,
    visible: Option<&HashSet<TargetId>>,
    min_firings: u32,
    limit: usize,
) -> Result<Vec<Flapping>> {
    let scope = scope_clause(visible);
    let sql = format!(
        "SELECT fingerprint, MAX(rule_uid) AS rule_uid, MAX(target_id) AS target_id,
                COUNT(*) AS changes,
                SUM(CASE WHEN to_phase = 'firing' THEN 1 ELSE 0 END) AS firings
         FROM alert_history
         WHERE at >= ? AND to_phase IN ('firing', 'resolved'){scope}
         GROUP BY fingerprint
         HAVING SUM(CASE WHEN to_phase = 'firing' THEN 1 ELSE 0 END) >= ?
         ORDER BY changes DESC, MAX(at) DESC
         LIMIT ?"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(to_sql(since))
        .bind(i64::from(min_firings))
        .bind(i64::try_from(limit).unwrap_or(i64::MAX))
        .fetch_all(pool)
        .await
        .context("lecture des alertes qui battent")?;
    rows.iter()
        .map(|row| {
            Ok(Flapping {
                fingerprint: row.try_get("fingerprint")?,
                rule_uid: row.try_get("rule_uid")?,
                target_id: row.try_get("target_id")?,
                changes: count(row.try_get("changes")?),
                firings: count(row.try_get("firings")?),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + seconds, 0).expect("valid timestamp")
    }

    async fn pool() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::open(&dir.path().join("t.db")).await.unwrap();
        (dir, pool)
    }

    async fn row(pool: &SqlitePool, fp: &str, target: i64, to: &str, notified: bool, t: i64) {
        sqlx::query(
            "INSERT INTO alert_history
                 (fingerprint, rule_uid, target_id, from_phase, to_phase, severity, notified, at)
             VALUES (?, 'disk', ?, 'ok', ?, 'warning', ?, ?)",
        )
        .bind(fp)
        .bind(target)
        .bind(to)
        .bind(i64::from(notified))
        .bind(to_sql(at(t)))
        .execute(pool)
        .await
        .expect("history row");
    }

    #[tokio::test]
    async fn un_episode_porte_son_acquittement_sa_resolution_et_ses_notifications() {
        let (_dir, pool) = pool().await;
        row(&pool, "disk@1", 1, "firing", true, 0).await;
        let fingerprints: HashSet<String> = ["disk@1".to_string()].into_iter().collect();
        count_notifications(&pool, &fingerprints).await.unwrap();
        count_notifications(&pool, &fingerprints).await.unwrap();
        stamp_acknowledged(&pool, "disk@1", at(90)).await.unwrap();
        // Le second acquittement ne déplace pas le premier.
        stamp_acknowledged(&pool, "disk@1", at(300)).await.unwrap();
        row(&pool, "disk@1", 1, "resolved", true, 600).await;
        count_notifications(&pool, &fingerprints).await.unwrap();

        let (episodes, truncated) = load_episodes(&pool, at(-60), None, 100).await.unwrap();
        assert!(!truncated);
        assert_eq!(episodes.len(), 1);
        let episode = &episodes[0];
        assert_eq!(episode.fired_at, at(0));
        assert_eq!(episode.acked_at, Some(at(90)));
        assert_eq!(episode.resolved_at, Some(at(600)));
        assert_eq!(episode.notifications, 3, "firing, a reminder and the resolution");

        // Résolue : un acquittement tardif ne touche plus l'épisode.
        stamp_acknowledged(&pool, "disk@1", at(700)).await.unwrap();
        let (episodes, _) = load_episodes(&pool, at(-60), None, 100).await.unwrap();
        assert_eq!(episodes[0].acked_at, Some(at(90)));
    }

    #[tokio::test]
    async fn une_ligne_ancienne_compte_son_drapeau_et_la_portee_filtre() {
        let (_dir, pool) = pool().await;
        row(&pool, "disk@1", 1, "firing", true, 0).await;
        row(&pool, "disk@2", 2, "firing", false, 10).await;

        let (episodes, _) = load_episodes(&pool, at(-60), None, 100).await.unwrap();
        assert_eq!(episodes.iter().map(|e| e.notifications).sum::<u32>(), 1);

        let only_two: HashSet<TargetId> = [2].into_iter().collect();
        let (scoped, _) = load_episodes(&pool, at(-60), Some(&only_two), 100).await.unwrap();
        assert_eq!(scoped.len(), 1);
        assert_eq!(scoped[0].target_id, Some(2));
        let (none, _) = load_episodes(&pool, at(-60), Some(&HashSet::new()), 100).await.unwrap();
        assert!(none.is_empty());

        let (cut, truncated) = load_episodes(&pool, at(-60), None, 1).await.unwrap();
        assert_eq!(cut.len(), 1);
        assert!(truncated);
    }

    #[tokio::test]
    async fn une_alerte_qui_bat_est_reperee() {
        let (_dir, pool) = pool().await;
        for cycle in 0..3 {
            row(&pool, "disk@1", 1, "firing", true, cycle * 100).await;
            row(&pool, "disk@1", 1, "resolved", true, cycle * 100 + 50).await;
        }
        row(&pool, "disk@2", 2, "firing", true, 0).await;

        let flapping = load_flapping(&pool, at(-60), None, 2, 10).await.unwrap();
        assert_eq!(flapping.len(), 1);
        assert_eq!(flapping[0].fingerprint, "disk@1");
        assert_eq!(flapping[0].changes, 6);
        assert_eq!(flapping[0].firings, 3);
    }
}
