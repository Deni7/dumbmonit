//! Vue d'ensemble : `get_status` et le résumé du tableau de bord.

use super::*;

pub(super) async fn get_status(state: &AppState) -> ToolResult {
    use crate::alerting::machine::EffectivePhase;

    let devices = load_devices(state).await?;
    let alerts = active_alerts(state).await?;
    let parents = parent_names(&devices);

    let count = |wanted: DeviceState| devices.iter().filter(|d| d.state == wanted).count();
    let reporting = count(DeviceState::Reporting);
    let unreachable = count(DeviceState::Unreachable) + count(DeviceState::Down);
    let waiting = count(DeviceState::Waiting);
    let disabled = count(DeviceState::Disabled);

    let firing: Vec<&ActiveAlertView> =
        alerts.iter().filter(|a| a.effective_phase == EffectivePhase::Firing).collect();
    let pending = alerts.iter().filter(|a| a.effective_phase == EffectivePhase::Pending).count();
    let suppressed =
        alerts.iter().filter(|a| a.effective_phase == EffectivePhase::Suppressed).count();
    let warnings = firing.iter().filter(|a| a.severity == Severity::Critical).count();
    let advisories = firing.iter().filter(|a| a.severity == Severity::Warning).count();
    let notices = firing.iter().filter(|a| a.severity == Severity::Info).count();

    // Un équipement injoignable dont la panne est déjà portée par une alerte
    // (règle « device unreachable ») compte une fois, pas deux — comme dans
    // l'interface.
    let voiced: std::collections::HashSet<TargetId> =
        firing.iter().filter_map(|a| a.target_id).collect();
    let silent_outages: Vec<&Device> =
        devices.iter().filter(|d| d.state.is_trouble() && !voiced.contains(&d.id())).collect();

    let sentence = if devices.is_empty() {
        "Nothing to watch yet.".to_string()
    } else {
        let mut parts = Vec::new();
        if warnings > 0 {
            parts.push(plural(warnings, "warning", "warnings"));
        }
        if advisories > 0 {
            parts.push(plural(advisories, "advisory", "advisories"));
        }
        if notices > 0 {
            parts.push(plural(notices, "notice", "notices"));
        }
        if !silent_outages.is_empty() {
            parts.push(format!("{} unreachable", silent_outages.len()));
        }
        if pending > 0 {
            parts.push(format!("{pending} building up"));
        }
        if !parts.is_empty() {
            format!("{}.", parts.join(", "))
        } else if reporting == 0 && waiting > 0 {
            "Waiting for the first reports.".to_string()
        } else {
            "Clear skies.".to_string()
        }
    };

    let mut text = format!(
        "**{sentence}**\n\nDevices: {} total — {reporting} reporting, {unreachable} unreachable, \
         {waiting} waiting, {disabled} disabled.\nAlerts: {warnings} warning(s), {advisories} \
         advisory(ies), {notices} notice(s) firing; {pending} building up; {suppressed} \
         suppressed by parent.",
        devices.len()
    );
    if !silent_outages.is_empty() || !firing.is_empty() {
        text.push_str("\n\nNeeds you:");
        for device in &silent_outages {
            text.push('\n');
            text.push_str(&device.line(&parents));
        }
        for alert in &firing {
            text.push('\n');
            text.push_str(&alert_line(alert, &parents));
        }
    }

    Ok(ToolOutput::new(
        text,
        json!({
            "sentence": sentence,
            "devices": { "total": devices.len(), "reporting": reporting, "unreachable": unreachable, "waiting": waiting, "disabled": disabled },
            "alerts": { "warnings": warnings, "advisories": advisories, "notices": notices, "building_up": pending, "suppressed": suppressed },
            "unreachable_devices": silent_outages.iter().map(|d| d.summary(&parents)).collect::<Vec<_>>(),
            "firing": firing.iter().map(|a| alert_json(a, &parents)).collect::<Vec<_>>(),
        }),
    ))
}

/// Résumé plat de l'état de l'instance pour les tableaux de bord tiers
/// (`GET /api/summary`, widget `customapi` de Homepage) : les compteurs de
/// `get_status`, sans phrase ni liste, avec un verdict global.
pub async fn dashboard_summary(state: &AppState) -> Result<Value, ToolError> {
    let status = get_status(state).await?.structured.unwrap_or(Value::Null);
    let count = |pointer: &str| status.pointer(pointer).and_then(Value::as_u64).unwrap_or(0);
    let total = count("/devices/total");
    let up = count("/devices/reporting");
    let down = count("/devices/unreachable");
    let waiting = count("/devices/waiting");
    let disabled = count("/devices/disabled");
    let critical = count("/alerts/warnings");
    let warning = count("/alerts/advisories");
    let info = count("/alerts/notices");
    let firing = critical + warning + info;
    // « ok » : rien à faire. « warning » : une alerte de moindre gravité.
    // « critical » : une alerte critique ou un équipement injoignable.
    let verdict = if critical > 0 || down > 0 {
        "critical"
    } else if warning > 0 || info > 0 {
        "warning"
    } else {
        "ok"
    };
    Ok(json!({
        "status": verdict,
        "sentence": status.get("sentence").cloned().unwrap_or(Value::Null),
        "devices": { "total": total, "up": up, "down": down, "waiting": waiting, "disabled": disabled },
        "alerts": {
            "firing": firing,
            "critical": critical,
            "warning": warning,
            "info": info,
            "building_up": count("/alerts/building_up"),
            "suppressed": count("/alerts/suppressed"),
        },
    }))
}
