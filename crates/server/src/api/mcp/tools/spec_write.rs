//! Fiches des outils d'écriture.

use super::*;

/// Les outils qui modifient l'instance ou le monde extérieur.
pub(super) fn write_specs() -> Vec<ToolSpec> {
    vec![
        // --- Écriture --------------------------------------------------------
        ToolSpec::write(
            "silence_device",
            "Silence a device",
            "Silences every alert of one device for a while (a one-off maintenance window \
             starting now). Alerts keep being evaluated but nothing is notified. Returns the \
             silence id and when it ends.",
            json!({
                "type": "object",
                "required": ["device"],
                "properties": {
                    "device": device_property(),
                    "hours": { "type": "number", "description": "Duration in hours (default 1, max 168).", "exclusiveMinimum": 0 },
                    "comment": { "type": "string", "description": "Why — shown in the UI next to the silence." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "id": { "type": "integer" },
                    "device_id": { "type": "integer" },
                    "device": { "type": "string" },
                    "starts_at": { "type": "string" },
                    "ends_at": { "type": "string" },
                }),
                &["id", "device_id", "device", "starts_at", "ends_at"],
            ),
        ),
        ToolSpec::write(
            "schedule_maintenance",
            "Schedule maintenance",
            "Plans a one-off maintenance window at a given time — for one device, or for the \
             whole instance when no device is named. Alerts in the window are evaluated but \
             not notified. At most 7 days long.",
            json!({
                "type": "object",
                "required": ["starts_at"],
                "properties": {
                    "starts_at": { "type": "string", "description": "Start, RFC 3339 (e.g. 2026-10-04T22:00:00Z)." },
                    "ends_at": { "type": "string", "description": "End, RFC 3339. Alternatively give hours." },
                    "hours": { "type": "number", "description": "Duration in hours when ends_at is not given (default 1, max 168).", "exclusiveMinimum": 0 },
                    "device": device_property(),
                    "name": { "type": "string", "description": "Short name shown in the UI." },
                    "comment": { "type": "string", "description": "Why." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "id": { "type": "integer" },
                    "name": { "type": "string" },
                    "device_id": nullable("integer"),
                    "device": nullable("string"),
                    "starts_at": { "type": "string" },
                    "ends_at": { "type": "string" },
                    "active_now": { "type": "boolean" },
                }),
                &["id", "name", "starts_at", "ends_at", "active_now"],
            ),
        ),
        ToolSpec::write(
            "remove_silence",
            "Remove a maintenance window",
            "Removes a maintenance window by id (see list_silences). Alerts on the device \
             notify again.",
            json!({
                "type": "object",
                "required": ["id"],
                "properties": { "id": { "type": "integer", "description": "Silence id." } },
                "additionalProperties": false
            }),
            object_of(
                json!({ "id": { "type": "integer" }, "removed": { "type": "boolean" } }),
                &["id", "removed"],
            ),
        )
        .destructive()
        .idempotent(),
        ToolSpec::write(
            "acknowledge_alert",
            "Acknowledge an alert",
            "Acknowledges one alert by fingerprint (see list_alerts): \"I know, stop reminding \
             me\". Reminders and escalations pause for the given hours (default 4, at most \
             720); the alert keeps being evaluated and its resolution is still notified. Pass \
             hours 0 to lift an acknowledgement. To mute a whole device, use silence_device.",
            json!({
                "type": "object",
                "required": ["fingerprint"],
                "properties": {
                    "fingerprint": { "type": "string", "description": "Alert fingerprint (see list_alerts)." },
                    "hours": { "type": "number", "description": "Duration in hours (default 4, max 720); 0 lifts the acknowledgement.", "minimum": 0 },
                    "note": { "type": "string", "description": "Why — shown in the UI next to the alert." }
                },
                "additionalProperties": false
            }),
            alert_schema(),
        ),
        ToolSpec::write(
            "dismiss_alert",
            "Dismiss an alert",
            "Dismisses one alert by fingerprint (see list_alerts): \"got it\". The current \
             occurrence leaves the active alerts and sends nothing more — no reminder, no \
             escalation, no resolution message — for as long as the condition lasts. If the \
             condition clears and comes back later, that is a new alert, notified as usual. \
             Pass undo true to restore a dismissed alert.",
            json!({
                "type": "object",
                "required": ["fingerprint"],
                "properties": {
                    "fingerprint": { "type": "string", "description": "Alert fingerprint (see list_alerts)." },
                    "undo": { "type": "boolean", "description": "Restore a dismissed alert instead (default false)." }
                },
                "additionalProperties": false
            }),
            alert_schema(),
        )
        .idempotent(),
        ToolSpec::write(
            "probe_device",
            "Probe a device now",
            "Probes a device right now instead of waiting for its next scheduled poll, and \
             reports what was measured — or why it failed. Useful to check whether an outage \
             is over.",
            json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Device id." },
                    "name": { "type": "string", "description": "Device name, if the id is unknown." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "device_id": { "type": "integer" },
                    "device": { "type": "string" },
                    "ok": { "type": "boolean" },
                    "sample_count": { "type": "integer" },
                    "series": list_of(json!({ "type": "string" })),
                }),
                &["device_id", "device", "ok", "sample_count", "series"],
            ),
        )
        .open_world(),
        ToolSpec::write(
            "set_device_enabled",
            "Enable or disable a device",
            "Enables or disables monitoring of a device. A disabled device is not polled and \
             raises no alert; its history is kept.",
            json!({
                "type": "object",
                "required": ["enabled"],
                "properties": {
                    "id": { "type": "integer", "description": "Device id." },
                    "name": { "type": "string", "description": "Device name, if the id is unknown." },
                    "enabled": { "type": "boolean" }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "device_id": { "type": "integer" },
                    "device": { "type": "string" },
                    "enabled": { "type": "boolean" },
                }),
                &["device_id", "device", "enabled"],
            ),
        )
        .destructive()
        .idempotent(),
        ToolSpec::write(
            "set_rule_enabled",
            "Enable or disable an alert rule",
            "Enables or disables an alert rule by uid or id. Disabling a rule resolves its \
             active alerts at the next evaluation.",
            json!({
                "type": "object",
                "required": ["enabled"],
                "properties": {
                    "uid": { "type": "string", "description": "Rule uid (see list_rules)." },
                    "id": { "type": "integer", "description": "Rule id, alternatively." },
                    "enabled": { "type": "boolean" }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "id": { "type": "integer" },
                    "uid": { "type": "string" },
                    "name": { "type": "string" },
                    "enabled": { "type": "boolean" },
                }),
                &["id", "uid", "name", "enabled"],
            ),
        )
        .destructive()
        .idempotent(),
        ToolSpec::write(
            "add_device",
            "Add a device",
            "Adds a device or service to monitor. Call list_device_types first (with the \
             kind) to learn the address format, the credential shape and the options. The \
             credential is stored encrypted and never returned; prefer the web interface \
             for secrets you would rather not type into a conversation.",
            json!({
                "type": "object",
                "required": ["name", "kind", "address"],
                "properties": {
                    "name": { "type": "string", "description": "Display name, unique and short (\"NAS\", \"Core switch\")." },
                    "kind": { "type": "string", "description": "Device type, from list_device_types (snmp, proxmox, http, ping…)." },
                    "address": { "type": "string", "description": "Host name, IP address or URL, as list_device_types describes for this kind." },
                    "interval_secs": { "type": "integer", "minimum": 10, "description": "Polling period in seconds (default 60)." },
                    "parent": device_property(),
                    "via_agent": device_property(),
                    "options": { "type": "object", "additionalProperties": { "type": ["string", "number", "boolean"] }, "description": "Kind-specific options by key (see list_device_types)." },
                    "tags": { "type": "object", "additionalProperties": { "type": "string" }, "description": "Free labels (role, site…)." },
                    "credential": { "type": "object", "description": "Credential object with its \"type\" (none, snmp_community, snmp_v3, api_token, username_password) and the fields list_device_types gives for it." },
                    "enabled": { "type": "boolean", "description": "Start monitoring right away (default true)." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "id": { "type": "integer" },
                    "name": { "type": "string" },
                    "address": { "type": "string" },
                    "kind": { "type": "string" },
                    "enabled": { "type": "boolean" },
                    "interval_secs": { "type": "integer" },
                    "credential_kind": { "type": "string" },
                }),
                &["id", "name", "address", "kind", "enabled", "interval_secs", "credential_kind"],
            ),
        ),
        ToolSpec::write(
            "discover_network",
            "Discover SNMP devices",
            "Scans a network (CIDR, 4096 addresses at most) for devices that answer SNMP, and \
             lists them with the profile auto-detection would apply. Nothing is added: use \
             add_device (kind snmp) for the ones to monitor. The server sends the probes.",
            json!({
                "type": "object",
                "required": ["cidr"],
                "properties": {
                    "cidr": { "type": "string", "description": "Network to scan, e.g. 192.168.1.0/24." },
                    "community": { "type": "string", "description": "SNMP v2c community (default public)." },
                    "port": { "type": "integer", "description": "UDP port (default 161)." },
                    "timeout_ms": { "type": "integer", "description": "Per-address timeout, 100 to 10000 ms." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "scanned": { "type": "integer" },
                    "devices": list_of(object_of(
                        json!({
                            "address": { "type": "string" },
                            "sysname": nullable("string"),
                            "sysdescr": nullable("string"),
                            "sysobjectid": nullable("string"),
                            "suggested_profile": nullable("string"),
                        }),
                        &["address"],
                    )),
                }),
                &["scanned", "devices"],
            ),
        )
        .idempotent()
        .open_world(),
        ToolSpec::write(
            "restart_container",
            "Restart a container",
            "Asks the agent of a machine to restart one of its Docker containers (see \
             list_containers). The command is queued; the agent runs it within seconds and \
             list_containers shows the outcome.",
            json!({
                "type": "object",
                "required": ["device", "container"],
                "properties": {
                    "device": device_property(),
                    "container": { "type": "string", "description": "Container name." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "command_id": { "type": "integer" },
                    "device_id": { "type": "integer" },
                    "device": { "type": "string" },
                    "container": { "type": "string" },
                    "status": { "type": "string" },
                }),
                &["command_id", "device_id", "device", "container", "status"],
            ),
        )
        .destructive(),
        ToolSpec::write(
            "test_channel",
            "Test a notification channel",
            "Sends a test message through one notification channel (see list_channels), by \
             the exact path real alerts take, and says whether it left.",
            json!({
                "type": "object",
                "required": ["channel"],
                "properties": {
                    "channel": { "type": ["string", "integer"], "description": "Channel id or name." }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "channel_id": { "type": "integer" },
                    "channel": { "type": "string" },
                    "ok": { "type": "boolean" },
                    "message": { "type": "string" },
                }),
                &["channel_id", "channel", "ok", "message"],
            ),
        )
        .open_world(),
        ToolSpec::write(
            "post_incident",
            "Post an incident update",
            "Posts on the status pages: opens an incident (or announces a maintenance) with a \
             first message, or — with incident_id — adds an update to an existing one and \
             moves its status. Email subscribers of the page are notified. Incident statuses: \
             investigating, identified, monitoring, resolved; maintenance: scheduled, \
             in_progress, completed.",
            json!({
                "type": "object",
                "required": ["body"],
                "properties": {
                    "body": { "type": "string", "description": "The message readers will see." },
                    "incident_id": { "type": "integer", "description": "Add an update to this incident instead of opening a new one (see list_status_pages)." },
                    "title": { "type": "string", "description": "Title of a new incident." },
                    "page": { "type": ["string", "integer"], "description": "Status page id, slug or title for a new incident; omit for every page." },
                    "kind": { "type": "string", "enum": ["incident", "maintenance"], "description": "Default incident." },
                    "status": { "type": "string", "enum": ["investigating", "identified", "monitoring", "resolved", "scheduled", "in_progress", "completed"] },
                    "severity": { "type": "string", "enum": ["minor", "major"] }
                },
                "additionalProperties": false
            }),
            object_of(
                json!({
                    "id": { "type": "integer" },
                    "title": { "type": "string" },
                    "kind": { "type": "string" },
                    "status": { "type": "string" },
                    "severity": { "type": "string" },
                    "page_id": nullable("integer"),
                    "updates": { "type": "integer" },
                }),
                &["id", "title", "kind", "status", "severity", "updates"],
            ),
        )
        .open_world(),
    ]
}
