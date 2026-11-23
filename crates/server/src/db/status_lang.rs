//! Langues d'une page de statut : langue par défaut et traductions des textes
//! de l'administrateur (titre, description, pied de page, noms de services).

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

/// Les langues de l'interface, les seules acceptées (un test du front vérifie
/// que la liste suit `project.inlang/settings.json`).
pub const SUPPORTED: [&str; 14] =
    ["en", "de", "es", "fr", "it", "pt", "pt-BR", "ru", "zh-Hans", "ja", "ko", "nl", "pl", "tr"];

/// Les textes d'une page dans une langue ; un champ vide garde le texte écrit.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Translation {
    pub lang: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub footer_text: String,
    /// Libellé écrit (nom d'un service ou d'un groupe) -> libellé traduit.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

/// Langue par défaut de la page, `""` pour l'anglais.
pub async fn default_lang(pool: &SqlitePool, page_id: i64) -> Result<String> {
    Ok(sqlx::query_scalar("SELECT default_lang FROM status_pages WHERE id = ?")
        .bind(page_id)
        .fetch_optional(pool)
        .await
        .context("lecture de la langue d'une page de statut")?
        .unwrap_or_default())
}

pub async fn list(pool: &SqlitePool, page_id: i64) -> Result<Vec<Translation>> {
    let rows = sqlx::query(
        "SELECT lang, title, description, footer_text, labels FROM status_page_translations
         WHERE page_id = ? ORDER BY lang",
    )
    .bind(page_id)
    .fetch_all(pool)
    .await
    .context("lecture des traductions d'une page de statut")?;
    rows.iter()
        .map(|row| {
            let labels: String = row.try_get("labels")?;
            Ok(Translation {
                lang: row.try_get("lang")?,
                title: row.try_get("title")?,
                description: row.try_get("description")?,
                footer_text: row.try_get("footer_text")?,
                labels: serde_json::from_str(&labels).unwrap_or_default(),
            })
        })
        .collect()
}

/// Une traduction, ou `None`.
pub async fn get(pool: &SqlitePool, page_id: i64, lang: &str) -> Result<Option<Translation>> {
    Ok(list(pool, page_id).await?.into_iter().find(|t| t.lang == lang))
}

/// Enregistre une traduction (création ou remplacement). Renvoie vrai si la
/// ligne a changé.
pub async fn upsert<'e, E>(executor: E, page_id: i64, t: &Translation) -> Result<bool>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let labels = serde_json::to_string(&t.labels)?;
    let done = sqlx::query(
        "INSERT INTO status_page_translations (page_id, lang, title, description, footer_text, labels)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(page_id, lang) DO UPDATE SET
             title = excluded.title, description = excluded.description,
             footer_text = excluded.footer_text, labels = excluded.labels
         WHERE title IS NOT excluded.title OR description IS NOT excluded.description
            OR footer_text IS NOT excluded.footer_text OR labels IS NOT excluded.labels",
    )
    .bind(page_id)
    .bind(&t.lang)
    .bind(&t.title)
    .bind(&t.description)
    .bind(&t.footer_text)
    .bind(labels)
    .execute(executor)
    .await
    .context("enregistrement d'une traduction de page de statut")?;
    Ok(done.rows_affected() > 0)
}

/// Remplace tout : langue par défaut et ensemble des traductions.
pub async fn replace(
    pool: &SqlitePool,
    page_id: i64,
    default_lang: &str,
    translations: &[Translation],
) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE status_pages SET default_lang = ? WHERE id = ?")
        .bind(default_lang)
        .bind(page_id)
        .execute(&mut *tx)
        .await
        .context("langue par défaut de la page de statut")?;
    sqlx::query("DELETE FROM status_page_translations WHERE page_id = ?")
        .bind(page_id)
        .execute(&mut *tx)
        .await?;
    for translation in translations {
        upsert(&mut *tx, page_id, translation).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Règle les langues d'une page restaurée depuis une sauvegarde. Renvoie vrai
/// si quelque chose a changé.
pub async fn restore<'e, E>(executor: E, page_id: i64, default_lang: &str) -> Result<bool>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    let done =
        sqlx::query("UPDATE status_pages SET default_lang = ? WHERE id = ? AND default_lang != ?")
            .bind(default_lang)
            .bind(page_id)
            .bind(default_lang)
            .execute(executor)
            .await
            .context("langue par défaut de la page de statut")?;
    Ok(done.rows_affected() > 0)
}
