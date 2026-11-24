//! Section `annotations` du lot : les repères de changement (migration 0288).
//!
//! Un périmètre désigne ses moniteurs par référence (`type|adresse`), comme les
//! silences et les SLO : les identifiants numériques d'une instance ne veulent
//! rien dire dans une autre. Un repère est un fait daté : s'il existe déjà (même
//! instant, même titre, même auteur), il est laissé tel quel.

use std::collections::HashMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::annotations::{Annotation, Scope, store};
use crate::backup::import::SectionReport;

/// Un repère dans le lot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BundleAnnotation {
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub scope: BundleScope,
    pub source: String,
    #[serde(default)]
    pub author: String,
}

/// Périmètre d'un repère, moniteurs désignés par référence.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BundleScope {
    Global,
    Monitors { targets: Vec<String> },
    Folder { folder: String },
    Tags { tags: Vec<String> },
}

/// Les repères, un moniteur supprimé depuis sortant du périmètre ; un
/// périmètre « moniteurs » qui n'en désigne plus aucun est écarté.
pub async fn collect(
    pool: &SqlitePool,
    refs: &HashMap<i64, String>,
) -> Result<Vec<BundleAnnotation>> {
    let all = store::list(pool, None, None, crate::annotations::MAX_ANNOTATIONS).await?;
    Ok(all.into_iter().filter_map(|item| carried(item, refs)).collect())
}

fn carried(item: Annotation, refs: &HashMap<i64, String>) -> Option<BundleAnnotation> {
    let scope = match item.scope {
        Scope::Global => BundleScope::Global,
        Scope::Monitors { targets } => {
            let named: Vec<String> =
                targets.iter().filter_map(|id| refs.get(id).cloned()).collect();
            if named.is_empty() {
                return None;
            }
            BundleScope::Monitors { targets: named }
        }
        Scope::Folder { folder } => BundleScope::Folder { folder },
        Scope::Tags { tags } => BundleScope::Tags { tags },
    };
    Some(BundleAnnotation {
        at: item.at,
        until: item.until,
        title: item.title,
        text: item.text,
        link: item.link,
        tags: item.tags,
        scope,
        source: item.source,
        author: item.author,
    })
}

/// Restaure les repères dans la transaction de l'appelant.
pub(crate) async fn restore(
    tx: &mut Transaction<'_, Sqlite>,
    items: &[BundleAnnotation],
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("annotations");
    for item in items {
        let scope = match &item.scope {
            BundleScope::Global => Scope::Global,
            BundleScope::Folder { folder } => Scope::Folder { folder: folder.clone() },
            BundleScope::Tags { tags } => Scope::Tags { tags: tags.clone() },
            BundleScope::Monitors { targets } => {
                let ids: Vec<i64> = targets.iter().filter_map(|r| refs.get(r).copied()).collect();
                if ids.is_empty() {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "Marker \"{}\": none of its monitors is in this backup; it was skipped.",
                        item.title
                    ));
                    continue;
                }
                Scope::Monitors { targets: ids }
            }
        };
        if !crate::annotations::SOURCES.contains(&item.source.as_str()) {
            report.skipped += 1;
            report.notes.push(format!("Marker \"{}\": unknown source, skipped.", item.title));
            continue;
        }
        let known: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM annotations WHERE at = ? AND title = ? AND author = ?",
        )
        .bind(&item.at)
        .bind(&item.title)
        .bind(&item.author)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du repère")?;
        if known.is_some() {
            report.skipped += 1;
            continue;
        }
        sqlx::query(
            "INSERT INTO annotations (at, until, title, text, link, tags, scope, source, author)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&item.at)
        .bind(&item.until)
        .bind(&item.title)
        .bind(&item.text)
        .bind(&item.link)
        .bind(serde_json::to_string(&item.tags)?)
        .bind(serde_json::to_string(&scope)?)
        .bind(&item.source)
        .bind(&item.author)
        .execute(&mut **tx)
        .await
        .context("création du repère")?;
        report.created += 1;
    }
    Ok(report)
}
