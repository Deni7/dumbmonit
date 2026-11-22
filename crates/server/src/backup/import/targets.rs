//! Équipements, stratégies de conteneur, multi-sites et paquets de types.

use super::*;


pub(super) async fn target_ids(tx: &mut Transaction<'_, Sqlite>) -> Result<HashMap<String, i64>> {
    let rows = sqlx::query("SELECT id, kind, address FROM targets")
        .fetch_all(&mut **tx)
        .await
        .context("relecture des équipements")?;
    rows.iter()
        .map(|row| {
            let kind: String = row.try_get("kind")?;
            let address: String = row.try_get("address")?;
            Ok((target_ref(&kind, &address), row.try_get("id")?))
        })
        .collect()
}

pub(super) async fn channel_ids(tx: &mut Transaction<'_, Sqlite>) -> Result<HashMap<String, i64>> {
    let rows = sqlx::query("SELECT id, name FROM notification_channels")
        .fetch_all(&mut **tx)
        .await
        .context("relecture des canaux")?;
    rows.iter().map(|row| Ok((row.try_get("name")?, row.try_get("id")?))).collect()
}

/// Normalise un identifiant du lot et vérifie qu'il a bien la forme attendue.
///
/// Stocker un JSON quelconque dans `credential_enc` ferait échouer toute
/// lecture de l'équipement plus tard, à un endroit qui n'expliquerait rien.
fn credential_bytes(value: &Value) -> Result<Vec<u8>> {
    let credential: dumbmonit_proto::Credential =
        serde_json::from_value(value.clone()).context("credential not understood")?;
    Ok(serde_json::to_vec(&credential)?)
}

pub(super) async fn restore_targets(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("targets");
    // Le sort de chaque équipement, tenu de côté : le second passage peut
    // encore faire passer un équipement « inchangé » à « modifié » s'il lui
    // rattache un parent différent.
    let mut outcomes: HashMap<String, RestoreOutcome> = HashMap::new();

    // Premier passage : les équipements eux-mêmes, sans leurs liens. Un parent
    // peut apparaître après son enfant dans le lot.
    for target in &bundle.targets {
        match upsert_target(tx, cipher, target).await {
            Ok(outcome) => {
                outcomes.insert(target_ref(&target.kind, &target.address), outcome);
            }
            Err(error) => report.refused(&format!("Monitor \"{}\"", target.name), &error),
        }
    }

    // Second passage : parents et agents relais, maintenant que tout existe.
    let refs = target_ids(tx).await?;
    for target in &bundle.targets {
        let key = target_ref(&target.kind, &target.address);
        let Some(id) = refs.get(&key) else { continue };
        let parent = resolve(&mut report, &refs, target.parent.as_deref(), &target.name, "parent");
        let relay =
            resolve(&mut report, &refs, target.via_agent.as_deref(), &target.name, "relay agent");
        // Une cible ne peut pas être son propre parent : la chaîne de
        // suppression des alertes tournerait en rond.
        let parent = parent.filter(|p| p != id);
        let relay = relay.filter(|p| p != id);

        if let Some(multisite) = &target.multisite
            && restore_multisite(tx, &mut report, &refs, *id, &target.name, multisite).await?
            && outcomes.get(&key) == Some(&RestoreOutcome::Skipped)
        {
            outcomes.insert(key.clone(), RestoreOutcome::Updated);
        }

        if restore_container_policies(tx, *id, &target.container_policies).await?
            && outcomes.get(&key) == Some(&RestoreOutcome::Skipped)
        {
            outcomes.insert(key.clone(), RestoreOutcome::Updated);
        }

        let current = sqlx::query("SELECT parent_id, via_agent FROM targets WHERE id = ?")
            .bind(id)
            .fetch_one(&mut **tx)
            .await
            .context("lecture des rattachements")?;
        if current.try_get::<Option<i64>, _>("parent_id")? == parent
            && current.try_get::<Option<i64>, _>("via_agent")? == relay
        {
            continue;
        }

        sqlx::query("UPDATE targets SET parent_id = ?, via_agent = ? WHERE id = ?")
            .bind(parent)
            .bind(relay)
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("rattachement des équipements")?;
        if outcomes.get(&key) == Some(&RestoreOutcome::Skipped) {
            outcomes.insert(key, RestoreOutcome::Updated);
        }
    }

    for outcome in outcomes.into_values() {
        report.count(outcome);
    }
    Ok(report)
}

/// Rétablit les politiques automatiques des conteneurs d'une machine : chaque
/// conteneur du lot est écrit, ceux que le lot ne cite pas restent tels quels.
/// Rend `true` si quelque chose a changé.
async fn restore_container_policies(
    tx: &mut Transaction<'_, Sqlite>,
    id: i64,
    policies: &[BundleContainerPolicy],
) -> Result<bool> {
    let mut changed = false;
    for policy in policies {
        let current = sqlx::query(
            "SELECT auto_restart, auto_update, prune_old_image, only_in_maintenance
             FROM container_policies WHERE target_id = ? AND container = ?",
        )
        .bind(id)
        .bind(&policy.container)
        .fetch_optional(&mut **tx)
        .await
        .context("lecture d'une politique de conteneur")?;
        if let Some(row) = &current
            && (row.try_get::<i64, _>("auto_restart")? != 0) == policy.auto_restart
            && (row.try_get::<i64, _>("auto_update")? != 0) == policy.auto_update
            && (row.try_get::<i64, _>("prune_old_image")? != 0) == policy.prune_old_image
            && (row.try_get::<i64, _>("only_in_maintenance")? != 0) == policy.only_in_maintenance
        {
            continue;
        }
        sqlx::query(
            "INSERT INTO container_policies
                 (target_id, container, auto_restart, auto_update, prune_old_image,
                  only_in_maintenance)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (target_id, container) DO UPDATE SET
                 auto_restart = excluded.auto_restart,
                 auto_update = excluded.auto_update,
                 prune_old_image = excluded.prune_old_image,
                 only_in_maintenance = excluded.only_in_maintenance",
        )
        .bind(id)
        .bind(&policy.container)
        .bind(i64::from(policy.auto_restart))
        .bind(i64::from(policy.auto_update))
        .bind(i64::from(policy.prune_old_image))
        .bind(i64::from(policy.only_in_maintenance))
        .execute(&mut **tx)
        .await
        .context("restauration d'une politique de conteneur")?;
        changed = true;
    }
    Ok(changed)
}

/// Rétablit le réglage multi-emplacements d'un moniteur. Un agent absent est
/// laissé de côté (avec une note), un quorum devenu trop grand revient à la
/// majorité. Rend `true` si quelque chose a changé.
async fn restore_multisite(
    tx: &mut Transaction<'_, Sqlite>,
    report: &mut SectionReport,
    refs: &HashMap<String, i64>,
    id: i64,
    owner: &str,
    multisite: &BundleMultisite,
) -> Result<bool> {
    let mut agents: Vec<i64> = multisite
        .agents
        .iter()
        .filter_map(|a| resolve(report, refs, Some(a), owner, "probe location"))
        .filter(|a| *a != id)
        .collect();
    agents.sort_unstable();
    agents.dedup();
    let locations = agents.len() + usize::from(multisite.server);
    let quorum = multisite.quorum.filter(|q| *q as usize <= locations).map(i64::from);

    let current =
        sqlx::query("SELECT include_server, quorum FROM target_multisite WHERE target_id = ?")
            .bind(id)
            .fetch_optional(&mut **tx)
            .await
            .context("lecture de la sonde multi-emplacements")?;
    let current_agents = sqlx::query_scalar::<_, i64>(
        "SELECT agent_id FROM target_multisite_agents WHERE target_id = ? ORDER BY agent_id",
    )
    .bind(id)
    .fetch_all(&mut **tx)
    .await
    .context("lecture des agents de la sonde multi-emplacements")?;
    if let Some(row) = &current
        && (row.try_get::<i64, _>("include_server")? != 0) == multisite.server
        && row.try_get::<Option<i64>, _>("quorum")? == quorum
        && current_agents == agents
    {
        return Ok(false);
    }

    sqlx::query("DELETE FROM target_multisite WHERE target_id = ?")
        .bind(id)
        .execute(&mut **tx)
        .await
        .context("retrait de la sonde multi-emplacements")?;
    sqlx::query(
        "INSERT INTO target_multisite (target_id, include_server, quorum) VALUES (?, ?, ?)",
    )
    .bind(id)
    .bind(i64::from(multisite.server))
    .bind(quorum)
    .execute(&mut **tx)
    .await
    .context("restauration de la sonde multi-emplacements")?;
    for agent in agents {
        sqlx::query("INSERT INTO target_multisite_agents (target_id, agent_id) VALUES (?, ?)")
            .bind(id)
            .bind(agent)
            .execute(&mut **tx)
            .await
            .context("restauration d'un agent de la sonde multi-emplacements")?;
    }
    Ok(true)
}

fn resolve(
    report: &mut SectionReport,
    refs: &HashMap<String, i64>,
    reference: Option<&str>,
    owner: &str,
    role: &str,
) -> Option<i64> {
    let reference = reference?;
    match refs.get(reference) {
        Some(id) => Some(*id),
        None => {
            report.notes.push(format!(
                "Monitor \"{owner}\": its {role} ({reference}) is not in this backup and does not \
                 exist here; the link was left empty."
            ));
            None
        }
    }
}

async fn upsert_target(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    target: &BundleTarget,
) -> Result<RestoreOutcome> {
    let credential = match &target.credential {
        None => None,
        Some(value) => Some(credential_bytes(value)?),
    };
    let tags = serde_json::to_string(&target.tags)?;

    let existing = sqlx::query(
        "SELECT id, name, profile_id, interval_secs, enabled, cve_scan, tags, credential_enc,
                group_name, note, position
         FROM targets WHERE kind = ? AND address = ?",
    )
    .bind(&target.kind)
    .bind(&target.address)
    .fetch_optional(&mut **tx)
    .await
    .context("recherche de l'équipement")?;

    let Some(row) = existing else {
        sqlx::query(
            "INSERT INTO targets (name, address, kind, profile_id, interval_secs, enabled, cve_scan,
                                  tags, credential_enc, group_name, note, position)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&target.name)
        .bind(&target.address)
        .bind(&target.kind)
        .bind(&target.profile_id)
        .bind(target.interval_secs.max(10))
        .bind(i64::from(target.enabled))
        .bind(i64::from(target.cve_scan))
        .bind(&tags)
        .bind(credential.map(|plain| cipher.encrypt(&plain)).transpose()?)
        .bind(&target.group_name)
        .bind(&target.note)
        .bind(target.position)
        .execute(&mut **tx)
        .await
        .context("création de l'équipement")?;
        return Ok(RestoreOutcome::Created);
    };

    let id: i64 = row.try_get("id")?;
    let stored_tags: String = row.try_get("tags")?;
    let stored_credential: Option<Vec<u8>> = row.try_get("credential_enc")?;
    let same = row.try_get::<String, _>("name")? == target.name
        && row.try_get::<Option<String>, _>("profile_id")? == target.profile_id
        && row.try_get::<i64, _>("interval_secs")? == target.interval_secs
        && (row.try_get::<i64, _>("enabled")? != 0) == target.enabled
        && (row.try_get::<i64, _>("cve_scan")? != 0) == target.cve_scan
        && serde_json::from_str::<Value>(&stored_tags).ok()
            == serde_json::from_str::<Value>(&tags).ok()
        && same_secret(cipher, stored_credential.as_deref(), target.credential.as_ref())
        && row.try_get::<String, _>("group_name")? == target.group_name
        && row.try_get::<String, _>("note")? == target.note
        && row.try_get::<i64, _>("position")? == target.position;

    if same {
        return Ok(RestoreOutcome::Skipped);
    }

    sqlx::query(
        "UPDATE targets SET name = ?, profile_id = ?, interval_secs = ?, enabled = ?,
             cve_scan = ?, tags = ?, credential_enc = ?, group_name = ?, note = ?, position = ?,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ?",
    )
    .bind(&target.name)
    .bind(&target.profile_id)
    .bind(target.interval_secs.max(10))
    .bind(i64::from(target.enabled))
    .bind(i64::from(target.cve_scan))
    .bind(&tags)
    .bind(credential.map(|plain| cipher.encrypt(&plain)).transpose()?)
    .bind(&target.group_name)
    .bind(&target.note)
    .bind(target.position)
    .bind(id)
    .execute(&mut **tx)
    .await
    .context("mise à jour de l'équipement")?;
    Ok(RestoreOutcome::Updated)
}

/// Compare un secret chiffré en base au clair du lot, sans jamais les afficher.
pub(super) fn same_secret(cipher: &Cipher, stored: Option<&[u8]>, wanted: Option<&Value>) -> bool {
    let decrypted = stored
        .and_then(|bytes| cipher.decrypt(bytes).ok())
        .and_then(|plain| serde_json::from_slice::<Value>(&plain).ok());
    match (decrypted, wanted) {
        (None, None) => true,
        (Some(left), Some(right)) => &left == right,
        _ => false,
    }
}

pub(super) async fn restore_packs(tx: &mut Transaction<'_, Sqlite>, bundle: &Bundle) -> Result<SectionReport> {
    let mut report = SectionReport::new("packs");
    for entry in &bundle.packs {
        let pack = match dumbmonit_pack::Pack::parse(&entry.yaml) {
            Ok(pack) => pack,
            Err(error) => {
                report.skipped += 1;
                report.notes.push(format!(
                    "pack \"{}\" does not pass this server's validation: {}",
                    entry.id,
                    error.errors.join("; ")
                ));
                continue;
            }
        };
        let existing = sqlx::query("SELECT sha256, enabled FROM packs WHERE id = ?")
            .bind(pack.id())
            .fetch_optional(&mut **tx)
            .await
            .context("recherche du paquet")?;
        if let Some(row) = &existing
            && row.try_get::<String, _>("sha256")? == pack.sha256()
            && (row.try_get::<i64, _>("enabled")? != 0) == entry.enabled
        {
            report.skipped += 1;
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO packs (id, version, yaml, sha256, enabled) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET version = excluded.version, yaml = excluded.yaml,
                 sha256 = excluded.sha256, enabled = excluded.enabled,
                 installed_at = datetime('now')",
        )
        .bind(pack.id())
        .bind(pack.version())
        .bind(pack.yaml())
        .bind(pack.sha256())
        .bind(i64::from(entry.enabled))
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) if existing.is_some() => report.count(RestoreOutcome::Updated),
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused(&format!("pack \"{}\"", entry.id), &error.into()),
        }
    }
    Ok(report)
}
