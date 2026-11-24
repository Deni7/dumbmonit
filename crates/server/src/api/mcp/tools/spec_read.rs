//! Fiches des outils de lecture.

use super::*;

/// Les outils qui ne changent rien.
pub(super) fn read_specs() -> Vec<ToolSpec> {
    vec![
        // --- Lecture ---------------------------------------------------------
        ToolSpec::read(
            "get_status",
            "Overall status",
            "The one-second answer to \"is everything fine?\": device counts (reporting, \
             unreachable, waiting), firing and building-up alerts, and a one-sentence \
             bulletin. Call this first.",
            no_arguments(),
            object_of(
                json!({
                    "sentence": { "type": "string" },
                    "devices": { "type": "object" },
                    "alerts": { "type": "object" },
                    "unreachable_devices": list_of(device_schema()),
                    "firing": list_of(alert_schema()),
                }),
                &["sentence", "devices", "alerts", "unreachable_devices", "firing"],
            ),
        ),
        ToolSpec::read(
            "list_devices",
            "List devices",
            "Lists monitored devices and services with id, kind, address, state (reporting / \
             unreachable / waiting / disabled / down), parent and tags. Filter by name \
             substring and/or state.",
            json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "Case-insensitive substring matched against name, address, kind and tags." },
                    "state": { "type": "string", "enum": ["reporting", "unreachable", "waiting", "disabled", "down", "unknown"], "description": "Only devices in this state." }
                },
                "additionalProperties": false
            }),
            object_of(json!({ "devices": list_of(device_schema()) }), &["devices"]),
        ),
        ToolSpec::read(
            "get_device",
            "Device details",
            "Details of one device: configuration, state, last probe, active alerts on it, \
             and a 24-hour summary of its key metrics (CPU, memory, fullest disk; \
             availability and latency for a service check).",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Device id." },
                    "name": { "type": "string", "description": "Device name, if the id is unknown." }
                },
                "additionalProperties": false
            }),
            {
                let mut schema = device_schema();
                schema["properties"]["profile_id"] = nullable("string");
                schema["properties"]["metrics_24h"] = json!({ "type": "object" });
                schema["properties"]["alerts"] = list_of(alert_schema());
                schema["required"] =
                    json!(["id", "name", "kind", "state", "metrics_24h", "alerts"]);
                schema
            },
        ),
        ToolSpec::read(
            "list_alerts",
            "List active alerts",
            "Active alerts: severity (info / advisory / warning), rule, device, since when, \
             current value, and whether it is suppressed by an unreachable parent. Alerts \
             still building up (condition true, hold time not elapsed) are included only \
             with include_pending.",
            json!({
                "type": "object",
                "properties": {
                    "include_pending": { "type": "boolean", "default": false, "description": "Also list alerts that are building up." },
                    "device": device_property()
                },
                "additionalProperties": false
            }),
            object_of(json!({ "alerts": list_of(alert_schema()) }), &["alerts"]),
        ),
        ToolSpec::read(
            "alert_history",
            "Alert history",
            "What happened: alert transitions (started firing, resolved…) most recent first. \
             Use it for \"what happened last night?\". Defaults to the last 24 hours.",
            json!({
                "type": "object",
                "properties": {
                    "since": { "type": "string", "description": "Lower bound, RFC 3339 (e.g. 2026-09-14T22:00:00Z). Takes precedence over hours." },
                    "hours": { "type": "number", "description": "Look back this many hours (default 24).", "minimum": 0 },
                    "device": device_property(),
                    "limit": { "type": "integer", "description": "Maximum number of entries (default 50, max 500).", "minimum": 1 }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "since": { "type": "string" },
                    "entries": list_of(object_of(
                        json!({
                            "at": { "type": "string" },
                            "rule_uid": { "type": "string" },
                            "rule": nullable("string"),
                            "device_id": nullable("integer"),
                            "device": nullable("string"),
                            "from": { "type": "string" },
                            "to": { "type": "string" },
                            "severity": { "type": "string" },
                            "value": nullable("number"),
                            "notified": { "type": "boolean" },
                            "reason": { "type": "string" },
                        }),
                        &["at", "rule_uid", "from", "to", "severity", "notified"],
                    )),
                }),
                &["since", "entries"],
            ),
        ),
        ToolSpec::read(
            "query_metrics",
            "Query metrics",
            "Runs a MetricsQL/PromQL range query against the time series (VictoriaMetrics). \
             Metrics are prefixed dumbmonit_ and carry a target label with the device id, \
             e.g. dumbmonit_cpu_usage_percent{target=\"3\"}. Returns at most 60 points per \
             series. Prefer get_device for the usual CPU/memory/disk summary.",
            json!({
                "type": "object",
                "required": ["query"],
                "properties": {
                    "query": { "type": "string", "description": "MetricsQL expression." },
                    "range_hours": { "type": "number", "description": "How far back to look (default 1, max 744).", "minimum": 0 },
                    "step_secs": { "type": "integer", "description": "Sampling step in seconds; widened automatically to stay under 60 points.", "minimum": 1 }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "query": { "type": "string" },
                    "step_secs": { "type": "integer" },
                    "series": list_of(object_of(
                        json!({ "metric": { "type": "object" }, "values": { "type": "array" } }),
                        &["metric", "values"],
                    )),
                }),
                &["query", "series"],
            ),
        ),
        ToolSpec::read(
            "list_silences",
            "List maintenance windows",
            "Lists maintenance windows (silences): id, name, device, schedule, and whether it \
             is active right now.",
            no_arguments(),
            object_of(
                json!({
                    "silences": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "name": { "type": "string" },
                            "comment": { "type": "string" },
                            "device_id": nullable("integer"),
                            "device": nullable("string"),
                            "matchers": { "type": "object" },
                            "schedule": { "type": "object" },
                            "enabled": { "type": "boolean" },
                            "active_now": { "type": "boolean" },
                        }),
                        &["id", "name", "enabled", "active_now"],
                    )),
                }),
                &["silences"],
            ),
        ),
        ToolSpec::read(
            "list_rules",
            "List alert rules",
            "Lists alert rules: uid, name, kind (threshold / anomaly / predict), severity, \
             threshold, hold time, and whether it is enabled or built in.",
            no_arguments(),
            object_of(
                json!({
                    "rules": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "uid": { "type": "string" },
                            "name": { "type": "string" },
                            "kind": { "type": "string" },
                            "severity": { "type": "string" },
                            "enabled": { "type": "boolean" },
                            "builtin": { "type": "boolean" },
                        }),
                        &["id", "uid", "name", "kind", "severity", "enabled", "builtin"],
                    )),
                }),
                &["rules"],
            ),
        ),
        ToolSpec::read(
            "list_device_types",
            "Device types",
            "The kinds of device this server can monitor (SNMP, Proxmox, Synology, agent, \
             HTTP/TCP/DNS/ping/TLS checks, heartbeats, integration packs…). Without \
             arguments: one line per kind. With kind: what to prepare, the address to give, \
             the accepted credentials and their fields, and the options — exactly what \
             add_device expects.",
            json!({
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "description": "A kind from the list, e.g. snmp, proxmox, http." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "kinds": list_of(object_of(
                        json!({
                            "kind": { "type": "string" },
                            "label": { "type": "string" },
                            "summary": { "type": "string" },
                        }),
                        &["kind", "label", "summary"],
                    )),
                }),
                &["kinds"],
            ),
        ),
        ToolSpec::read(
            "list_agents",
            "List agents",
            "Machines running the DumbMonit agent: hostname, OS, agent version, whether it \
             relays probes for a remote site (and how many devices go through it), last \
             contact, binding state and whether it accepts commands.",
            no_arguments(),
            object_of(
                json!({
                    "agents": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "name": { "type": "string" },
                            "hostname": { "type": "string" },
                            "os": { "type": "string" },
                            "agent_version": { "type": "string" },
                            "relay": { "type": "boolean" },
                            "site": nullable("string"),
                            "relayed": { "type": "integer" },
                            "last_seen_at": nullable("string"),
                            "binding": { "type": "string" },
                            "commands_supported": { "type": "boolean" },
                        }),
                        &["id", "name", "hostname", "relay", "relayed", "binding"],
                    )),
                }),
                &["agents"],
            ),
        ),
        ToolSpec::read(
            "list_containers",
            "List containers",
            "Docker containers of a machine that runs the agent: running or not, health, \
             restarts, uptime, whether an image update is available, and the last command \
             sent to each.",
            json!({
                "type": "object",
                "required": ["device"],
                "properties": { "device": device_property() },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "device_id": { "type": "integer" },
                    "device": { "type": "string" },
                    "containers": list_of(object_of(
                        json!({
                            "name": { "type": "string" },
                            "image": { "type": "string" },
                            "up": { "type": "boolean" },
                            "health": { "type": "string" },
                        }),
                        &["name", "image", "up", "health"],
                    )),
                }),
                &["device_id", "device", "containers"],
            ),
        ),
        ToolSpec::read(
            "list_heartbeats",
            "List heartbeats",
            "Heartbeat (push) monitors — jobs that call a secret URL each time they run: \
             expected interval, last call, what it reported, and the verdict (waiting, \
             on_time, missed, reported_down). The secret URL itself is never shown.",
            no_arguments(),
            object_of(
                json!({
                    "heartbeats": list_of(object_of(
                        json!({
                            "device_id": { "type": "integer" },
                            "device": { "type": "string" },
                            "enabled": { "type": "boolean" },
                            "verdict": { "type": "string" },
                            "expected_interval_secs": nullable("integer"),
                            "grace_secs": nullable("integer"),
                            "last_seen_at": nullable("string"),
                            "last_seen_age_secs": nullable("integer"),
                            "last_status": nullable("string"),
                            "last_message": nullable("string"),
                            "received_total": { "type": "integer" },
                            "settings_error": nullable("string"),
                        }),
                        &["device_id", "device", "enabled", "verdict", "received_total"],
                    )),
                }),
                &["heartbeats"],
            ),
        ),
        ToolSpec::read(
            "list_status_pages",
            "List status pages",
            "Status pages (title, public path /s/<slug>, published or draft, how many devices \
             they show) and incidents — open ones first, with their latest update.",
            no_arguments(),
            object_of(
                json!({
                    "pages": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "title": { "type": "string" },
                            "slug": { "type": "string" },
                            "path": { "type": "string" },
                            "published": { "type": "boolean" },
                            "devices": { "type": "integer" },
                        }),
                        &["id", "title", "slug", "path", "published", "devices"],
                    )),
                    "incidents": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "page_id": nullable("integer"),
                            "title": { "type": "string" },
                            "kind": { "type": "string" },
                            "status": { "type": "string" },
                            "severity": { "type": "string" },
                            "open": { "type": "boolean" },
                            "starts_at": { "type": "string" },
                            "ends_at": nullable("string"),
                            "latest_update": nullable("string"),
                        }),
                        &["id", "title", "kind", "status", "severity", "open"],
                    )),
                }),
                &["pages", "incidents"],
            ),
        ),
        ToolSpec::read(
            "list_channels",
            "List notification channels",
            "Notification channels (Discord, ntfy, email…): name, kind, enabled, when a \
             message last left, and the last error. Their secrets are never returned.",
            no_arguments(),
            object_of(
                json!({
                    "channels": list_of(object_of(
                        json!({
                            "id": { "type": "integer" },
                            "name": { "type": "string" },
                            "kind": { "type": "string" },
                            "enabled": { "type": "boolean" },
                            "has_secret": { "type": "boolean" },
                            "last_error": nullable("string"),
                            "last_sent_at": nullable("string"),
                        }),
                        &["id", "name", "kind", "enabled"],
                    )),
                }),
                &["channels"],
            ),
        ),
        ToolSpec::read(
            "list_packs",
            "List integration packs",
            "Installed integration packs (device types described in YAML): id, version, the \
             device kind they add, whether enabled, how many devices use them, and any error.",
            no_arguments(),
            object_of(
                json!({
                    "packs": list_of(object_of(
                        json!({
                            "id": { "type": "string" },
                            "version": { "type": "string" },
                            "label": { "type": "string" },
                            "kind": nullable("string"),
                            "enabled": { "type": "boolean" },
                            "targets": { "type": "integer" },
                            "error": nullable("string"),
                        }),
                        &["id", "version", "label", "enabled", "targets"],
                    )),
                }),
                &["packs"],
            ),
        ),
    ]
}
