//! Équipements : état, lecture (`list_devices`, `get_device`) et actions (sonde, activation, ajout, découverte, redémarrage).

use super::*;

/// État d'un équipement, avec les mots de l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeviceState {
    Reporting,
    Unreachable,
    Waiting,
    Disabled,
    Down,
    Unknown,
}

impl DeviceState {
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Reporting => "reporting",
            Self::Unreachable => "unreachable",
            Self::Waiting => "waiting",
            Self::Disabled => "disabled",
            Self::Down => "down",
            Self::Unknown => "unknown",
        }
    }

    pub(super) fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "reporting" | "online" | "up" => Some(Self::Reporting),
            "unreachable" | "offline" => Some(Self::Unreachable),
            "waiting" | "pending" => Some(Self::Waiting),
            "disabled" => Some(Self::Disabled),
            "down" => Some(Self::Down),
            "unknown" => Some(Self::Unknown),
            _ => None,
        }
    }

    pub(super) fn is_trouble(self) -> bool {
        matches!(self, Self::Unreachable | Self::Down)
    }
}

pub(super) struct Device {
    pub(super) target: Target,
    pub(super) state: DeviceState,
    pub(super) last_probe_at: Option<String>,
    pub(super) last_error: Option<String>,
    /// Motif du dernier échec d'une sonde de service, quand il est connu.
    pub(super) probe_reason: Option<String>,
}

impl Device {
    pub(super) fn id(&self) -> TargetId {
        self.target.id
    }

    pub(super) fn name(&self) -> &str {
        &self.target.name
    }

    /// Comme pour un lecteur de l'API : un assistant ne reçoit jamais les
    /// options qui ressemblent à des secrets, ni le mot de passe d'une URL.
    pub(super) fn summary(&self, parents: &HashMap<TargetId, String>) -> Value {
        json!({
            "id": self.target.id,
            "name": self.target.name,
            "kind": self.target.kind,
            "address": targets::redact_userinfo(&self.target.address),
            "state": self.state.word(),
            "enabled": self.target.enabled,
            "parent_id": self.target.parent_id,
            "parent": self.target.parent_id.and_then(|id| parents.get(&id)),
            "tags": targets::reader_tags(&self.target.tags),
            "interval_secs": self.target.interval.as_secs(),
            "last_probe_at": self.last_probe_at,
            "last_error": self.last_error,
        })
    }

    pub(super) fn line(&self, parents: &HashMap<TargetId, String>) -> String {
        let mut line = format!(
            "- **{}** (#{}, {}, {}) — {}",
            self.target.name,
            self.target.id,
            self.target.kind,
            targets::redact_userinfo(&self.target.address),
            self.state.word()
        );
        if let Some(detail) = self.detail() {
            line.push_str(&format!(": {detail}"));
        }
        if let Some(at) = &self.last_probe_at {
            line.push_str(&format!("; last probe {at}"));
        }
        if let Some(parent) = self.target.parent_id.and_then(|id| parents.get(&id)) {
            line.push_str(&format!("; parent: {parent}"));
        }
        if !self.target.tags.is_empty() {
            line.push_str(&format!(
                "; tags: {}",
                format_tags(&targets::reader_tags(&self.target.tags))
            ));
        }
        line
    }

    /// Ce qui ne va pas, quand quelque chose ne va pas.
    pub(super) fn detail(&self) -> Option<&str> {
        self.last_error.as_deref().or(self.probe_reason.as_deref())
    }
}

/// Même déduction que l'interface (`targetState`) : désactivé, erreur, jamais
/// interrogé, ou interrogation trop ancienne — plus de trois périodes, et jamais
/// moins de 90 secondes de tolérance.
pub(super) fn classic_state(
    target: &Target,
    last_probe_at: Option<&str>,
    last_error: Option<&str>,
    now: DateTime<Utc>,
) -> DeviceState {
    if !target.enabled {
        return DeviceState::Disabled;
    }
    if last_error.is_some() {
        return DeviceState::Unreachable;
    }
    let Some(last) = last_probe_at.and_then(|raw| DateTime::parse_from_rfc3339(raw).ok()) else {
        return DeviceState::Waiting;
    };
    let tolerance = TimeDelta::seconds((target.interval.as_secs() * 3).max(90) as i64);
    if now - last.with_timezone(&Utc) > tolerance {
        DeviceState::Unreachable
    } else {
        DeviceState::Reporting
    }
}

/// Dernier résultat des sondes de service, par identifiant de cible.
///
/// Sans VictoriaMetrics, on retombe sur la déduction classique plutôt que
/// d'échouer : l'assistant a une réponse, un peu moins précise.
async fn probe_results(state: &AppState) -> HashMap<TargetId, (bool, Option<String>)> {
    let mut out = HashMap::new();
    let Ok(series) = state
        .victoria
        .query(&format!("last_over_time(dumbmonit_probe_success[{PROBE_WINDOW}])"))
        .await
    else {
        return out;
    };
    for serie in series {
        let Some(id) = serie.metric.get("target").and_then(|raw| raw.parse::<TargetId>().ok())
        else {
            continue;
        };
        let up = serie.value.1.parse::<f64>().map(|v| v >= 1.0).unwrap_or(false);
        out.insert(id, (up, None));
    }
    if out.values().any(|(up, _)| !*up)
        && let Ok(series) = state
            .victoria
            .query(&format!("tlast_over_time(dumbmonit_probe_failure_info[{PROBE_WINDOW}])"))
            .await
    {
        for serie in series {
            let Some(id) = serie.metric.get("target").and_then(|raw| raw.parse::<TargetId>().ok())
            else {
                continue;
            };
            if let Some((false, reason)) = out.get_mut(&id) {
                *reason = serie.metric.get("reason").cloned();
            }
        }
    }
    out
}

pub(super) async fn load_devices(state: &AppState) -> Result<Vec<Device>, ToolError> {
    let targets = db::targets::list(&state.pool, &state.cipher).await?;
    let mut statuses = db::targets::statuses(&state.pool).await?;
    let has_services = targets.iter().any(|t| UPTIME_KINDS.contains(&t.kind.as_str()));
    let probes = if has_services { probe_results(state).await } else { HashMap::new() };
    let now = Utc::now();

    Ok(targets
        .into_iter()
        .map(|target| {
            let status = statuses.remove(&target.id);
            let last_probe_at = status.as_ref().and_then(|s| s.last_probe_at.clone());
            let last_error = status.as_ref().and_then(|s| s.last_error.clone());
            let classic =
                classic_state(&target, last_probe_at.as_deref(), last_error.as_deref(), now);
            let mut probe_reason = None;
            let state = if UPTIME_KINDS.contains(&target.kind.as_str()) && target.enabled {
                // Pour un service, `last_error` ne parle que de configuration ; la
                // vérité est dans le résultat de la sonde.
                match (last_error.is_some(), probes.get(&target.id)) {
                    (true, _) => DeviceState::Unreachable,
                    (false, Some((true, _))) => DeviceState::Reporting,
                    (false, Some((false, reason))) => {
                        probe_reason = reason.clone();
                        DeviceState::Down
                    }
                    // Un heartbeat sans verdict n'a pas encore été appelé : il attend.
                    (false, None) if classic == DeviceState::Waiting || target.kind == "push" => {
                        DeviceState::Waiting
                    }
                    (false, None) => DeviceState::Unknown,
                }
            } else {
                classic
            };
            Device { target, state, last_probe_at, last_error, probe_reason }
        })
        .collect())
}

pub(super) fn parent_names(devices: &[Device]) -> HashMap<TargetId, String> {
    devices.iter().map(|d| (d.id(), d.name().to_string())).collect()
}

/// Retrouve un équipement par identifiant ou par nom.
///
/// Le nom est comparé sans la casse, d'abord exactement, puis comme sous-chaîne
/// unique : « the nas » doit suffire quand un seul équipement s'appelle « NAS ».
pub(super) fn resolve<'a>(devices: &'a [Device], reference: &str) -> Result<&'a Device, ToolError> {
    if let Ok(id) = reference.parse::<TargetId>()
        && let Some(device) = devices.iter().find(|d| d.id() == id)
    {
        return Ok(device);
    }
    let wanted = reference.trim().to_lowercase();
    if let Some(device) = devices.iter().find(|d| d.name().to_lowercase() == wanted) {
        return Ok(device);
    }
    let partial: Vec<&Device> =
        devices.iter().filter(|d| d.name().to_lowercase().contains(&wanted)).collect();
    match partial.as_slice() {
        [device] => Ok(device),
        [] => Err(failed(format!(
            "No device matches \"{reference}\". Known devices: {}.",
            names(devices)
        ))),
        many => Err(failed(format!(
            "\"{reference}\" is ambiguous: {}. Use the id.",
            many.iter()
                .map(|d| format!("{} (#{})", d.name(), d.id()))
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

pub(super) fn names(devices: &[Device]) -> String {
    if devices.is_empty() {
        return "none yet".to_string();
    }
    devices.iter().map(|d| format!("{} (#{})", d.name(), d.id())).collect::<Vec<_>>().join(", ")
}

pub(super) fn resolve_arg<'a>(devices: &'a [Device], args: &Args) -> Result<&'a Device, ToolError> {
    let reference =
        args.device_ref().ok_or_else(|| failed("Name the device: \"id\" or \"name\"."))?;
    resolve(devices, &reference)
}

pub(super) async fn list_devices(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let parents = parent_names(&devices);
    let filter = args.str("filter").map(str::to_lowercase);
    let wanted = match args.str("state") {
        None => None,
        Some(raw) => Some(DeviceState::parse(raw).ok_or_else(|| {
            failed(format!(
                "Unknown state \"{raw}\" (expected: reporting, unreachable, waiting, disabled, down, unknown)."
            ))
        })?),
    };

    let selected: Vec<&Device> = devices
        .iter()
        .filter(|d| wanted.is_none_or(|w| d.state == w))
        .filter(|d| {
            filter.as_deref().is_none_or(|needle| {
                d.name().to_lowercase().contains(needle)
                    || d.target.address.to_lowercase().contains(needle)
                    || d.target.kind.to_lowercase().contains(needle)
                    || format_tags(&d.target.tags).to_lowercase().contains(needle)
            })
        })
        .collect();

    let text = if selected.is_empty() {
        if devices.is_empty() {
            "No device is monitored yet.".to_string()
        } else {
            format!("No device matches. Known devices: {}.", names(&devices))
        }
    } else {
        let mut lines = vec![format!("{} of {} devices:", selected.len(), devices.len())];
        lines.extend(selected.iter().map(|d| d.line(&parents)));
        lines.join("\n")
    };

    Ok(ToolOutput::new(
        text,
        json!({ "devices": selected.iter().map(|d| d.summary(&parents)).collect::<Vec<_>>() }),
    ))
}

/// Une valeur instantanée lue dans VictoriaMetrics, ou rien.
async fn instant_value(state: &AppState, query: &str) -> Option<f64> {
    let series = state.victoria.query(query).await.ok()?;
    series
        .iter()
        .filter_map(|s| s.value.1.parse::<f64>().ok())
        .fold(None, |acc, v| Some(acc.map_or(v, |a: f64| a.max(v))))
}

/// Résumé 24 h des métriques clés d'un équipement.
///
/// Chaque collecteur nomme ses séries : les unions `or` couvrent SNMP
/// (HOST-RESOURCES), l'agent, Proxmox, Synology et PBS. Ce qui n'existe pas est
/// simplement absent du résumé.
async fn metrics_summary(state: &AppState, device: &Device) -> Vec<(String, f64, f64)> {
    let sel = format!("target=\"{}\"", device.id());
    let expressions: Vec<(&str, String)> = if UPTIME_KINDS.contains(&device.target.kind.as_str()) {
        vec![
            ("availability_percent", format!("dumbmonit_probe_success{{{sel}}} * 100")),
            ("latency_seconds", format!("dumbmonit_probe_duration_seconds{{{sel}}}")),
        ]
    } else {
        vec![
            (
                "cpu_percent",
                format!(
                    "max by (target) (dumbmonit_cpu_load_percent{{{sel}}} or \
                     dumbmonit_cpu_usage_percent{{{sel}}} or \
                     dumbmonit_proxmox_node_cpu_percent{{{sel}}})"
                ),
            ),
            (
                "memory_percent",
                format!(
                    "max by (target) (dumbmonit_memory_used_percent{{{sel}}} or \
                     dumbmonit_synology_memory_usage_percent{{{sel}}} or \
                     dumbmonit_pbs_node_memory_used_percent{{{sel}}} or \
                     100 * dumbmonit_memory_bytes_used{{{sel}}} / dumbmonit_memory_bytes_total{{{sel}}} or \
                     100 * dumbmonit_proxmox_node_memory_used_bytes{{{sel}}} / dumbmonit_proxmox_node_memory_total_bytes{{{sel}}})"
                ),
            ),
            (
                "disk_fullest_percent",
                format!(
                    "max by (target) (100 * dumbmonit_storage_bytes_used{{{sel}}} / dumbmonit_storage_bytes_total{{{sel}}} or \
                     dumbmonit_proxmox_storage_used_percent{{{sel}}} or \
                     dumbmonit_proxmox_node_rootfs_percent{{{sel}}} or \
                     dumbmonit_pbs_datastore_used_percent{{{sel}}})"
                ),
            ),
        ]
    };

    let mut out = Vec::new();
    for (name, expr) in expressions {
        let max = instant_value(state, &format!("max_over_time(({expr})[24h])")).await;
        let avg = instant_value(state, &format!("avg_over_time(({expr})[24h])")).await;
        if let (Some(max), Some(avg)) = (max, avg) {
            out.push((name.to_string(), max, avg));
        }
    }
    out
}

pub(super) async fn get_device(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let device = resolve_arg(&devices, args)?;
    let parents = parent_names(&devices);
    let alerts = active_alerts(state).await?;
    let own: Vec<&ActiveAlertView> =
        alerts.iter().filter(|a| a.target_id == Some(device.id())).collect();
    let summary = metrics_summary(state, device).await;

    let mut text = device.line(&parents);
    text.push_str(&format!(
        "\nPolled every {} s; profile: {}; credential: {}.",
        device.target.interval.as_secs(),
        device.target.profile_id.as_deref().unwrap_or("not detected yet"),
        device.target.credential.kind_label()
    ));
    if summary.is_empty() {
        text.push_str("\n\nLast 24 h: no CPU/memory/disk series available for this device.");
    } else {
        text.push_str("\n\nLast 24 h:");
        for (name, max, avg) in &summary {
            text.push_str(&format!(
                "\n- {name}: max {}, average {}",
                trim_float(*max),
                trim_float(*avg)
            ));
        }
    }
    if own.is_empty() {
        text.push_str("\n\nNo active alert on this device.");
    } else {
        text.push_str(&format!("\n\n{}:", plural(own.len(), "active alert", "active alerts")));
        for alert in &own {
            text.push('\n');
            text.push_str(&alert_line(alert, &parents));
        }
    }

    let mut structured = device.summary(&parents);
    structured["profile_id"] = json!(device.target.profile_id);
    structured["metrics_24h"] = summary
        .iter()
        .map(|(name, max, avg)| (name.clone(), json!({ "max": max, "avg": avg })))
        .collect::<serde_json::Map<_, _>>()
        .into();
    structured["alerts"] = own.iter().map(|a| alert_json(a, &parents)).collect::<Vec<_>>().into();
    Ok(ToolOutput::new(text, structured))
}

pub(super) async fn probe_device(state: &AppState, args: &Args) -> ToolResult {
    let devices = load_devices(state).await?;
    let device = resolve_arg(&devices, args)?;
    match targets::probe_now(State(state.clone()), Path(device.id())).await {
        Ok(Json(report)) => {
            let shown: Vec<&str> = report.series.iter().take(25).map(String::as_str).collect();
            let more = report.series.len().saturating_sub(shown.len());
            let mut text = format!(
                "{} (#{}) answered: {} samples across {} series.",
                device.name(),
                device.id(),
                report.sample_count,
                report.series.len()
            );
            if !shown.is_empty() {
                text.push_str(&format!("\nSeries: {}", shown.join(", ")));
                if more > 0 {
                    text.push_str(&format!(" … and {more} more"));
                }
            }
            Ok(ToolOutput::new(
                text,
                json!({ "device_id": device.id(), "device": device.name(), "ok": true, "sample_count": report.sample_count, "series": report.series }),
            ))
        }
        Err(ApiError::BadRequest(message)) => {
            Err(failed(format!("Probe of {} (#{}) failed: {message}", device.name(), device.id())))
        }
        Err(other) => Err(other.into()),
    }
}

pub(super) async fn set_device_enabled(state: &AppState, args: &Args) -> ToolResult {
    let enabled = args.required_bool("enabled")?;
    let devices = load_devices(state).await?;
    let device = resolve_arg(&devices, args)?;
    db::targets::set_enabled(&state.pool, device.id(), enabled).await?;
    Ok(ToolOutput::new(
        format!(
            "{} (#{}) is now {}.",
            device.name(),
            device.id(),
            if enabled {
                "enabled: it will be polled at its next interval"
            } else {
                "disabled: no polling, no alerts"
            }
        ),
        json!({ "device_id": device.id(), "device": device.name(), "enabled": enabled }),
    ))
}

/// Valeur d'option en texte : un modèle envoie volontiers `161` ou `true`.
pub(super) fn option_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

pub(super) async fn add_device(state: &AppState, args: &Args) -> ToolResult {
    let name = args.str("name").ok_or_else(|| failed("\"name\" is required."))?;
    let kind = args.str("kind").ok_or_else(|| {
        failed("\"kind\" is required: call list_device_types to see the possible values.")
    })?;
    let address = args.str("address").ok_or_else(|| failed("\"address\" is required."))?;

    let devices = load_devices(state).await?;
    let reference = |key: &str| -> Result<Option<TargetId>, ToolError> {
        match args.0.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Number(n)) => n
                .as_i64()
                .map(|id| resolve(&devices, &id.to_string()).map(|d| Some(d.id())))
                .unwrap_or_else(|| Err(failed(format!("\"{key}\" must be a device id or name.")))),
            Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
            Some(Value::String(text)) => resolve(&devices, text).map(|d| Some(d.id())),
            Some(_) => Err(failed(format!("\"{key}\" must be a device id or name."))),
        }
    };
    let parent_id = reference("parent")?;
    let via_agent = reference("via_agent")?;

    let mut tags = BTreeMap::new();
    for key in ["tags", "options"] {
        match args.0.get(key) {
            None | Some(Value::Null) => {}
            Some(Value::Object(map)) => {
                for (k, v) in map {
                    let text = option_text(v).ok_or_else(|| {
                        failed(format!("\"{key}.{k}\" must be a string, a number or a boolean."))
                    })?;
                    tags.insert(k.clone(), text);
                }
            }
            Some(_) => return Err(failed(format!("\"{key}\" must be an object."))),
        }
    }

    let mut payload = json!({ "name": name, "address": address, "kind": kind, "tags": tags });
    if let Some(interval) = args.i64("interval_secs") {
        payload["interval_secs"] = json!(interval.max(0));
    }
    if let Some(parent) = parent_id {
        payload["parent_id"] = json!(parent);
    }
    if let Some(agent) = via_agent {
        payload["via_agent"] = json!(agent);
    }
    if let Some(enabled) = args.bool("enabled") {
        payload["enabled"] = json!(enabled);
    }
    match args.0.get("credential") {
        None | Some(Value::Null) => {}
        Some(credential @ Value::Object(_)) => payload["credential"] = credential.clone(),
        Some(_) => return Err(failed("\"credential\" must be an object with a \"type\".")),
    }
    // La désérialisation est celle de `POST /api/targets` : mêmes règles, mêmes
    // messages. Son texte d'erreur nomme un champ ou un type, jamais une valeur.
    let payload: targets::TargetPayload = serde_json::from_value(payload).map_err(|error| {
        failed(format!(
            "Invalid device: {error}. Call list_device_types with kind \"{kind}\" for the expected \
             shape."
        ))
    })?;

    let (_, Json(view)) = targets::create(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Json(crate::api::targets_bulk::WithChannels { target: payload, channels: None }),
    )
    .await?;
    let mut text = format!(
        "Added **{}** (#{}, {}, {}); credential: {}. It will be polled every {} s",
        view.name, view.id, view.kind, view.address, view.credential_kind, view.interval_secs
    );
    if let Some(agent) = view.via_agent.and_then(|id| devices.iter().find(|d| d.id() == id)) {
        text.push_str(&format!(" through the relay agent {}", agent.name()));
    }
    text.push_str("; call probe_device to check it right now.");
    let structured = serde_json::to_value(&view).map_err(anyhow::Error::from)?;
    Ok(ToolOutput::new(text, structured))
}

pub(super) async fn discover_network(token: &ApiToken, args: &Args) -> ToolResult {
    let cidr =
        args.str("cidr").ok_or_else(|| failed("\"cidr\" is required (e.g. 192.168.1.0/24)."))?;
    let port = match args.i64("port") {
        None => None,
        Some(port) => {
            Some(u16::try_from(port).map_err(|_| failed("\"port\" must be between 1 and 65535."))?)
        }
    };
    let request = discovery::ScanRequest {
        cidr: cidr.to_string(),
        community: args.str("community").map(str::to_string),
        port,
        timeout_ms: args.i64("timeout_ms").map(|ms| ms.max(0) as u64),
    };
    let identity = AdminIdentity(Principal::Token(token.clone()));
    let Json(report) = discovery::scan(identity, Json(request)).await?;

    let text = if report.devices.is_empty() {
        format!("Scanned {} addresses in {cidr}: no device answered SNMP.", report.scanned)
    } else {
        let mut lines = vec![format!(
            "Scanned {} addresses in {cidr}: {} answered SNMP.",
            report.scanned,
            plural(report.devices.len(), "device", "devices")
        )];
        for d in &report.devices {
            lines.push(format!(
                "- {} — {}{}",
                d.address,
                d.sysname.as_deref().unwrap_or("(no name)"),
                d.suggested_profile
                    .as_deref()
                    .map(|p| format!(" (profile {p})"))
                    .unwrap_or_default()
            ));
        }
        lines.push("Use add_device with kind snmp to monitor one of them.".into());
        lines.join("\n")
    };
    let structured = serde_json::to_value(&report).map_err(anyhow::Error::from)?;
    Ok(ToolOutput::new(text, structured))
}

pub(super) async fn restart_container(
    state: &AppState,
    token: &ApiToken,
    args: &Args,
) -> ToolResult {
    let container = args
        .str("container")
        .ok_or_else(|| failed("\"container\" (see list_containers) is required."))?
        .to_string();
    let devices = load_devices(state).await?;
    let device = resolve_agent(&devices, args)?;
    let who = Some(Extension(CurrentPrincipal(Principal::Token(token.clone()))));
    let (_, Json(command)) =
        agent_commands::restart(State(state.clone()), Path((device.id(), container.clone())), who)
            .await?;
    Ok(ToolOutput::new(
        format!(
            "Restart of container {container} on {} (#{}) queued as command #{}. The agent picks \
             it up within seconds; list_containers shows the outcome.",
            device.name(),
            device.id(),
            command.id
        ),
        json!({
            "command_id": command.id,
            "device_id": device.id(),
            "device": device.name(),
            "container": container,
            "status": command.status,
        }),
    ))
}
