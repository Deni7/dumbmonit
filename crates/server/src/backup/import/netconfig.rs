//! Configurations réseau et balayages planifiés.

use super::*;

/// Clés d'hôte mémorisées : identique, laissée ; différente, remplacée par
/// celle du lot (c'est elle que l'opérateur a validée).
pub(super) async fn restore_netconfig_host_keys(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("netconfig_host_keys");
    for entry in &bundle.netconfig_host_keys {
        let Some(target_id) = refs.get(&entry.target).copied() else {
            report.skipped += 1;
            report.notes.push(format!(
                "SSH host key: its device ({}) is not in this backup.",
                entry.target
            ));
            continue;
        };
        let existing: Option<Option<String>> =
            sqlx::query_scalar("SELECT host_key FROM netconfig_state WHERE target_id = ?")
                .bind(target_id)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche de la clé d'hôte")?;
        if existing.as_ref().and_then(Option::as_deref) == Some(entry.host_key.as_str()) {
            report.skipped += 1;
            continue;
        }
        let outcome = sqlx::query(
            "INSERT INTO netconfig_state (target_id, host_key, host_key_pinned_at)
             VALUES (?, ?, datetime('now'))
             ON CONFLICT (target_id) DO UPDATE
             SET host_key = excluded.host_key, host_key_pinned_at = excluded.host_key_pinned_at,
                 host_key_seen = NULL, host_key_seen_at = NULL",
        )
        .bind(target_id)
        .bind(&entry.host_key)
        .execute(&mut **tx)
        .await
        .map(|_| match existing {
            Some(_) => RestoreOutcome::Updated,
            None => RestoreOutcome::Created,
        });
        match outcome {
            Ok(outcome) => report.count(outcome),
            Err(error) => {
                report.refused(&format!("SSH host key of {}", entry.target), &error.into())
            }
        }
    }
    Ok(report)
}

/// Scans planifiés et appareils connus ou ignorés. Un scan déjà présent (même
/// sous-réseau, même scanner) garde ses réglages ; le lot n'y ajoute que les
/// décisions sur les appareils. Ne retire jamais rien. L'inventaire est marqué
/// initialisé dès qu'il reçoit des appareils : le prochain balayage ne doit pas
/// refaire tout le réseau « nouveau ».
pub(super) async fn restore_netscan_schedules(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    use crate::netinventory::{MAX_INTERVAL_SECS, MIN_INTERVAL_SECS, canonical_cidr};

    let mut report = SectionReport::new("netscan_schedules");
    for entry in &bundle.netscan_schedules {
        let Ok(cidr) = canonical_cidr(&entry.cidr) else {
            report.skipped += 1;
            report.notes.push(format!("scheduled scan \"{}\" has an invalid network", entry.cidr));
            continue;
        };
        let agent_id = match &entry.agent {
            Some(reference) => match refs.get(reference).copied() {
                Some(id) => Some(id),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "scheduled scan {cidr}: its relay agent ({reference}) is not in the backup."
                    ));
                    continue;
                }
            },
            None => None,
        };

        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM netscan_schedules
             WHERE cidr = ?1 AND IFNULL(agent_id, 0) = IFNULL(?2, 0)",
        )
        .bind(&cidr)
        .bind(agent_id)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du scan planifié")?;
        let (schedule_id, mut changed) = match existing {
            Some(id) => (id, false),
            None => {
                let depth = if entry.depth == "extended" { "extended" } else { "standard" };
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO netscan_schedules
                         (name, cidr, agent_id, interval_secs, depth, enabled)
                     VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
                )
                .bind(entry.name.chars().take(80).collect::<String>())
                .bind(&cidr)
                .bind(agent_id)
                .bind(entry.interval_secs.clamp(MIN_INTERVAL_SECS, MAX_INTERVAL_SECS))
                .bind(depth)
                .bind(entry.enabled)
                .fetch_one(&mut **tx)
                .await
                .context("création du scan planifié")?;
                (id, true)
            }
        };

        for device in &entry.devices {
            let row: Option<(i64, i64, i64)> = sqlx::query_as(
                "SELECT id, known, ignored FROM netscan_devices
                 WHERE schedule_id = ? AND ident = ?",
            )
            .bind(schedule_id)
            .bind(&device.ident)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche de l'appareil")?;
            match row {
                Some((_, 1, ignored)) if (ignored != 0) == device.ignored => {}
                Some((id, _, _)) => {
                    sqlx::query(
                        "UPDATE netscan_devices SET known = 1, ignored = ?, mac_change_from = NULL
                         WHERE id = ?",
                    )
                    .bind(device.ignored)
                    .bind(id)
                    .execute(&mut **tx)
                    .await
                    .context("mise à jour de l'appareil")?;
                    sqlx::query("UPDATE netscan_ports SET known = 1 WHERE device_id = ?")
                        .bind(id)
                        .execute(&mut **tx)
                        .await
                        .context("ports de l'appareil")?;
                    changed = true;
                }
                None => {
                    sqlx::query(
                        "INSERT INTO netscan_devices
                             (schedule_id, ident, mac, ip, hostname, vendor, known, ignored,
                              first_seen, last_seen)
                         VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
                    )
                    .bind(schedule_id)
                    .bind(&device.ident)
                    .bind(&device.mac)
                    .bind(&device.ip)
                    .bind(&device.hostname)
                    .bind(&device.vendor)
                    .bind(device.ignored)
                    .bind(&device.first_seen)
                    .bind(&device.last_seen)
                    .execute(&mut **tx)
                    .await
                    .context("création de l'appareil")?;
                    changed = true;
                }
            }
        }
        if !entry.devices.is_empty() {
            sqlx::query("UPDATE netscan_schedules SET initialized = 1 WHERE id = ?")
                .bind(schedule_id)
                .execute(&mut **tx)
                .await
                .context("initialisation de l'inventaire")?;
        }
        report.count(match (existing.is_some(), changed) {
            (false, _) => RestoreOutcome::Created,
            (true, true) => RestoreOutcome::Updated,
            (true, false) => RestoreOutcome::Skipped,
        });
    }
    Ok(report)
}

/// Versions des configurations réseau : rechiffrées avec la clé de cette
/// instance, empreinte de comparaison recalculée. Une version déjà présente
/// (même équipement, même date de relevé) est laissée.
pub(super) async fn restore_netconfig_versions(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("netconfig_versions");
    for entry in &bundle.netconfig_versions {
        let Some(target_id) = refs.get(&entry.target).copied() else {
            report.skipped += 1;
            continue;
        };
        let present: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM netconfig_versions WHERE target_id = ? AND fetched_at = ?",
        )
        .bind(target_id)
        .bind(&entry.fetched_at)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche d'une version de configuration")?;
        let too_large = entry.content.len() > dumbmonit_collectors::netconfig::MAX_CONFIG_BYTES;
        if present.is_some() || too_large {
            report.skipped += 1;
            continue;
        }
        let sealed = crate::netconfig::seal(cipher, &entry.content)?;
        let digest = crate::netconfig::digest_of(cipher, &entry.profile, &entry.content);
        let outcome = sqlx::query(
            "INSERT INTO netconfig_versions
                 (target_id, fetched_at, last_seen_at, profile, digest, size_bytes, lines,
                  added, removed, content_enc)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(target_id)
        .bind(&entry.fetched_at)
        .bind(&entry.last_seen_at)
        .bind(&entry.profile)
        .bind(&digest)
        .bind(entry.content.len() as i64)
        .bind(entry.content.lines().count() as i64)
        .bind(entry.added)
        .bind(entry.removed)
        .bind(&sealed)
        .execute(&mut **tx)
        .await
        .map(|_| RestoreOutcome::Created);
        match outcome {
            Ok(outcome) => report.count(outcome),
            Err(error) => report.refused(
                &format!("Configuration version of {} ({})", entry.target, entry.fetched_at),
                &error.into(),
            ),
        }
    }
    Ok(report)
}
