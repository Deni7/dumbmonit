//! Silences et fenêtres de maintenance.

use super::*;

fn schedule_text(schedule: &Schedule) -> String {
    match schedule {
        Schedule::Once { starts_at, ends_at } => {
            format!("once, {} → {}", rfc3339(*starts_at), rfc3339(*ends_at))
        }
        Schedule::Recovery { starts_at } => {
            format!("until the device is reachable again, from {}", rfc3339(*starts_at))
        }
        Schedule::Weekly { days, start_minute, end_minute, utc_offset_minutes, timezone } => {
            let days: Vec<&str> =
                days.iter().filter_map(|d| WEEKDAYS.get(*d as usize).copied()).collect();
            format!(
                "weekly on {}, {:02}:{:02} → {:02}:{:02} ({})",
                days.join("/"),
                start_minute / 60,
                start_minute % 60,
                end_minute / 60,
                end_minute % 60,
                zone_text(timezone.as_deref(), *utc_offset_minutes)
            )
        }
        Schedule::Monthly {
            days,
            nth_weekdays,
            start_minute,
            duration_minutes,
            utc_offset_minutes,
            timezone,
        } => {
            let mut when: Vec<String> = days.iter().map(|day| format!("day {day}")).collect();
            for nth in nth_weekdays {
                let name = WEEKDAYS.get(nth.weekday as usize).copied().unwrap_or("?");
                let rank = if nth.nth < 0 { "last".to_string() } else { format!("#{}", nth.nth) };
                when.push(format!("{rank} {name}"));
            }
            format!(
                "monthly on {}, {:02}:{:02} for {} min ({})",
                when.join("/"),
                start_minute / 60,
                start_minute % 60,
                duration_minutes,
                zone_text(timezone.as_deref(), *utc_offset_minutes)
            )
        }
    }
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/// Fuseau d'une fenêtre récurrente, tel qu'on le dit à un assistant.
fn zone_text(timezone: Option<&str>, utc_offset_minutes: i32) -> String {
    match timezone.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => name.to_string(),
        None => format!("UTC{:+}", utc_offset_minutes / 60),
    }
}

pub(super) async fn list_silences(state: &AppState) -> ToolResult {
    let devices = load_devices(state).await?;
    let parents = parent_names(&devices);
    let Json(silences) = alerts::list_silences(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;

    let text = if silences.is_empty() {
        "No maintenance window.".to_string()
    } else {
        let mut lines = vec![format!(
            "{}:",
            plural(silences.len(), "maintenance window", "maintenance windows")
        )];
        for s in &silences {
            let scope = match s.target_id {
                Some(id) => parents.get(&id).cloned().unwrap_or_else(|| format!("device #{id}")),
                None if s.matchers.is_empty() => "whole instance".to_string(),
                None => format!("labels {}", format_tags(&s.matchers)),
            };
            lines.push(format!(
                "- #{} {} — {}; {}; {}{}{}",
                s.id,
                s.name,
                scope,
                schedule_text(&s.schedule),
                if s.active_now {
                    "active now"
                } else if s.enabled {
                    "not active now"
                } else {
                    "disabled"
                },
                if s.comment.is_empty() { String::new() } else { format!("; \"{}\"", s.comment) },
                ""
            ));
        }
        lines.join("\n")
    };
    let structured: Vec<Value> = silences
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "comment": s.comment,
                "device_id": s.target_id,
                "device": s.target_id.and_then(|id| parents.get(&id)),
                "matchers": s.matchers,
                "schedule": s.schedule,
                "enabled": s.enabled,
                "active_now": s.active_now,
            })
        })
        .collect();
    Ok(ToolOutput::new(text, json!({ "silences": structured })))
}

pub(super) async fn silence_device(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let device = resolve_arg(&devices, args)?;
    let hours = args.f64("hours").unwrap_or(1.0);
    if !hours.is_finite() || hours <= 0.0 {
        return Err(failed("\"hours\" must be a positive number."));
    }
    if hours > MAX_SILENCE_HOURS {
        return Err(failed(format!(
            "\"hours\" is limited to {MAX_SILENCE_HOURS} (one week). To silence a device for \
             longer, disable it or its rules instead."
        )));
    }
    let comment = args.str("comment").unwrap_or("Silenced via the assistant").to_string();
    let starts_at = Utc::now();
    let ends_at = starts_at + TimeDelta::milliseconds((hours * 3_600_000.0) as i64);

    let (_, Json(silence)) = alerts::create_silence(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Json(SilencePayload {
            name: format!("{} — silenced by assistant", device.name()),
            comment: Some(comment),
            target_id: Some(device.id()),
            matchers: BTreeMap::new(),
            schedule: json!({ "kind": "once", "starts_at": rfc3339(starts_at), "ends_at": rfc3339(ends_at) }),
            enabled: Some(true),
        }),
    )
    .await?;

    Ok(ToolOutput::new(
        format!(
            "Silence #{} created: alerts on {} (#{}) are muted until {} ({} h). Remove it \
             earlier with remove_silence.",
            silence.id,
            device.name(),
            device.id(),
            rfc3339(ends_at),
            trim_float(hours)
        ),
        json!({ "id": silence.id, "device_id": device.id(), "device": device.name(), "starts_at": rfc3339(starts_at), "ends_at": rfc3339(ends_at) }),
    ))
}

pub(super) async fn remove_silence(state: &AppState, args: &Args) -> ToolResult {
    let id = args.i64("id").ok_or_else(|| failed("\"id\" (silence id) is required."))?;
    alerts::delete_silence(State(state.clone()), Path(id)).await?;
    Ok(ToolOutput::new(
        format!("Silence #{id} removed. Alerts it covered notify again."),
        json!({ "id": id, "removed": true }),
    ))
}

pub(super) async fn schedule_maintenance(state: &AppState, args: &Args) -> ToolResult {
    let parse = |key: &str| -> Result<Option<DateTime<Utc>>, ToolError> {
        match args.str(key) {
            None => Ok(None),
            Some(raw) => DateTime::parse_from_rfc3339(raw)
                .map(|at| Some(at.with_timezone(&Utc)))
                .map_err(|_| {
                    failed(format!(
                        "\"{key}\" must be an RFC 3339 time, e.g. 2026-10-04T22:00:00Z."
                    ))
                }),
        }
    };
    let starts_at = parse("starts_at")?.ok_or_else(|| failed("\"starts_at\" is required."))?;
    let ends_at = match parse("ends_at")? {
        Some(end) => end,
        None => {
            let hours = args.f64("hours").unwrap_or(1.0);
            if !hours.is_finite() || hours <= 0.0 {
                return Err(failed("\"hours\" must be a positive number."));
            }
            starts_at + TimeDelta::milliseconds((hours * 3_600_000.0) as i64)
        }
    };
    if ends_at <= starts_at {
        return Err(failed("The window must end after it starts."));
    }
    if ends_at - starts_at > TimeDelta::milliseconds((MAX_SILENCE_HOURS * 3_600_000.0) as i64) {
        return Err(failed(
            "A maintenance window planned by an assistant lasts 7 days at most. For a recurring \
             window, use the web interface.",
        ));
    }
    if ends_at <= Utc::now() {
        return Err(failed("That window is already over."));
    }

    let devices = load_devices(state).await?;
    let device = match args.device_ref() {
        Some(reference) => Some(resolve(&devices, &reference)?),
        None => None,
    };
    let scope = device.map_or_else(|| "whole instance".to_string(), |d| d.name().to_string());
    let name =
        args.str("name").map(str::to_string).unwrap_or_else(|| format!("Maintenance — {scope}"));

    let (_, Json(silence)) = alerts::create_silence(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Json(SilencePayload {
            name: name.clone(),
            comment: Some(args.str("comment").unwrap_or("Planned via the assistant").to_string()),
            target_id: device.map(Device::id),
            matchers: BTreeMap::new(),
            schedule: json!({ "kind": "once", "starts_at": rfc3339(starts_at), "ends_at": rfc3339(ends_at) }),
            enabled: Some(true),
        }),
    )
    .await?;

    Ok(ToolOutput::new(
        format!(
            "Maintenance window #{} \"{name}\" planned for {scope}: {} → {}{}. Remove it with \
             remove_silence.",
            silence.id,
            rfc3339(starts_at),
            rfc3339(ends_at),
            if silence.active_now { " (active now)" } else { "" }
        ),
        json!({
            "id": silence.id,
            "name": name,
            "device_id": device.map(Device::id),
            "device": device.map(|d| d.name().to_string()),
            "starts_at": rfc3339(starts_at),
            "ends_at": rfc3339(ends_at),
            "active_now": silence.active_now,
        }),
    ))
}
