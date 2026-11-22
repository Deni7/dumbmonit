//! Sections `monitor_templates` et `target_channels` du lot : les modèles de
//! moniteur (migration 0283) et les canaux propres à chaque moniteur
//! (migration 0282).
//!
//! Les identifiants numériques ne voyagent pas : un modèle se retrouve par son
//! nom, un moniteur par sa référence (type + adresse), un canal par son nom.
//! Un canal absent de l'instance restaurée est laissé de côté et signalé.

use std::collections::{BTreeMap, HashMap};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::backup::import::{RestoreOutcome, SectionReport};

/// Un modèle ; ses canaux par leur nom.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleMonitorTemplate {
    pub name: String,
    pub kind: String,
    pub interval_secs: u64,
    #[serde(default)]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<String>,
}

/// Les canaux d'un moniteur (voir [`crate::backup::bundle::target_ref`]).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleTargetChannels {
    pub target: String,
    pub channels: Vec<String>,
}

fn names_of(ids: &[i64], channel_names: &HashMap<i64, String>) -> Vec<String> {
    let mut names: Vec<String> =
        ids.iter().filter_map(|id| channel_names.get(id).cloned()).collect();
    names.sort();
    names
}

/// Les modèles, par nom.
pub async fn collect_templates(
    pool: &SqlitePool,
    channel_names: &HashMap<i64, String>,
) -> Result<Vec<BundleMonitorTemplate>> {
    Ok(crate::db::monitor_templates::list(pool)
        .await?
        .into_iter()
        .map(|template| BundleMonitorTemplate {
            channels: names_of(&template.channels, channel_names),
            name: template.name,
            kind: template.kind,
            interval_secs: template.interval_secs,
            tags: template.tags,
        })
        .collect())
}

/// Les canaux de chaque moniteur qui en a, par référence.
pub async fn collect_target_channels(
    pool: &SqlitePool,
    refs: &HashMap<i64, String>,
    channel_names: &HashMap<i64, String>,
) -> Result<Vec<BundleTargetChannels>> {
    let mut out: Vec<BundleTargetChannels> = crate::db::target_channels::all(pool)
        .await?
        .into_iter()
        .filter_map(|(id, channels)| {
            let target = refs.get(&id)?.clone();
            let channels = names_of(&channels, channel_names);
            (!channels.is_empty()).then_some(BundleTargetChannels { target, channels })
        })
        .collect();
    out.sort_by(|a, b| a.target.cmp(&b.target));
    Ok(out)
}

/// Les canaux nommés qui existent ; les autres sont signalés dans `report`.
fn channel_ids(
    names: &[String],
    channels: &HashMap<String, i64>,
    owner: &str,
    report: &mut SectionReport,
) -> Vec<i64> {
    let mut ids = Vec::new();
    for name in names {
        match channels.get(name) {
            Some(id) => ids.push(*id),
            None => report
                .notes
                .push(format!("{owner}: notification channel \"{name}\" not found, left out")),
        }
    }
    crate::db::target_channels::normalise(&ids)
}

async fn template_channels(tx: &mut Transaction<'_, Sqlite>, id: i64) -> Result<Vec<i64>> {
    sqlx::query_scalar::<_, i64>(
        "SELECT channel_id FROM monitor_template_channels WHERE template_id = ?
         ORDER BY channel_id",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .context("relecture des canaux d'un modèle")
}

/// Restaure les modèles : créés s'ils manquent, mis à jour d'après le lot
/// s'ils diffèrent, laissés tels quels sinon.
pub async fn restore_templates(
    tx: &mut Transaction<'_, Sqlite>,
    templates: &[BundleMonitorTemplate],
    channels: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("monitor_templates");
    for template in templates {
        if template.name.trim().is_empty() || template.kind.trim().is_empty() {
            report.skipped += 1;
            report.notes.push("a template without a name or a type was left out".into());
            continue;
        }
        let owner = format!("template \"{}\"", template.name);
        let wanted = channel_ids(&template.channels, channels, &owner, &mut report);
        let tags = serde_json::to_string(&template.tags)?;
        let interval = template.interval_secs.max(10) as i64;
        let existing: Option<(i64, String, i64, String)> = sqlx::query_as(
            "SELECT id, kind, interval_secs, tags FROM monitor_templates WHERE name = ?",
        )
        .bind(&template.name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche d'un modèle")?;
        let (id, outcome) = match existing {
            Some((id, kind, secs, saved_tags)) => {
                let same = kind == template.kind
                    && secs == interval
                    && serde_json::from_str::<BTreeMap<String, String>>(&saved_tags)
                        .is_ok_and(|saved| saved == template.tags)
                    && template_channels(tx, id).await? == wanted;
                if same {
                    report.count(RestoreOutcome::Skipped);
                    continue;
                }
                sqlx::query(
                    "UPDATE monitor_templates SET kind = ?, interval_secs = ?, tags = ?,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%S', 'now')
                     WHERE id = ?",
                )
                .bind(&template.kind)
                .bind(interval)
                .bind(&tags)
                .bind(id)
                .execute(&mut **tx)
                .await
                .context("mise à jour d'un modèle")?;
                (id, RestoreOutcome::Updated)
            }
            None => {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO monitor_templates (name, kind, interval_secs, tags)
                     VALUES (?, ?, ?, ?) RETURNING id",
                )
                .bind(&template.name)
                .bind(&template.kind)
                .bind(interval)
                .bind(&tags)
                .fetch_one(&mut **tx)
                .await
                .context("création d'un modèle")?;
                (id, RestoreOutcome::Created)
            }
        };
        sqlx::query("DELETE FROM monitor_template_channels WHERE template_id = ?")
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("effacement des canaux d'un modèle")?;
        for channel in &wanted {
            sqlx::query(
                "INSERT INTO monitor_template_channels (template_id, channel_id) VALUES (?, ?)",
            )
            .bind(id)
            .bind(channel)
            .execute(&mut **tx)
            .await
            .context("rattachement d'un canal à un modèle")?;
        }
        report.count(outcome);
    }
    Ok(report)
}

/// Restaure les canaux propres aux moniteurs du lot. Un moniteur absent du
/// lot garde les siens.
pub async fn restore_target_channels(
    tx: &mut Transaction<'_, Sqlite>,
    entries: &[BundleTargetChannels],
    refs: &HashMap<String, i64>,
    channels: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("target_channels");
    for entry in entries {
        let Some(&target) = refs.get(&entry.target) else {
            report.skipped += 1;
            report
                .notes
                .push(format!("monitor {} not found, its channels were left out", entry.target));
            continue;
        };
        let owner = format!("monitor {}", entry.target);
        let wanted = channel_ids(&entry.channels, channels, &owner, &mut report);
        let saved: Vec<i64> = sqlx::query_scalar(
            "SELECT channel_id FROM target_channels WHERE target_id = ? ORDER BY channel_id",
        )
        .bind(target)
        .fetch_all(&mut **tx)
        .await
        .context("relecture des canaux d'un moniteur")?;
        if saved == wanted {
            report.count(RestoreOutcome::Skipped);
            continue;
        }
        crate::db::target_channels::set_in(tx, target, &wanted).await?;
        let outcome =
            if saved.is_empty() { RestoreOutcome::Created } else { RestoreOutcome::Updated };
        report.count(outcome);
    }
    Ok(report)
}
