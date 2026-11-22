//! Canaux de notification.

use super::*;


pub(super) async fn restore_channels(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("channels");
    for channel in &bundle.channels {
        match upsert_channel(tx, cipher, channel).await {
            Ok(outcome) => report.count(outcome),
            Err(error) => report.refused(&format!("Channel \"{}\"", channel.name), &error),
        }
    }
    Ok(report)
}

async fn upsert_channel(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    channel: &BundleChannel,
) -> Result<RestoreOutcome> {
    let settings = serde_json::to_string(&channel.settings)?;
    let policy = serde_json::to_string(&channel.policy)?;
    let secrets = match &channel.secrets {
        None => None,
        Some(value) => Some(cipher.encrypt(serde_json::to_vec(value)?.as_slice())?),
    };

    let existing = sqlx::query(
        "SELECT id, kind, enabled, settings, secret_enc, policy
                     FROM notification_channels WHERE name = ?",
    )
    .bind(&channel.name)
    .fetch_optional(&mut **tx)
    .await
    .context("recherche du canal")?;

    let Some(row) = existing else {
        sqlx::query(
            "INSERT INTO notification_channels (name, kind, enabled, settings, secret_enc, policy)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&channel.name)
        .bind(&channel.kind)
        .bind(i64::from(channel.enabled))
        .bind(&settings)
        .bind(secrets)
        .bind(&policy)
        .execute(&mut **tx)
        .await
        .context("création du canal")?;
        return Ok(RestoreOutcome::Created);
    };

    let id: i64 = row.try_get("id")?;
    let stored_settings: String = row.try_get("settings")?;
    let stored_policy: String = row.try_get("policy")?;
    let stored_secret: Option<Vec<u8>> = row.try_get("secret_enc")?;
    let same = row.try_get::<String, _>("kind")? == channel.kind
        && (row.try_get::<i64, _>("enabled")? != 0) == channel.enabled
        && serde_json::from_str::<Value>(&stored_settings).ok() == Some(channel.settings.clone())
        && serde_json::from_str::<Value>(&stored_policy).ok() == Some(channel.policy.clone())
        && same_secret(cipher, stored_secret.as_deref(), channel.secrets.as_ref());

    if same {
        return Ok(RestoreOutcome::Skipped);
    }

    sqlx::query(
        "UPDATE notification_channels
         SET kind = ?, enabled = ?, settings = ?, secret_enc = COALESCE(?, secret_enc),
             policy = ?, updated_at = datetime('now')
         WHERE id = ?",
    )
    .bind(&channel.kind)
    .bind(i64::from(channel.enabled))
    .bind(&settings)
    .bind(secrets)
    .bind(&policy)
    .bind(id)
    .execute(&mut **tx)
    .await
    .context("mise à jour du canal")?;
    Ok(RestoreOutcome::Updated)
}
