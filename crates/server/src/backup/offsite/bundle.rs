//! Section `backup_destinations` du lot : les destinations hors site, leurs
//! réglages et leurs secrets.
//!
//! Les secrets partent en clair dans la charge utile, que la phrase de passe
//! chiffre tout entière, comme ceux des canaux ; ils sont rechiffrés par la
//! clé de l'instance qui restaure. L'empreinte SFTP confirmée voyage avec les
//! réglages : l'instance restaurée ne fait pas confiance à une clé neuve. Les
//! résultats d'envoi, eux, sont des constats et ne voyagent pas.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

use super::{Kind, MAX_KEEP, Secrets, Settings, clean_secrets, store};
use crate::backup::import::{RestoreOutcome, SectionReport};
use crate::crypto::Cipher;

pub const SECTION: &str = "backup_destinations";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleBackupDestination {
    pub name: String,
    pub kind: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    pub keep: u32,
    #[serde(default)]
    pub include_key: bool,
    pub settings: Value,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secrets: Secrets,
}

fn enabled_by_default() -> bool {
    true
}

/// Les destinations, secrets déchiffrés. Des secrets illisibles partent
/// vides, signalés dans le journal : la destination reste décrite.
pub async fn collect(pool: &SqlitePool, cipher: &Cipher) -> Result<Vec<BundleBackupDestination>> {
    let mut out = Vec::new();
    for destination in store::list(pool).await? {
        let secrets = match store::secrets(pool, cipher, destination.id).await {
            Ok(secrets) => secrets,
            Err(error) => {
                tracing::warn!(
                    %error,
                    name = %destination.name,
                    "unreadable destination secrets, exported empty"
                );
                Secrets::new()
            }
        };
        out.push(BundleBackupDestination {
            name: destination.name.clone(),
            kind: destination.settings.kind().as_str().to_string(),
            enabled: destination.enabled,
            keep: destination.keep,
            include_key: destination.include_key,
            settings: destination.settings.to_value(),
            secrets,
        });
    }
    Ok(out)
}

/// Une ligne du lot, contrôlée comme le ferait l'API.
fn checked(saved: &BundleBackupDestination) -> Result<(Settings, Secrets), String> {
    let name = saved.name.trim();
    if name.is_empty() || name.chars().count() > super::MAX_NAME_LEN {
        return Err("its name is empty or too long".into());
    }
    let kind = Kind::parse(&saved.kind).ok_or_else(|| format!("unknown kind {}", saved.kind))?;
    let settings = Settings::parse(kind, &saved.settings)?;
    settings.validate()?;
    let secrets = clean_secrets(saved.secrets.clone());
    settings.check_secrets(&secrets)?;
    Ok((settings, secrets))
}

/// Crée ou met à jour chaque destination par son nom ; rien n'est supprimé.
pub async fn restore(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    list: &[BundleBackupDestination],
) -> Result<SectionReport> {
    let mut report = SectionReport::new(SECTION);
    for saved in list {
        let (settings, secrets) = match checked(saved) {
            Ok(checked) => checked,
            Err(reason) => {
                report.skipped += 1;
                let note = format!("Destination \"{}\" was left out: {reason}", saved.name);
                report.notes.push(note);
                continue;
            }
        };
        let name = saved.name.trim();
        let what = format!("Destination \"{name}\"");
        let keep = i64::from(saved.keep.clamp(1, MAX_KEEP));
        let raw = settings.to_value().to_string();
        let existing: Option<(i64, String, i64, i64, i64, String, Option<Vec<u8>>)> =
            sqlx::query_as(
                "SELECT id, kind, enabled, keep, include_key, settings, secrets
                 FROM backup_destinations WHERE name = ? COLLATE NOCASE",
            )
            .bind(name)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche d'une destination de sauvegarde")?;

        if let Some((id, kind, enabled, stored_keep, include_key, stored, sealed)) = &existing {
            let stored_secrets = store::unseal(cipher, sealed.as_deref()).ok();
            let same = kind == settings.kind().as_str()
                && (*enabled != 0) == saved.enabled
                && *stored_keep == keep
                && (*include_key != 0) == saved.include_key
                && *stored == raw
                && stored_secrets.as_ref() == Some(&secrets);
            if same {
                report.count(RestoreOutcome::Skipped);
                continue;
            }
            let (sealed, fields) = store::seal(cipher, &secrets)?;
            let result = sqlx::query(
                "UPDATE backup_destinations
                 SET kind = ?, enabled = ?, keep = ?, include_key = ?, settings = ?, secrets = ?,
                     secret_fields = ?, updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')
                 WHERE id = ?",
            )
            .bind(settings.kind().as_str())
            .bind(i64::from(saved.enabled))
            .bind(keep)
            .bind(i64::from(saved.include_key))
            .bind(&raw)
            .bind(sealed)
            .bind(fields)
            .bind(*id)
            .execute(&mut **tx)
            .await;
            match result {
                Ok(_) => report.count(RestoreOutcome::Updated),
                Err(error) => report.refused(&what, &anyhow::Error::from(error)),
            }
            continue;
        }

        let (sealed, fields) = store::seal(cipher, &secrets)?;
        let result = sqlx::query(
            "INSERT INTO backup_destinations
                 (name, kind, enabled, keep, include_key, settings, secrets, secret_fields)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(name)
        .bind(settings.kind().as_str())
        .bind(i64::from(saved.enabled))
        .bind(keep)
        .bind(i64::from(saved.include_key))
        .bind(&raw)
        .bind(sealed)
        .bind(fields)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused(&what, &anyhow::Error::from(error)),
        }
    }
    Ok(report)
}
