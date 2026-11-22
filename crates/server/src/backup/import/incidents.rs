//! Incidents, post-mortems et objectifs de niveau de service.

use super::*;

pub(super) async fn restore_incidents(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("incidents");
    let pages: HashMap<String, i64> = sqlx::query("SELECT id, slug FROM status_pages")
        .fetch_all(&mut **tx)
        .await
        .context("relecture des pages de statut")?
        .iter()
        .map(|row| Ok((row.try_get("slug")?, row.try_get("id")?)))
        .collect::<Result<_>>()?;

    for incident in &bundle.incidents {
        let page_id = match &incident.page {
            None => None,
            Some(slug) => match pages.get(slug) {
                Some(id) => Some(*id),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "Incident \"{}\": its status page ({slug}) is not in this backup.",
                        incident.title
                    ));
                    continue;
                }
            },
        };

        // Un incident est un fait daté, pas un réglage : s'il est déjà là, on
        // le laisse tel quel plutôt que de réécrire un récit.
        let existing = sqlx::query(
            "SELECT id FROM incidents WHERE page_id IS ? AND title = ? AND starts_at = ?",
        )
        .bind(page_id)
        .bind(&incident.title)
        .bind(&incident.starts_at)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de l'incident")?;
        if let Some(row) = existing {
            // Le post-mortem du lot rejoint un incident déjà là qui n'en a pas ;
            // celui que l'instance a déjà écrit n'est jamais remplacé.
            let id: i64 = row.try_get("id")?;
            let added = match &incident.postmortem {
                Some(postmortem) => {
                    restore_postmortem(tx, id, &incident.status, postmortem).await?
                }
                None => false,
            };
            if added {
                report.updated += 1;
            } else {
                report.skipped += 1;
            }
            continue;
        }

        let row = sqlx::query(
            "INSERT INTO incidents (page_id, title, kind, status, severity, starts_at, ends_at)
             VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(page_id)
        .bind(&incident.title)
        .bind(&incident.kind)
        .bind(&incident.status)
        .bind(&incident.severity)
        .bind(&incident.starts_at)
        .bind(&incident.ends_at)
        .fetch_one(&mut **tx)
        .await
        .context("création de l'incident")?;
        let id: i64 = row.try_get("id")?;

        for update in &incident.updates {
            sqlx::query(
                "INSERT INTO incident_updates (incident_id, status, body, created_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(id)
            .bind(&update.status)
            .bind(&update.body)
            .bind(&update.created_at)
            .execute(&mut **tx)
            .await
            .context("création d'une mise à jour d'incident")?;
        }
        if let Some(postmortem) = &incident.postmortem {
            restore_postmortem(tx, id, &incident.status, postmortem).await?;
        }
        report.created += 1;
    }
    Ok(report)
}

/// Écrit le post-mortem d'un incident s'il n'en a pas encore. Un lot édité à
/// la main ne publie pas le post-mortem d'un incident encore ouvert.
async fn restore_postmortem(
    tx: &mut Transaction<'_, Sqlite>,
    incident_id: i64,
    status: &str,
    postmortem: &BundlePostmortem,
) -> Result<bool> {
    let published = postmortem.published && matches!(status, "resolved" | "completed");
    let published_at = if published {
        postmortem.published_at.clone().or_else(|| Some(crate::db::now()))
    } else {
        None
    };
    let done = sqlx::query(
        "INSERT OR IGNORE INTO incident_postmortems
             (incident_id, summary, impact, root_cause, timeline, actions, published, published_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(incident_id)
    .bind(&postmortem.summary)
    .bind(&postmortem.impact)
    .bind(&postmortem.root_cause)
    .bind(serde_json::to_string(&postmortem.timeline)?)
    .bind(serde_json::to_string(&postmortem.actions)?)
    .bind(published)
    .bind(published_at)
    .execute(&mut **tx)
    .await
    .context("création d'un post-mortem")?;
    Ok(done.rows_affected() > 0)
}

pub(super) async fn restore_slos(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("slos");
    for item in &bundle.slos {
        let mut targets = Vec::new();
        for reference in &item.scope_targets {
            match refs.get(reference) {
                Some(id) => targets.push(*id),
                None => report.notes.push(format!(
                    "SLO \"{}\": the monitor \"{reference}\" of its scope is not in this backup \
                     and was left out.",
                    item.name
                )),
            }
        }
        // Un périmètre dont aucun moniteur nommé n'a survécu, et sans dossier,
        // couvrirait toute l'instance : le SLO est écarté plutôt qu'élargi.
        if targets.is_empty() && item.scope_groups.is_empty() && !item.scope_targets.is_empty() {
            report.skipped += 1;
            report.notes.push(format!(
                "SLO \"{}\": none of its monitors is in this backup; it was skipped.",
                item.name
            ));
            continue;
        }
        let input = crate::slo::SloInput {
            name: item.name.clone(),
            description: item.description.clone(),
            objective: item.objective,
            window_days: item.window_days,
            scope: crate::reports::layout::Scope { targets, groups: item.scope_groups.clone() },
            enabled: item.enabled,
        };
        let input = match input.validate() {
            Ok(input) => input,
            Err(error) => {
                report.skipped += 1;
                report.notes.push(format!("SLO \"{}\" was skipped: {error}", item.name));
                continue;
            }
        };
        let scope_json = serde_json::to_string(&input.scope)?;
        let existing = sqlx::query(
            "SELECT id, description, objective, window_days, scope, enabled
             FROM slos WHERE name = ?",
        )
        .bind(&input.name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du SLO")?;
        match existing {
            Some(row) => {
                let same = row.try_get::<String, _>("description")? == input.description
                    && row.try_get::<f64, _>("objective")? == input.objective
                    && row.try_get::<i64, _>("window_days")? == i64::from(input.window_days)
                    && row.try_get::<String, _>("scope")? == scope_json
                    && (row.try_get::<i64, _>("enabled")? != 0) == input.enabled;
                if same {
                    report.skipped += 1;
                    continue;
                }
                sqlx::query(
                    "UPDATE slos SET description = ?, objective = ?, window_days = ?, scope = ?,
                         enabled = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                     WHERE id = ?",
                )
                .bind(&input.description)
                .bind(input.objective)
                .bind(i64::from(input.window_days))
                .bind(&scope_json)
                .bind(input.enabled)
                .bind(row.try_get::<i64, _>("id")?)
                .execute(&mut **tx)
                .await
                .context("mise à jour du SLO")?;
                report.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO slos (name, description, objective, window_days, scope, enabled)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&input.name)
                .bind(&input.description)
                .bind(input.objective)
                .bind(i64::from(input.window_days))
                .bind(&scope_json)
                .bind(input.enabled)
                .execute(&mut **tx)
                .await
                .context("création du SLO")?;
                report.created += 1;
            }
        }
    }
    Ok(report)
}
