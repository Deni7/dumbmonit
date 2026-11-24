//! Sentinelles d'agent, moniteurs push et sites.

use super::*;

/// Sentinelles : un agent absent du lot (ou de l'instance) est signalé, un réglage
/// identique à celui en place n'est pas réécrit.
pub(super) async fn restore_agent_sentinels(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    use crate::collectors::agent::sentinel::check_channel;

    let mut report = SectionReport::new("agent_sentinels");
    for entry in &bundle.agent_sentinels {
        let Some(target_id) = refs.get(&entry.agent).copied() else {
            report.skipped += 1;
            report
                .notes
                .push(format!("Sentinel: its agent ({}) is not in this backup.", entry.agent));
            continue;
        };
        if let Some(channel) = &entry.channel
            && let Err(why) = check_channel(channel)
        {
            report.skipped += 1;
            report.notes.push(format!("Sentinel of {}: {why}", entry.agent));
            continue;
        }
        let urls: Vec<&String> = entry
            .urls
            .iter()
            .filter(|url| dumbmonit_proto::https_authority(url).is_some())
            .take(dumbmonit_proto::SENTINEL_MAX_URLS)
            .collect();
        let interval = entry.interval_secs.clamp(
            dumbmonit_proto::SENTINEL_MIN_INTERVAL_SECS,
            dumbmonit_proto::SENTINEL_MAX_INTERVAL_SECS,
        );
        let threshold = entry.threshold.clamp(1, dumbmonit_proto::SENTINEL_MAX_THRESHOLD);
        // Sans adresse ni canal, une sentinelle ne peut pas être active.
        let enabled = entry.enabled && !urls.is_empty() && entry.channel.is_some();
        let pins: std::collections::BTreeMap<&String, String> = entry
            .pins
            .iter()
            .filter(|(url, _)| urls.contains(url))
            .filter_map(|(url, raw)| Some((url, dumbmonit_proto::normalize_pin(raw)?)))
            .collect();
        let urls_json = serde_json::to_string(&urls)?;
        let pins_json = serde_json::to_string(&pins)?;

        let existing = sqlx::query(
            "SELECT enabled, urls, pins, interval_secs, threshold, channel_enc \
             FROM agent_sentinel WHERE target_id = ?",
        )
        .bind(target_id)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de la sentinelle")?;

        if let Some(row) = &existing {
            let stored_channel: Option<Vec<u8>> = row.try_get("channel_enc")?;
            let same_channel = match (&entry.channel, stored_channel) {
                (None, None) => true,
                (Some(wanted), Some(bytes)) => cipher
                    .decrypt(&bytes)
                    .ok()
                    .and_then(|plain| serde_json::from_slice(&plain).ok())
                    .is_some_and(|stored: dumbmonit_proto::SentinelChannel| &stored == wanted),
                _ => false,
            };
            if same_channel
                && (row.try_get::<i64, _>("enabled")? != 0) == enabled
                && row.try_get::<String, _>("urls")? == urls_json
                && row.try_get::<String, _>("pins")? == pins_json
                && row.try_get::<i64, _>("interval_secs")? == interval as i64
                && row.try_get::<i64, _>("threshold")? == i64::from(threshold)
            {
                report.skipped += 1;
                continue;
            }
        }

        let encrypted = match &entry.channel {
            Some(channel) => Some(cipher.encrypt(&serde_json::to_vec(channel)?)?),
            None => None,
        };
        let outcome = sqlx::query(
            "INSERT INTO agent_sentinel \
                 (target_id, enabled, urls, pins, interval_secs, threshold, channel_enc) \
             VALUES (?, ?, ?, ?, ?, ?, ?) \
             ON CONFLICT(target_id) DO UPDATE SET enabled = excluded.enabled, \
                 urls = excluded.urls, pins = excluded.pins, \
                 interval_secs = excluded.interval_secs, \
                 threshold = excluded.threshold, channel_enc = excluded.channel_enc, \
                 updated_at = datetime('now')",
        )
        .bind(target_id)
        .bind(i64::from(enabled))
        .bind(&urls_json)
        .bind(&pins_json)
        .bind(interval as i64)
        .bind(i64::from(threshold))
        .bind(encrypted)
        .execute(&mut **tx)
        .await;
        match outcome {
            Ok(_) => report.count(if existing.is_some() {
                RestoreOutcome::Updated
            } else {
                RestoreOutcome::Created
            }),
            Err(error) => report.refused(&entry.agent, &anyhow::Error::from(error)),
        }
    }
    Ok(report)
}

pub(super) async fn restore_push_monitors(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("push_monitors");
    for monitor in &bundle.push_monitors {
        let Some(target_id) = refs.get(&monitor.target).copied() else {
            report.skipped += 1;
            report.notes.push(format!(
                "Heartbeat token: its monitor ({}) is not in this backup.",
                monitor.target
            ));
            continue;
        };
        let fingerprint = crate::collectors::push::token::fingerprint(&monitor.token);
        let existing = sqlx::query("SELECT token_hash FROM push_monitors WHERE target_id = ?")
            .bind(target_id)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche du moniteur en poussée")?;

        if let Some(row) = &existing
            && row.try_get::<String, _>("token_hash")? == fingerprint
        {
            report.skipped += 1;
            continue;
        }

        let encrypted = cipher.encrypt(monitor.token.as_bytes())?;
        let outcome = match &existing {
            Some(_) => sqlx::query(
                "UPDATE push_monitors SET token_hash = ?, token_enc = ? WHERE target_id = ?",
            )
            .bind(&fingerprint)
            .bind(&encrypted)
            .bind(target_id)
            .execute(&mut **tx)
            .await
            .map(|_| RestoreOutcome::Updated),
            None => sqlx::query(
                "INSERT INTO push_monitors (target_id, token_hash, token_enc) VALUES (?, ?, ?)",
            )
            .bind(target_id)
            .bind(&fingerprint)
            .bind(&encrypted)
            .execute(&mut **tx)
            .await
            .map(|_| RestoreOutcome::Created),
        };
        match outcome {
            Ok(outcome) => report.count(outcome),
            Err(error) => {
                report.refused(&format!("Heartbeat token of {}", monitor.target), &error.into())
            }
        }
    }
    Ok(report)
}

/// Paquets d'intégration : revérifiés par ce serveur-ci, qui peut être d'une
/// autre version que celui qui a écrit le lot. Un paquet identique (même
/// empreinte) est laissé tel quel ; un paquet différent remplace l'installé,
/// son état activé ou non compris. Les règles viennent de la section `rules`.
/// Emplacements, par nom (sans casse) : identique, laissé ; différent, mis à
/// jour. Puis les affectations des équipements du lot, qui ne comptent pas
/// comme des changements de la section : elles suivent les équipements.
pub(super) async fn restore_locations(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("locations");
    for entry in &bundle.locations {
        let name = entry.name.trim();
        let valid = !name.is_empty()
            && entry.latitude.is_finite()
            && (-90.0..=90.0).contains(&entry.latitude)
            && entry.longitude.is_finite()
            && (-180.0..=180.0).contains(&entry.longitude);
        if !valid {
            report.skipped += 1;
            report.notes.push(format!("location \"{name}\" has no valid name or coordinates"));
            continue;
        }
        // Un lot sans code (ancienne version) reçoit celui que le nom donne par défaut.
        let code = match crate::api::locations::default_code(&entry.code) {
            code if code.is_empty() => crate::api::locations::default_code(name),
            code => code,
        };
        let existing = sqlx::query(
            "SELECT id, address, latitude, longitude, site, code FROM locations
             WHERE name = ? COLLATE NOCASE",
        )
        .bind(name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de l'emplacement")?;
        if let Some(row) = &existing
            && row.try_get::<String, _>("address")? == entry.address
            && row.try_get::<f64, _>("latitude")? == entry.latitude
            && row.try_get::<f64, _>("longitude")? == entry.longitude
            && row.try_get::<String, _>("site")? == entry.site
            && row.try_get::<String, _>("code")? == code
        {
            report.skipped += 1;
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO locations (name, address, latitude, longitude, site, code)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(name) DO UPDATE SET address = excluded.address,
                 latitude = excluded.latitude, longitude = excluded.longitude,
                 site = excluded.site, code = excluded.code",
        )
        .bind(name)
        .bind(&entry.address)
        .bind(entry.latitude)
        .bind(entry.longitude)
        .bind(&entry.site)
        .bind(&code)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) if existing.is_some() => report.count(RestoreOutcome::Updated),
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused(&format!("location \"{name}\""), &error.into()),
        }
    }
    for target in &bundle.targets {
        let Some(location) = target.location.as_deref() else { continue };
        let Some(id) = refs.get(&target_ref(&target.kind, &target.address)) else { continue };
        sqlx::query(
            "UPDATE targets SET location_id =
                 (SELECT id FROM locations WHERE name = ? COLLATE NOCASE)
             WHERE id = ?",
        )
        .bind(location)
        .bind(id)
        .execute(&mut **tx)
        .await
        .context("emplacement d'un équipement")?;
    }
    Ok(report)
}
