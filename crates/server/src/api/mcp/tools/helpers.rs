//! Mots et mises en forme partagés par les outils : alertes, dates, pluriels.

use super::*;

pub(super) fn format_tags(tags: &BTreeMap<String, String>) -> String {
    tags.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ")
}

/// Le mot de l'interface pour une sévérité interne.
pub(super) fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "advisory",
        Severity::Critical => "warning",
    }
}

/// Le mot sur la plaque : sévérité, ou surcouche (suppressed, building up).
fn alert_word(alert: &ActiveAlertView) -> &'static str {
    use crate::alerting::machine::EffectivePhase;
    match alert.effective_phase {
        EffectivePhase::Suppressed => "suppressed by parent",
        EffectivePhase::Pending => "building up",
        _ if alert.learning => "learning",
        _ => severity_word(alert.severity),
    }
}

pub(super) async fn active_alerts(state: &AppState) -> Result<Vec<ActiveAlertView>, ToolError> {
    let Json(alerts) = alerts::list_active(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Query(alerts::ActiveQuery::default()),
    )
    .await?;
    Ok(alerts)
}

pub(super) fn alert_json(alert: &ActiveAlertView, devices: &HashMap<TargetId, String>) -> Value {
    json!({
        "fingerprint": alert.fingerprint,
        "rule_uid": alert.rule_uid,
        "rule": alert.rule_name,
        "severity": severity_word(alert.severity),
        "state": alert_word(alert),
        "device_id": alert.target_id,
        "device": alert.target_id.and_then(|id| devices.get(&id)),
        "suppressed_by": alert.suppressed_by.and_then(|id| devices.get(&id)),
        "silenced": alert.silenced,
        "acknowledged": alert.acked,
        "acked_by": alert.acked.then_some(alert.acked_by.as_deref()).flatten(),
        "acked_until": alert.acked.then_some(alert.acked_until.map(rfc3339)).flatten(),
        "ack_note": alert.acked.then_some(alert.ack_note.as_deref()).flatten(),
        "dismissed": alert.dismissed,
        "dismissed_by": alert.dismissed.then_some(alert.dismissed_by.as_deref()).flatten(),
        "value": alert.value,
        "since": alert.firing_since.or(alert.condition_since).map(rfc3339),
        "labels": alert.labels,
    })
}

pub(super) fn alert_line(alert: &ActiveAlertView, devices: &HashMap<TargetId, String>) -> String {
    let device = alert
        .target_id
        .and_then(|id| devices.get(&id).cloned())
        .unwrap_or_else(|| "(no device)".to_string());
    let mut line = format!("- [{}] {} — {}", alert_word(alert), alert.rule_name, device);
    if let Some(since) = alert.firing_since.or(alert.condition_since) {
        line.push_str(&format!(", since {}", rfc3339(since)));
    }
    if let Some(value) = alert.value {
        line.push_str(&format!(", value {}", trim_float(value)));
    }
    let detail: Vec<String> = alert
        .labels
        .iter()
        .filter(|(k, _)| {
            !["target", "target_id", "host", "hostname", "instance", "__name__"]
                .contains(&k.as_str())
        })
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    if !detail.is_empty() {
        line.push_str(&format!(" ({})", detail.join(", ")));
    }
    if let Some(parent) = alert.suppressed_by.and_then(|id| devices.get(&id)) {
        line.push_str(&format!("; suppressed by {parent}"));
    }
    if alert.silenced {
        line.push_str("; silenced");
    }
    if alert.acked {
        line.push_str("; acknowledged");
        if let Some(who) = &alert.acked_by {
            line.push_str(&format!(" by {who}"));
        }
        if let Some(until) = alert.acked_until {
            line.push_str(&format!(" until {}", rfc3339(until)));
        }
        if let Some(note) = &alert.ack_note {
            line.push_str(&format!(" ({note})"));
        }
    }
    line
}

pub(super) fn rfc3339(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(super) fn trim_float(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value:.2}")
    }
}

pub(super) fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}
