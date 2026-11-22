//! Alertes : lecture, historique, acquittement et rejet.

use super::*;

pub(super) async fn list_alerts(state: &AppState, args: &Args) -> ToolResult {
    use crate::alerting::machine::EffectivePhase;

    let devices = load_devices(state).await?;
    let parents = parent_names(&devices);
    let only = match args.device_ref() {
        Some(reference) => Some(resolve(&devices, &reference)?.id()),
        None => None,
    };
    let include_pending = args.bool("include_pending").unwrap_or(false);

    let alerts = active_alerts(state).await?;
    let selected: Vec<&ActiveAlertView> = alerts
        .iter()
        .filter(|a| include_pending || a.effective_phase != EffectivePhase::Pending)
        .filter(|a| only.is_none_or(|id| a.target_id == Some(id)))
        .collect();

    let text = if selected.is_empty() {
        match only {
            Some(id) => {
                format!("No active alert on {}.", parents.get(&id).cloned().unwrap_or_default())
            }
            None => "No active alert.".to_string(),
        }
    } else {
        let mut lines =
            vec![format!("{}:", plural(selected.len(), "active alert", "active alerts"))];
        lines.extend(selected.iter().map(|a| alert_line(a, &parents)));
        lines.join("\n")
    };
    Ok(ToolOutput::new(
        text,
        json!({ "alerts": selected.iter().map(|a| alert_json(a, &parents)).collect::<Vec<_>>() }),
    ))
}

pub(super) async fn alert_history(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let parents = parent_names(&devices);
    let only = match args.device_ref() {
        Some(reference) => Some(resolve(&devices, &reference)?.id()),
        None => None,
    };
    let since = match args.str("since") {
        Some(raw) => raw.to_string(),
        None => {
            let hours = args.f64("hours").filter(|h| h.is_finite() && *h > 0.0).unwrap_or(24.0);
            let hours = hours.min(MAX_RANGE_HOURS);
            rfc3339(Utc::now() - TimeDelta::milliseconds((hours * 3_600_000.0) as i64))
        }
    };
    let limit = args.i64("limit").unwrap_or(DEFAULT_HISTORY_LIMIT).clamp(1, MAX_HISTORY_LIMIT);

    // L'historique n'est pas filtrable par équipement côté base : on demande plus
    // large et on filtre ici, borné pour ne pas rapatrier des mois.
    let fetch = if only.is_some() { (limit * 10).min(5_000) } else { limit };
    let Json(entries) = alerts::history(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Query(HistoryQuery { since: Some(since.clone()), limit: Some(fetch), dismissed: None }),
    )
    .await?;
    let Json(rules) = alerts::list_rules(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let rule_names: HashMap<&str, &str> =
        rules.iter().map(|r| (r.uid.as_str(), r.name.as_str())).collect();

    let selected: Vec<_> = entries
        .iter()
        .filter(|e| only.is_none_or(|id| e.target_id == Some(id)))
        .take(limit as usize)
        .collect();

    let text = if selected.is_empty() {
        format!("No alert transition since {since}.")
    } else {
        let mut lines = vec![format!(
            "{} since {since} (most recent first):",
            plural(selected.len(), "transition", "transitions")
        )];
        for e in &selected {
            let device = e
                .target_id
                .and_then(|id| parents.get(&id).cloned())
                .unwrap_or_else(|| "(no device)".to_string());
            let mut line = format!(
                "- {} — {} on {}: {} → {} [{}]",
                e.at,
                rule_names.get(e.rule_uid.as_str()).copied().unwrap_or(e.rule_uid.as_str()),
                device,
                e.from_phase.as_str(),
                e.to_phase.as_str(),
                severity_word(e.severity)
            );
            if let Some(value) = e.value {
                line.push_str(&format!(", value {}", trim_float(value)));
            }
            if !e.notified && !e.reason.is_empty() {
                line.push_str(&format!(" (not notified: {})", e.reason));
            }
            lines.push(line);
        }
        lines.join("\n")
    };

    let structured: Vec<Value> = selected
        .iter()
        .map(|e| {
            json!({
                "at": e.at,
                "rule_uid": e.rule_uid,
                "rule": rule_names.get(e.rule_uid.as_str()),
                "device_id": e.target_id,
                "device": e.target_id.and_then(|id| parents.get(&id)),
                "from": e.from_phase.as_str(),
                "to": e.to_phase.as_str(),
                "severity": severity_word(e.severity),
                "value": e.value,
                "notified": e.notified,
                "reason": e.reason,
            })
        })
        .collect();
    Ok(ToolOutput::new(text, json!({ "since": since, "entries": structured })))
}

pub(super) async fn acknowledge_alert(state: &AppState, args: &Args) -> ToolResult {
    let fingerprint = args
        .str("fingerprint")
        .ok_or_else(|| failed("\"fingerprint\" (see list_alerts) is required."))?
        .to_string();
    let hours = args.f64("hours").unwrap_or(DEFAULT_ACK_HOURS);
    if !hours.is_finite() || hours < 0.0 {
        return Err(failed("\"hours\" must be a number greater than or equal to 0."));
    }
    let payload = if hours == 0.0 {
        AckPayload { until: Some(None), duration_secs: None, note: None }
    } else {
        AckPayload {
            until: None,
            duration_secs: Some((hours * 3600.0).round() as i64),
            note: args.str("note").map(str::to_string),
        }
    };
    let request = payload.validate(Utc::now())?;
    let lifted = request == alerts::AckRequest::Clear;
    let alert = alerts::acknowledge(state, &fingerprint, request, "assistant").await?;

    let devices = load_devices(state).await?;
    let names: HashMap<TargetId, String> =
        devices.iter().map(|device| (device.id(), device.name().to_string())).collect();
    let text = if lifted {
        format!("Acknowledgement lifted on {}: reminders resume.", alert.rule_name)
    } else {
        format!(
            "Acknowledged {} ({}) until {}: no reminder until then; the resolution will still \
             be notified. Lift it earlier with hours 0.",
            alert.rule_name,
            alert
                .target_id
                .and_then(|id| names.get(&id).cloned())
                .unwrap_or_else(|| "no device".into()),
            alert.acked_until.map(rfc3339).unwrap_or_default()
        )
    };
    Ok(ToolOutput::new(text, alert_json(&alert, &names)))
}

pub(super) async fn dismiss_alert(state: &AppState, args: &Args) -> ToolResult {
    let fingerprint = args
        .str("fingerprint")
        .ok_or_else(|| failed("\"fingerprint\" (see list_alerts) is required."))?
        .to_string();
    let undo = args.bool("undo").unwrap_or(false);
    let alert = alerts::dismiss(state, &fingerprint, !undo, "assistant").await?;

    let devices = load_devices(state).await?;
    let names: HashMap<TargetId, String> =
        devices.iter().map(|device| (device.id(), device.name().to_string())).collect();
    let device = alert
        .target_id
        .and_then(|id| names.get(&id).cloned())
        .unwrap_or_else(|| "no device".into());
    let text = if undo {
        format!("{} ({device}) restored: it is listed and notifies again.", alert.rule_name)
    } else {
        format!(
            "Dismissed {} ({device}): hidden from the active alerts, nothing more is sent until \
             it resolves. If it comes back later, it alerts again. Undo with undo true.",
            alert.rule_name
        )
    };
    Ok(ToolOutput::new(text, alert_json(&alert, &names)))
}
