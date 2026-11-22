//! Inventaire : types d'équipement, agents, conteneurs, heartbeats, paquets.

use super::*;

pub(super) async fn list_device_types(state: &AppState, args: &Args) -> ToolResult {
    let Json(kinds) = collectors::list(State(state.clone())).await;
    if let Some(wanted) = args.str("kind") {
        let Some(kind) = kinds.iter().find(|k| k.kind.eq_ignore_ascii_case(wanted)) else {
            return Err(failed(format!(
                "Unknown device type \"{wanted}\". Known types: {}.",
                kinds.iter().map(|k| &*k.kind).collect::<Vec<&str>>().join(", ")
            )));
        };
        let mut text = format!("**{}** (`{}`) — {}", kind.label, kind.kind, kind.summary);
        if !kind.address_hint.is_empty() {
            text.push_str(&format!("\nAddress, e.g.: {}", kind.address_hint));
            if kind.default_port > 0 {
                text.push_str(&format!(" (default port {})", kind.default_port));
            }
        }
        if kind.credentials.is_empty() {
            text.push_str("\nCredential: none.");
        } else {
            text.push_str("\nCredential (the `credential` object of add_device):");
            for credential in &kind.credentials {
                let fields: Vec<String> = credential
                    .fields
                    .iter()
                    .map(|f| {
                        let mut field = format!("{}{}", f.key, if f.required { "" } else { "?" });
                        if !f.choices.is_empty() {
                            field.push_str(&format!(
                                " ({})",
                                f.choices
                                    .iter()
                                    .map(AsRef::as_ref)
                                    .collect::<Vec<&str>>()
                                    .join("|")
                            ));
                        }
                        field
                    })
                    .collect();
                text.push_str(&format!(
                    "\n- type `{}` — {}{}",
                    credential.kind,
                    credential.label,
                    if fields.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", fields.join(", "))
                    }
                ));
            }
        }
        if !kind.options.is_empty() {
            text.push_str("\nOptions (the `options` object of add_device):");
            for option in &kind.options {
                let mut line = format!("\n- `{}` — {}", option.key, option.label);
                if option.required {
                    line.push_str(" (required)");
                }
                if !option.default.is_empty() {
                    line.push_str(&format!("; default {}", option.default));
                }
                if !option.choices.is_empty() {
                    line.push_str(&format!(
                        "; one of {}",
                        option.choices.iter().map(AsRef::as_ref).collect::<Vec<&str>>().join(", ")
                    ));
                }
                if !option.help.is_empty() {
                    line.push_str(&format!(". {}", option.help));
                }
                text.push_str(&line);
            }
        }
        if !kind.setup.steps.is_empty() {
            text.push_str(&format!("\n{}:", kind.setup.title));
            for (index, step) in kind.setup.steps.iter().enumerate() {
                text.push_str(&format!("\n{}. {}", index + 1, step.replace('\n', " — ")));
            }
        }
        if !kind.setup.ports.is_empty() {
            text.push_str("\nNetwork ports to open:");
            for port in &kind.setup.ports {
                let way = if port.direction == "to_server" {
                    "device/agent to DumbMonit"
                } else {
                    "DumbMonit to device"
                };
                let optional = if port.optional { ", optional" } else { "" };
                text.push_str(&format!(
                    "\n- {}/{} ({way}{optional}): {}",
                    port.port, port.protocol, port.purpose
                ));
            }
        }
        if !kind.setup.permissions.is_empty() {
            text.push_str("\nRequired permissions:");
            for permission in &kind.setup.permissions {
                text.push_str(&format!("\n- {}", permission.replace('\n', " — ")));
            }
        }
        if !kind.setup.warning.is_empty() {
            text.push_str(&format!("\nNote: {}", kind.setup.warning));
        }
        let described = serde_json::to_value(kind).map_err(anyhow::Error::from)?;
        return Ok(ToolOutput::new(text, json!({ "kinds": [described] })));
    }

    let mut lines = vec![format!("{} device types:", kinds.len())];
    let mut structured = Vec::new();
    for kind in &kinds {
        let credentials: Vec<&str> = kind.credential_types.iter().map(AsRef::as_ref).collect();
        lines.push(format!(
            "- `{}` — {}: {}{}",
            kind.kind,
            kind.label,
            kind.summary,
            if credentials.is_empty() || credentials == ["none"] {
                String::new()
            } else {
                format!(" (credential: {})", credentials.join(" or "))
            }
        ));
        structured.push(json!({
            "kind": kind.kind,
            "label": kind.label,
            "summary": kind.summary,
            "credential_types": credentials,
            "options": kind.options.iter().map(|o| o.key.as_ref()).collect::<Vec<&str>>(),
        }));
    }
    lines
        .push("Call list_device_types with a kind for its address, credential and options.".into());
    Ok(ToolOutput::new(lines.join("\n"), json!({ "kinds": structured })))
}

pub(super) async fn list_agents(state: &AppState) -> ToolResult {
    let hosts = crate::collectors::agent::list_hosts(&state.pool).await?;
    let counts = db::targets::relayed_counts(&state.pool).await?;
    if hosts.is_empty() {
        return Ok(ToolOutput::new(
            "No agent has reported yet. Install one from Add monitor, Server with the agent."
                .into(),
            json!({ "agents": [] }),
        ));
    }
    let mut lines = vec![format!("{}:", plural(hosts.len(), "agent", "agents"))];
    let mut structured = Vec::new();
    for (id, name, info) in hosts {
        let relayed = counts.get(&id).copied().unwrap_or(0);
        let mut line = format!(
            "- **{name}** (#{id}) — {} on {}{}, agent {}",
            info.hostname,
            info.os,
            info.os_version.as_deref().map(|v| format!(" {v}")).unwrap_or_default(),
            info.agent_version
        );
        if info.relay {
            line.push_str(&format!(
                "; relay{} for {}",
                info.site.as_deref().map(|s| format!(" of site {s}")).unwrap_or_default(),
                plural(relayed, "device", "devices")
            ));
        }
        line.push_str(&format!(
            "; last seen {}; {}{}",
            info.last_seen_at.as_deref().unwrap_or("never"),
            info.binding_state(),
            if info.commands_supported() { "" } else { "; no remote commands" }
        ));
        lines.push(line);
        structured.push(json!({
            "id": id,
            "name": name,
            "hostname": &info.hostname,
            "os": &info.os,
            "os_version": &info.os_version,
            "arch": &info.arch,
            "agent_version": &info.agent_version,
            "relay": info.relay,
            "site": &info.site,
            "relayed": relayed,
            "last_seen_at": &info.last_seen_at,
            "binding": info.binding_state(),
            "commands_supported": info.commands_supported(),
        }));
    }
    Ok(ToolOutput::new(lines.join("\n"), json!({ "agents": structured })))
}

/// Un équipement de type `agent`, ou un échec qui dit lesquels le sont.
pub(super) fn resolve_agent<'a>(devices: &'a [Device], args: &Args) -> Result<&'a Device, ToolError> {
    let device = resolve_arg(devices, args)?;
    if device.target.kind != "agent" {
        let agents: Vec<String> = devices
            .iter()
            .filter(|d| d.target.kind == "agent")
            .map(|d| format!("{} (#{})", d.name(), d.id()))
            .collect();
        return Err(failed(format!(
            "{} is a \"{}\" device, not a machine running the agent. Agents: {}.",
            device.name(),
            device.target.kind,
            if agents.is_empty() { "none yet".to_string() } else { agents.join(", ") }
        )));
    }
    Ok(device)
}

pub(super) async fn list_containers(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let device = resolve_agent(&devices, args)?;
    let Json(containers) =
        agent_commands::list_containers(State(state.clone()), Path(device.id())).await?;
    let text = if containers.is_empty() {
        format!("No container reported by {} (#{}).", device.name(), device.id())
    } else {
        let mut lines = vec![format!(
            "{} on {} (#{}):",
            plural(containers.len(), "container", "containers"),
            device.name(),
            device.id()
        )];
        for c in &containers {
            let mut line = format!(
                "- **{}** ({}) — {}, health {}, {}",
                c.name,
                c.image,
                if c.up { "running" } else { "stopped" },
                c.health,
                plural(c.restart_count as usize, "restart", "restarts")
            );
            if c.update_available == Some(true) {
                line.push_str("; update available");
            }
            if let Some(command) = &c.last_command {
                line.push_str(&format!("; last command {} {}", command.kind, command.status));
            }
            lines.push(line);
        }
        lines.join("\n")
    };
    let containers = serde_json::to_value(&containers).map_err(anyhow::Error::from)?;
    Ok(ToolOutput::new(
        text,
        json!({ "device_id": device.id(), "device": device.name(), "containers": containers }),
    ))
}

fn verdict_word(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Waiting => "waiting",
        Verdict::OnTime => "on_time",
        Verdict::Missed => "missed",
        Verdict::ReportedDown => "reported_down",
    }
}

pub(super) async fn list_heartbeats(state: &AppState) -> ToolResult {
    let devices = load_devices(state).await?;
    let now_ms = Utc::now().timestamp_millis();
    let mut lines = Vec::new();
    let mut structured = Vec::new();
    for device in devices.iter().filter(|d| d.target.kind == push::KIND) {
        // Lecture seule : le jeton n'est ni créé ni déchiffré pour être montré —
        // il ne quitte jamais ce module.
        let monitor = push::store::get(&state.pool, &state.cipher, device.id()).await?;
        let (settings, settings_error) = match push::Settings::from_target(&device.target) {
            Ok(settings) => (Some(settings), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let last_seen_ms = monitor.as_ref().and_then(|m| m.last_seen_ms);
        let verdict = match (&monitor, &settings) {
            (Some(m), Some(settings)) => {
                push::evaluate(m.last_seen_ms, m.last_status, now_ms, settings)
            }
            (Some(m), None) if m.last_seen_ms.is_some() => Verdict::OnTime,
            _ => Verdict::Waiting,
        };
        let age = last_seen_ms.map(|then| push::age(then, now_ms).as_secs());
        let mut line = format!(
            "- **{}** (#{}) — {}",
            device.name(),
            device.id(),
            if device.target.enabled { verdict_word(verdict) } else { "disabled" }
        );
        if let Some(settings) = &settings {
            line.push_str(&format!(
                "; expected every {} s (+{} s grace)",
                settings.expected.as_secs(),
                settings.grace.as_secs()
            ));
        }
        match (&monitor, age) {
            (Some(m), Some(age)) => {
                line.push_str(&format!(
                    "; last call {age} s ago ({}){}",
                    m.last_status.as_str(),
                    if m.last_message.is_empty() {
                        String::new()
                    } else {
                        format!(": \"{}\"", m.last_message)
                    }
                ));
            }
            _ => line.push_str("; never called yet"),
        }
        if let Some(error) = &settings_error {
            line.push_str(&format!("; settings error: {error}"));
        }
        lines.push(line);
        structured.push(json!({
            "device_id": device.id(),
            "device": device.name(),
            "enabled": device.target.enabled,
            "verdict": verdict_word(verdict),
            "expected_interval_secs": settings.as_ref().map(|s| s.expected.as_secs()),
            "grace_secs": settings.as_ref().map(|s| s.grace.as_secs()),
            "last_seen_at": monitor.as_ref().and_then(|m| m.last_seen_at.clone()),
            "last_seen_age_secs": age,
            "last_status": monitor.as_ref().map(|m| m.last_status.as_str()),
            "last_message": monitor.as_ref().map(|m| m.last_message.clone()),
            "received_total": monitor.as_ref().map_or(0, |m| m.received_total),
            "settings_error": settings_error,
        }));
    }
    let text = if lines.is_empty() {
        "No heartbeat monitor. Add a device of kind push to watch a cron job or a backup \
         script."
            .to_string()
    } else {
        format!("{}:\n{}", plural(lines.len(), "heartbeat", "heartbeats"), lines.join("\n"))
    };
    Ok(ToolOutput::new(text, json!({ "heartbeats": structured })))
}

pub(super) async fn list_packs(state: &AppState, token: &ApiToken) -> ToolResult {
    let principal = Extension(CurrentPrincipal(Principal::Token(token.clone())));
    let Json(packs) = packs::list(State(state.clone()), principal).await?;
    let text = if packs.is_empty() {
        "No integration pack installed.".to_string()
    } else {
        let mut lines =
            vec![format!("{}:", plural(packs.len(), "integration pack", "integration packs"))];
        for p in &packs {
            let mut line = format!(
                "- **{}** (`{}` {}) — {}, {}",
                p.label,
                p.id,
                p.version,
                if p.enabled { "enabled" } else { "disabled" },
                plural(p.targets.max(0) as usize, "device", "devices")
            );
            if let Some(kind) = &p.kind {
                line.push_str(&format!("; device kind `{kind}`"));
            }
            if let Some(error) = &p.error {
                line.push_str(&format!("; error: {error}"));
            }
            lines.push(line);
        }
        lines.join("\n")
    };
    let structured: Vec<Value> = packs
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "version": p.version,
                "label": p.label,
                "summary": p.summary,
                "kind": p.kind,
                "enabled": p.enabled,
                "targets": p.targets,
                "rules": p.rules,
                "warnings": p.warnings,
                "error": p.error,
            })
        })
        .collect();
    Ok(ToolOutput::new(text, json!({ "packs": structured })))
}
