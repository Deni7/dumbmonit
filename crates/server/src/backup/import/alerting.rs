//! Règles d'alerte, surcharges et politique de notification.

use super::*;

/// Rend au sélecteur ses identifiants locaux.
fn selector_from_bundle(
    report: &mut SectionReport,
    value: &Value,
    refs: &HashMap<String, i64>,
    rule: &str,
) -> String {
    if value.get("kind").and_then(Value::as_str) == Some("refs") {
        let listed = value.get("refs").and_then(Value::as_array).cloned().unwrap_or_default();
        let mut ids = Vec::new();
        for reference in listed.iter().filter_map(Value::as_str) {
            match refs.get(reference) {
                Some(id) => ids.push(*id),
                None => report.notes.push(format!(
                    "Rule \"{rule}\": it targeted {reference}, which is not in this backup; that \
                     device was dropped from its scope."
                )),
            }
        }
        // Un sélecteur devenu vide viserait plus rien : la règle serait muette
        // sans le dire. On la remet sur tous les équipements, ce que le compte
        // rendu signale.
        if ids.is_empty() {
            report.notes.push(format!(
                "Rule \"{rule}\": none of the devices it targeted exist here; it was restored as \
                 applying to every device. Narrow it again if that is not what you want."
            ));
            return r#"{"kind":"all"}"#.to_string();
        }
        return serde_json::json!({ "kind": "ids", "ids": ids }).to_string();
    }
    value.to_string()
}

pub(super) async fn restore_rules(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
    channels: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("rules");
    for rule in &bundle.rules {
        let selector = selector_from_bundle(&mut report, &rule.selector, refs, &rule.name);
        let mut ids = Vec::new();
        for name in &rule.channels {
            match channels.get(name) {
                Some(id) => ids.push(*id),
                None => report.notes.push(format!(
                    "Rule \"{}\": the channel \"{name}\" it sends to is not in this backup; it \
                     was dropped from the rule.",
                    rule.name
                )),
            }
        }
        match upsert_rule(tx, rule, &selector, &ids).await {
            Ok(outcome) => report.count(outcome),
            Err(error) => report.refused(&format!("Rule \"{}\"", rule.name), &error),
        }
    }
    Ok(report)
}

async fn upsert_rule(
    tx: &mut Transaction<'_, Sqlite>,
    rule: &BundleRule,
    selector: &str,
    channels: &[i64],
) -> Result<RestoreOutcome> {
    let channel_json = serde_json::to_string(channels)?;
    let params = serde_json::to_string(&rule.params)?;

    let existing = sqlx::query(
        "SELECT id, name, description, kind, query, operator, threshold, clear_threshold,
                for_secs, severity, selector, channels, params, unit, repeat_secs,
                escalate_after_secs, enabled
         FROM alert_rules WHERE uid = ?",
    )
    .bind(&rule.uid)
    .fetch_optional(&mut **tx)
    .await
    .context("recherche de la règle")?;

    if let Some(row) = &existing {
        let same = row.try_get::<String, _>("name")? == rule.name
            && row.try_get::<String, _>("description")? == rule.description
            && row.try_get::<String, _>("kind")? == rule.kind
            && row.try_get::<String, _>("query")? == rule.query
            && row.try_get::<String, _>("operator")? == rule.operator
            && row.try_get::<f64, _>("threshold")? == rule.threshold
            && row.try_get::<Option<f64>, _>("clear_threshold")? == rule.clear_threshold
            && row.try_get::<i64, _>("for_secs")? == rule.for_secs
            && row.try_get::<String, _>("severity")? == rule.severity
            && row.try_get::<String, _>("selector")? == selector
            && row.try_get::<String, _>("channels")? == channel_json
            // Les réglages (anomalie, prévision, dérive) se comparent en JSON
            // structuré : l'ordre des clés diffère entre la base et le lot.
            && serde_json::from_str::<Value>(&row.try_get::<String, _>("params")?).ok().as_ref()
                == Some(&rule.params)
            && row.try_get::<String, _>("unit")? == rule.unit
            && row.try_get::<Option<i64>, _>("repeat_secs")? == rule.repeat_secs
            && row.try_get::<Option<i64>, _>("escalate_after_secs")? == rule.escalate_after_secs
            && (row.try_get::<i64, _>("enabled")? != 0) == rule.enabled;
        if same {
            return Ok(RestoreOutcome::Skipped);
        }
    }

    sqlx::query(
        "INSERT INTO alert_rules
             (uid, name, description, kind, query, operator, threshold, clear_threshold,
              for_secs, severity, selector, channels, params, unit, repeat_secs,
              escalate_after_secs, enabled, builtin, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
         ON CONFLICT(uid) DO UPDATE SET
             name = excluded.name, description = excluded.description, kind = excluded.kind,
             query = excluded.query, operator = excluded.operator,
             threshold = excluded.threshold, clear_threshold = excluded.clear_threshold,
             for_secs = excluded.for_secs, severity = excluded.severity,
             selector = excluded.selector, channels = excluded.channels,
             params = excluded.params, unit = excluded.unit,
             repeat_secs = excluded.repeat_secs,
             escalate_after_secs = excluded.escalate_after_secs,
             enabled = excluded.enabled, updated_at = datetime('now')",
    )
    .bind(&rule.uid)
    .bind(&rule.name)
    .bind(&rule.description)
    .bind(&rule.kind)
    .bind(&rule.query)
    .bind(&rule.operator)
    .bind(rule.threshold)
    .bind(rule.clear_threshold)
    .bind(rule.for_secs)
    .bind(&rule.severity)
    .bind(selector)
    .bind(&channel_json)
    .bind(&params)
    .bind(&rule.unit)
    .bind(rule.repeat_secs)
    .bind(rule.escalate_after_secs)
    .bind(i64::from(rule.enabled))
    .bind(i64::from(rule.builtin))
    .execute(&mut **tx)
    .await
    .context("enregistrement de la règle")?;

    Ok(if existing.is_some() { RestoreOutcome::Updated } else { RestoreOutcome::Created })
}

pub(super) async fn restore_overrides(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("rule_overrides");
    for over in &bundle.rule_overrides {
        let Some(target_id) = refs.get(&over.target).copied() else {
            report.skipped += 1;
            report.notes.push(format!(
                "Per-monitor setting of rule \"{}\": its monitor ({}) is not in this backup.",
                over.rule_uid, over.target
            ));
            continue;
        };
        let existing = sqlx::query(
            "SELECT threshold, clear_threshold, enabled FROM rule_overrides
             WHERE rule_uid = ? AND target_id = ?",
        )
        .bind(&over.rule_uid)
        .bind(target_id)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de la surcharge")?;

        if let Some(row) = &existing {
            let enabled: Option<i64> = row.try_get("enabled")?;
            if row.try_get::<Option<f64>, _>("threshold")? == over.threshold
                && row.try_get::<Option<f64>, _>("clear_threshold")? == over.clear_threshold
                && enabled.map(|v| v != 0) == over.enabled
            {
                report.skipped += 1;
                continue;
            }
        }

        sqlx::query(
            "INSERT INTO rule_overrides (rule_uid, target_id, threshold, clear_threshold, enabled)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(rule_uid, target_id) DO UPDATE SET
                 threshold = excluded.threshold, clear_threshold = excluded.clear_threshold,
                 enabled = excluded.enabled, updated_at = datetime('now')",
        )
        .bind(&over.rule_uid)
        .bind(target_id)
        .bind(over.threshold)
        .bind(over.clear_threshold)
        .bind(over.enabled.map(i64::from))
        .execute(&mut **tx)
        .await
        .context("enregistrement de la surcharge")?;
        report.count(if existing.is_some() {
            RestoreOutcome::Updated
        } else {
            RestoreOutcome::Created
        });
    }
    Ok(report)
}

pub(super) async fn restore_notify_policy(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("notify_policy");
    let Some(policy) = &bundle.notify_policy else { return Ok(report) };
    let value = serde_json::to_vec(policy)?;

    let existing = sqlx::query("SELECT value FROM settings WHERE key = ?")
        .bind(crate::notify::policy_store::POLICY_KEY)
        .fetch_optional(&mut **tx)
        .await
        .context("lecture de la politique de notification")?;
    if let Some(row) = &existing
        && row.try_get::<Vec<u8>, _>("value")? == value
    {
        report.skipped += 1;
        return Ok(report);
    }

    sqlx::query(
        "INSERT INTO settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value,
             updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
    )
    .bind(crate::notify::policy_store::POLICY_KEY)
    .bind(&value)
    .execute(&mut **tx)
    .await
    .context("écriture de la politique de notification")?;
    report.count(if existing.is_some() {
        RestoreOutcome::Updated
    } else {
        RestoreOutcome::Created
    });
    Ok(report)
}
