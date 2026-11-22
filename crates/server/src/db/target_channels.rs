//! Canaux de notification propres à un moniteur (migration 0282).
//!
//! Sans ligne ici, un moniteur prévient comme avant : les canaux de la règle,
//! à défaut tous. Avec des lignes, une alerte dont la règle ne nomme aucun
//! canal ne part que vers ceux-là ; une règle qui nomme ses canaux garde le
//! dernier mot (c'est elle qui sait qu'un disque plein va à l'équipe stockage).

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use dumbmonit_proto::TargetId;
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::alerting::group::AlertGroup;

/// Les canaux de chaque moniteur qui en a, triés.
pub async fn all(pool: &SqlitePool) -> Result<HashMap<TargetId, Vec<i64>>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT target_id, channel_id FROM target_channels ORDER BY target_id, channel_id",
    )
    .fetch_all(pool)
    .await
    .context("lecture des canaux des moniteurs")?;
    let mut out: HashMap<TargetId, Vec<i64>> = HashMap::new();
    for (target, channel) in rows {
        out.entry(target).or_default().push(channel);
    }
    Ok(out)
}

/// Les canaux d'un moniteur, triés ; vide : ceux de ses règles.
pub async fn of(pool: &SqlitePool, id: TargetId) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(
        "SELECT channel_id FROM target_channels WHERE target_id = ? ORDER BY channel_id",
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .context("lecture des canaux d'un moniteur")
}

/// Remplace les canaux d'un moniteur (vide : il n'en a plus de propres).
pub async fn set(pool: &SqlitePool, id: TargetId, channels: &[i64]) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction des canaux")?;
    set_in(&mut tx, id, channels).await?;
    tx.commit().await.context("validation des canaux d'un moniteur")?;
    Ok(())
}

/// [`set`] dans la transaction de l'appelant.
pub async fn set_in(
    tx: &mut Transaction<'_, Sqlite>,
    id: TargetId,
    channels: &[i64],
) -> Result<()> {
    sqlx::query("DELETE FROM target_channels WHERE target_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .context("effacement des canaux d'un moniteur")?;
    for channel in channels {
        sqlx::query("INSERT OR IGNORE INTO target_channels (target_id, channel_id) VALUES (?, ?)")
            .bind(id)
            .bind(channel)
            .execute(&mut **tx)
            .await
            .context("rattachement d'un canal à un moniteur")?;
    }
    Ok(())
}

/// Identifiants des canaux qui existent.
pub async fn known_channels(pool: &SqlitePool) -> Result<HashSet<i64>> {
    let ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM notification_channels")
        .fetch_all(pool)
        .await
        .context("lecture des canaux")?;
    Ok(ids.into_iter().collect())
}

/// Liste nettoyée : dédoublonnée et triée.
pub fn normalise(channels: &[i64]) -> Vec<i64> {
    let mut out = channels.to_vec();
    out.sort_unstable();
    out.dedup();
    out
}

/// Restreint aux canaux du moniteur les groupes que leurs règles envoient
/// partout (`channels` vide). Un groupe sans équipement, ou dont une règle
/// nomme ses canaux, reste tel quel.
pub fn narrow(groups: &mut [AlertGroup], assigned: &HashMap<TargetId, Vec<i64>>) {
    for group in groups {
        if !group.channels.is_empty() {
            continue;
        }
        if let Some(channels) = group.target_id.and_then(|id| assigned.get(&id))
            && !channels.is_empty()
        {
            group.channels = channels.clone();
        }
    }
}

/// [`narrow`], la table n'étant lue que si un groupe peut être concerné.
pub async fn narrow_groups(pool: &SqlitePool, groups: &mut [AlertGroup]) -> Result<()> {
    if !groups.iter().any(|g| g.channels.is_empty() && g.target_id.is_some()) {
        return Ok(());
    }
    let assigned = all(pool).await?;
    if !assigned.is_empty() {
        narrow(groups, &assigned);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::Utc;

    use super::*;
    use crate::alerting::model::Severity;

    fn group(target: Option<TargetId>, channels: Vec<i64>) -> AlertGroup {
        AlertGroup {
            target_id: target,
            target_name: String::new(),
            target_kind: String::new(),
            target_tags: BTreeMap::new(),
            severity: Severity::Info,
            items: Vec::new(),
            channels,
            client_id: None,
            at: Utc::now(),
        }
    }

    /// Les canaux du moniteur remplacent « tous » ; ceux qu'une règle nomme
    /// restent, comme le groupe des séries sans équipement.
    #[test]
    fn les_canaux_du_moniteur_ne_remplacent_que_tous_les_canaux() {
        let assigned = HashMap::from([(1, vec![7, 9]), (2, vec![7])]);
        let mut groups = vec![
            group(Some(1), vec![]),
            group(Some(2), vec![3]),
            group(None, vec![]),
            group(Some(5), vec![]),
        ];
        narrow(&mut groups, &assigned);
        assert_eq!(groups[0].channels, vec![7, 9]);
        assert_eq!(groups[1].channels, vec![3], "the rule's own channels win");
        assert!(groups[2].channels.is_empty());
        assert!(groups[3].channels.is_empty(), "no channel of its own: every channel");
    }

    #[test]
    fn une_liste_est_dedoublonnee_et_triee() {
        assert_eq!(normalise(&[4, 2, 4, 1]), vec![1, 2, 4]);
    }
}
