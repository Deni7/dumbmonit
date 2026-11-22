//! Runbooks et remédiation automatique.

use super::*;


pub(super) async fn restore_runbooks(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    use crate::remediation::runbooks::RunbookDraft;
    let mut report = SectionReport::new("runbooks");
    for entry in &bundle.runbooks {
        let draft = RunbookDraft {
            title: entry.title.clone(),
            body: entry.body.clone(),
            links: entry.links.clone(),
        };
        let draft = match draft.validate() {
            Ok(draft) => draft,
            Err(why) => {
                report.skipped += 1;
                report.notes.push(format!("runbook \"{}\": {why}", entry.title));
                continue;
            }
        };
        let existing: Option<(i64, String)> =
            sqlx::query_as("SELECT id, body FROM runbooks WHERE title = ? ORDER BY id LIMIT 1")
                .bind(&draft.title)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche de la procédure")?;
        let id = match &existing {
            Some((id, body)) => {
                let links: Vec<(String, String)> = sqlx::query_as(
                    "SELECT scope, value FROM runbook_links WHERE runbook_id = ?
                     ORDER BY scope, value",
                )
                .bind(id)
                .fetch_all(&mut **tx)
                .await
                .context("lecture des rattachements")?;
                let same_links = links.len() == draft.links.len()
                    && links
                        .iter()
                        .zip(&draft.links)
                        .all(|((scope, value), link)| *scope == link.scope && *value == link.value);
                if *body == draft.body && same_links {
                    report.skipped += 1;
                    continue;
                }
                sqlx::query(
                    "UPDATE runbooks SET body = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%S', 'now')
                     WHERE id = ?",
                )
                .bind(&draft.body)
                .bind(id)
                .execute(&mut **tx)
                .await
                .context("mise à jour de la procédure")?;
                *id
            }
            None => sqlx::query_scalar(
                "INSERT INTO runbooks (title, body, updated_by) VALUES (?, ?, 'backup')
                 RETURNING id",
            )
            .bind(&draft.title)
            .bind(&draft.body)
            .fetch_one(&mut **tx)
            .await
            .context("création de la procédure")?,
        };
        sqlx::query("DELETE FROM runbook_links WHERE runbook_id = ?")
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("effacement des rattachements")?;
        for link in &draft.links {
            sqlx::query("INSERT INTO runbook_links (runbook_id, scope, value) VALUES (?, ?, ?)")
                .bind(id)
                .bind(&link.scope)
                .bind(&link.value)
                .execute(&mut **tx)
                .await
                .context("écriture d'un rattachement")?;
        }
        report.count(if existing.is_some() {
            RestoreOutcome::Updated
        } else {
            RestoreOutcome::Created
        });
    }
    Ok(report)
}

/// Actions, par nom. Une action revalidée par le catalogue de ce serveur ; une
/// action modifiée désarme les politiques qui l'emploient.
pub(super) async fn restore_remediation_actions(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("remediation_actions");
    for entry in &bundle.remediation_actions {
        let spec = match crate::remediation::ActionSpec::parse(&entry.kind, &entry.params) {
            Ok(spec) => spec,
            Err(why) => {
                report.skipped += 1;
                report.notes.push(format!("action \"{}\": {why}", entry.name));
                continue;
            }
        };
        if let Err(why) = crate::remediation::validate_secrets(spec.kind(), &entry.secrets) {
            report.skipped += 1;
            report.notes.push(format!("action \"{}\": {why}", entry.name));
            continue;
        }
        let target = match &entry.target {
            Some(reference) => match refs.get(reference) {
                Some(id) => Some(*id),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "action \"{}\": the device \"{reference}\" is not in this instance",
                        entry.name
                    ));
                    continue;
                }
            },
            None => None,
        };
        let params = spec.to_params().to_string();
        let secrets = if entry.secrets.values().any(|value| !value.is_empty()) {
            Some(cipher.encrypt(&serde_json::to_vec(&entry.secrets)?)?)
        } else {
            None
        };
        let existing: Option<(i64, String, String, Option<i64>)> = sqlx::query_as(
            "SELECT id, kind, params, target_id FROM remediation_actions WHERE name = ?",
        )
        .bind(&entry.name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de l'action")?;
        match existing {
            Some((_, kind, current, target_id))
                if kind == spec.kind() && current == params && target_id == target =>
            {
                report.skipped += 1;
            }
            Some((id, ..)) => {
                sqlx::query(
                    "UPDATE remediation_actions SET kind = ?, params = ?, secrets = ?, target_id = ?,
                         updated_at = strftime('%Y-%m-%dT%H:%M:%S', 'now')
                     WHERE id = ?",
                )
                .bind(spec.kind())
                .bind(&params)
                .bind(&secrets)
                .bind(target)
                .bind(id)
                .execute(&mut **tx)
                .await
                .context("mise à jour de l'action")?;
                sqlx::query(
                    "UPDATE remediation_policies SET enabled = 0, armed_by = NULL, armed_at = NULL
                     WHERE action_id = ?",
                )
                .bind(id)
                .execute(&mut **tx)
                .await
                .context("désarmement des politiques")?;
                report.count(RestoreOutcome::Updated);
            }
            None => {
                let result = sqlx::query(
                    "INSERT INTO remediation_actions (name, kind, params, secrets, target_id)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&entry.name)
                .bind(spec.kind())
                .bind(&params)
                .bind(&secrets)
                .bind(target)
                .execute(&mut **tx)
                .await;
                match result {
                    Ok(_) => report.count(RestoreOutcome::Created),
                    Err(error) => report.refused("action", &error.into()),
                }
            }
        }
    }
    Ok(report)
}

/// Politiques : toujours créées désarmées, jamais en double (même action,
/// même règle, même équipement).
pub(super) async fn restore_remediation_policies(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("remediation_policies");
    for entry in &bundle.remediation_policies {
        let action: Option<i64> =
            sqlx::query_scalar("SELECT id FROM remediation_actions WHERE name = ?")
                .bind(&entry.action)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche de l'action de la politique")?;
        let Some(action) = action else {
            report.skipped += 1;
            report.notes.push(format!("policy: the action \"{}\" is missing", entry.action));
            continue;
        };
        let target = match &entry.target {
            Some(reference) => match refs.get(reference) {
                Some(id) => Some(*id),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "policy: the device \"{reference}\" is not in this instance"
                    ));
                    continue;
                }
            },
            None => None,
        };
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM remediation_policies
             WHERE action_id = ? AND rule_uid = ? AND target_id IS ?",
        )
        .bind(action)
        .bind(&entry.rule_uid)
        .bind(target)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de la politique")?;
        if exists.is_some() {
            report.skipped += 1;
            continue;
        }
        let result = sqlx::query(
            "INSERT INTO remediation_policies
                 (action_id, rule_uid, target_id, after_minutes, max_attempts, window_minutes,
                  enabled)
             VALUES (?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(action)
        .bind(&entry.rule_uid)
        .bind(target)
        .bind(entry.after_minutes)
        .bind(entry.max_attempts)
        .bind(entry.window_minutes)
        .execute(&mut **tx)
        .await;
        match result {
            Ok(_) => report.count(RestoreOutcome::Created),
            Err(error) => report.refused("policy", &error.into()),
        }
    }
    Ok(report)
}
