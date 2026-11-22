//! Topologie : corrections, liens de LUN, règles et notes de mise à jour.

use super::*;


/// Un nœud du lot, avec les identifiants d'ici ; `None` : son équipement
/// n'est pas là.
fn local_node(node: &BundleNode, refs: &HashMap<String, i64>) -> Option<(String, i64)> {
    let host = *refs.get(&node.target)?;
    Some((topology_edits::node_from_parts(host, node.child.as_deref())?, host))
}

/// Corrections de la carte : une correction identique déjà là est laissée.
/// Le parent d'alerte d'un rattachement voyage déjà avec l'équipement
/// (`parent`) : rien n'est réécrit dans `targets`.
pub(super) async fn restore_topology_corrections(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("topology_corrections");
    for entry in &bundle.topology_corrections {
        let Some(kind) = CorrectionKind::parse(&entry.kind) else {
            report.skipped += 1;
            report.notes.push(format!("map correction of unknown kind \"{}\"", entry.kind));
            continue;
        };
        let missing = |report: &mut SectionReport| {
            report.skipped += 1;
            report.notes.push(format!(
                "map correction on \"{}\": a device it points at is not in this instance",
                entry.to.target
            ));
        };
        let Some((to, to_target)) = local_node(&entry.to, refs) else {
            missing(&mut report);
            continue;
        };
        let (from, from_target) = match &entry.from {
            Some(node) => match local_node(node, refs) {
                Some((id, target)) => (Some(id), Some(target)),
                None => {
                    missing(&mut report);
                    continue;
                }
            },
            None => (None, None),
        };
        let same: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM topology_corrections
             WHERE kind = ? AND from_node IS ? AND to_node = ? AND source IS ? AND value IS ?",
        )
        .bind(kind.as_str())
        .bind(&from)
        .bind(&to)
        .bind(&entry.source)
        .bind(&entry.value)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de la correction")?;
        if same.is_some() {
            report.skipped += 1;
            continue;
        }
        let previous = entry.previous_parent.as_ref().and_then(|r| refs.get(r).copied());
        let result = sqlx::query(
            "INSERT INTO topology_corrections (kind, from_node, to_node, from_target, to_target,
                 source, value, use_for_alerts, previous_parent, author, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, COALESCE(NULLIF(?, ''), datetime('now')))",
        )
        .bind(kind.as_str())
        .bind(&from)
        .bind(&to)
        .bind(from_target)
        .bind(to_target)
        .bind(&entry.source)
        .bind(&entry.value)
        .bind(entry.use_for_alerts)
        .bind(previous)
        .bind(&entry.author)
        .bind(&entry.created_at)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused(&format!("map correction on \"{to}\""), &error.into()),
        }
    }
    Ok(report)
}

/// Liens de LUN, par (NAS, LUN) : identique, laissé ; différent, mis à jour.
pub(super) async fn restore_lun_links(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("lun_links");
    for entry in &bundle.lun_links {
        let target = refs.get(&entry.target).copied();
        let consumer = refs.get(&entry.consumer).copied();
        let (Some(target), Some(consumer)) = (target, consumer) else {
            report.skipped += 1;
            report.notes.push(format!(
                "LUN link \"{}\" on \"{}\": a device it points at is not in this instance",
                entry.lun, entry.target
            ));
            continue;
        };
        let existing: Option<(i64, String, String)> = sqlx::query_as(
            "SELECT consumer_target_id, family, entity FROM lun_links
             WHERE target_id = ? AND lun = ?",
        )
        .bind(target)
        .bind(&entry.lun)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du lien de LUN")?;
        let wanted = (consumer, entry.family.clone(), entry.entity.clone());
        let outcome = match &existing {
            Some(row) if *row == wanted => {
                report.count(RestoreOutcome::Skipped);
                continue;
            }
            Some(_) => RestoreOutcome::Updated,
            None => RestoreOutcome::Created,
        };
        let result = sqlx::query(
            "INSERT INTO lun_links (target_id, lun, consumer_target_id, family, entity, author)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (target_id, lun) DO UPDATE SET
                consumer_target_id = excluded.consumer_target_id,
                family = excluded.family,
                entity = excluded.entity,
                author = excluded.author",
        )
        .bind(target)
        .bind(&entry.lun)
        .bind(consumer)
        .bind(&entry.family)
        .bind(&entry.entity)
        .bind(&entry.author)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) => report.count(outcome),
            Err(error) => report.refused(&format!("LUN link \"{}\"", entry.lun), &error.into()),
        }
    }
    Ok(report)
}

/// Changements de configuration fermés, par (version, identifiant) : déjà
/// fermé, laissé tel quel — la date d'origine est conservée.
pub(super) async fn restore_upgrade_notes(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("upgrade_notes_closed");
    for entry in &bundle.upgrade_notes_closed {
        let valid = |text: &str| !text.is_empty() && text.len() <= 64;
        if !valid(&entry.version) || !valid(&entry.note_id) {
            report.skipped += 1;
            report.notes.push("A closed configuration change has no usable identifier".into());
            continue;
        }
        // Une date absente devient « maintenant ».
        let closed_at = (!entry.closed_at.trim().is_empty()).then_some(entry.closed_at.as_str());
        let result = sqlx::query(
            "INSERT INTO upgrade_notes_closed (version, note_id, closed_at, closed_by)
             VALUES (?, ?, COALESCE(?, strftime('%Y-%m-%d %H:%M:%S', 'now')), ?)
             ON CONFLICT (version, note_id) DO NOTHING",
        )
        .bind(&entry.version)
        .bind(&entry.note_id)
        .bind(closed_at)
        .bind(&entry.closed_by)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(done) if done.rows_affected() > 0 => report.count(RestoreOutcome::Created),
            Ok(_) => report.count(RestoreOutcome::Skipped),
            Err(error) => {
                let what = format!("configuration change \"{}\"", entry.note_id);
                report.refused(&what, &error.into());
            }
        }
    }
    Ok(report)
}

/// Règles de la carte, par leur clé : même état, laissée ; autre état, mise à jour.
pub(super) async fn restore_topology_rules(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("topology_rules");
    for entry in &bundle.topology_rules {
        let (Some(kind), Some(status)) =
            (RuleKind::parse(&entry.kind), RuleStatus::parse(&entry.status))
        else {
            report.skipped += 1;
            report.notes.push(format!("map rule of unknown kind \"{}\"", entry.kind));
            continue;
        };
        let parent = match &entry.parent {
            Some(node) => match local_node(node, refs) {
                Some(found) => Some(found),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "map rule: the device \"{}\" it depends on is not in this instance",
                        node.target
                    ));
                    continue;
                }
            },
            None => None,
        };
        let spec = RuleSpec {
            kind,
            subnet: entry.subnet.clone(),
            address: entry.address.clone(),
            target_kind: entry.target_kind.clone(),
            source: entry.source.clone(),
            parent: parent.as_ref().map(|(id, _)| id.clone()),
        };
        let key = spec.key();
        let existing: Option<String> =
            sqlx::query_scalar("SELECT status FROM topology_rules WHERE key = ?")
                .bind(&key)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche de la règle de carte")?;
        if existing.as_deref() == Some(status.as_str()) {
            report.skipped += 1;
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO topology_rules (key, kind, status, subnet, address, target_kind, source,
                 parent_node, parent_target, evidence, author, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, COALESCE(NULLIF(?, ''), datetime('now')))
             ON CONFLICT(key) DO UPDATE SET status = excluded.status",
        )
        .bind(&key)
        .bind(kind.as_str())
        .bind(status.as_str())
        .bind(&spec.subnet)
        .bind(&spec.address)
        .bind(&spec.target_kind)
        .bind(&spec.source)
        .bind(&spec.parent)
        .bind(parent.map(|(_, target)| target))
        .bind(entry.evidence)
        .bind(&entry.author)
        .bind(&entry.created_at)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) if existing.is_some() => report.count(RestoreOutcome::Updated),
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused("map rule", &error.into()),
        }
    }
    Ok(report)
}
