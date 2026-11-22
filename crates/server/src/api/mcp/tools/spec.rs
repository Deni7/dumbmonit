//! Le catalogue des outils : fiche de chaque outil et schémas JSON partagés.

use super::*;

pub struct ToolSpec {
    pub name: &'static str,
    pub scope: Scope,
    /// Nom lisible, pour l'interface du client.
    title: &'static str,
    description: &'static str,
    input_schema: Value,
    /// Forme de `structuredContent` ; le serveur s'engage à la respecter.
    output_schema: Value,
    /// Écriture qui retire ou coupe quelque chose (silence supprimé, appareil
    /// désactivé, conteneur redémarré) plutôt que d'ajouter.
    destructive: bool,
    /// Rappeler l'outil avec les mêmes arguments ne change rien de plus.
    idempotent: bool,
    /// L'outil touche au monde extérieur (réseau balayé, message envoyé), pas
    /// seulement à l'instance.
    open_world: bool,
}

impl ToolSpec {
    pub(super) fn read(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        input_schema: Value,
        output_schema: Value,
    ) -> Self {
        Self {
            name,
            scope: Scope::Read,
            title,
            description,
            input_schema,
            output_schema,
            destructive: false,
            idempotent: true,
            open_world: false,
        }
    }

    pub(super) fn write(
        name: &'static str,
        title: &'static str,
        description: &'static str,
        input_schema: Value,
        output_schema: Value,
    ) -> Self {
        Self {
            name,
            scope: Scope::Write,
            title,
            description,
            input_schema,
            output_schema,
            destructive: false,
            idempotent: false,
            open_world: false,
        }
    }

    pub(super) fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    pub(super) fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }

    pub(super) fn open_world(mut self) -> Self {
        self.open_world = true;
        self
    }

    fn describe(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "outputSchema": self.output_schema,
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.scope == Scope::Read,
                "destructiveHint": self.destructive,
                "idempotentHint": self.idempotent,
                "openWorldHint": self.open_world,
            },
        })
    }
}

/// Argument « équipement » commun à plusieurs outils.
pub(super) fn device_property() -> Value {
    json!({
        "type": ["string", "integer"],
        "description": "Device id (integer) or device name (case-insensitive; a unique \
                        substring is enough)."
    })
}

/// Aucun argument.
pub(super) fn no_arguments() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

pub(super) fn nullable(kind: &str) -> Value {
    json!({ "type": [kind, "null"] })
}

pub(super) fn object_of(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

pub(super) fn list_of(item: Value) -> Value {
    json!({ "type": "array", "items": item })
}

/// Un équipement tel que `Device::summary` le rend.
pub(super) fn device_schema() -> Value {
    object_of(
        json!({
            "id": { "type": "integer" },
            "name": { "type": "string" },
            "kind": { "type": "string" },
            "address": { "type": "string" },
            "state": { "type": "string", "enum": ["reporting", "unreachable", "waiting", "disabled", "down", "unknown"] },
            "enabled": { "type": "boolean" },
            "parent_id": nullable("integer"),
            "parent": nullable("string"),
            "tags": { "type": "object", "additionalProperties": { "type": "string" } },
            "interval_secs": { "type": "integer" },
            "last_probe_at": nullable("string"),
            "last_error": nullable("string"),
        }),
        &["id", "name", "kind", "address", "state", "enabled"],
    )
}

/// Une alerte telle que `alert_json` la rend.
pub(super) fn alert_schema() -> Value {
    object_of(
        json!({
            "fingerprint": { "type": "string" },
            "rule_uid": { "type": "string" },
            "rule": { "type": "string" },
            "severity": { "type": "string", "enum": ["info", "advisory", "warning"] },
            "state": { "type": "string" },
            "device_id": nullable("integer"),
            "device": nullable("string"),
            "suppressed_by": nullable("string"),
            "silenced": { "type": "boolean" },
            "acknowledged": { "type": "boolean" },
            "acked_by": nullable("string"),
            "acked_until": nullable("string"),
            "ack_note": nullable("string"),
            "dismissed": { "type": "boolean" },
            "dismissed_by": nullable("string"),
            "value": nullable("number"),
            "since": nullable("string"),
            "labels": { "type": "object" },
        }),
        &["fingerprint", "rule_uid", "rule", "severity", "state", "silenced", "acknowledged"],
    )
}

/// Le catalogue complet : lecture d'abord, puis écriture.
fn specs() -> Vec<ToolSpec> {
    let mut specs = read_specs();
    specs.extend(write_specs());
    specs
}

pub fn catalogue() -> Vec<Value> {
    specs().iter().map(ToolSpec::describe).collect()
}

pub fn find(name: &str) -> Option<ToolSpec> {
    specs().into_iter().find(|spec| spec.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_schema_and_a_scope() {
        let specs = specs();
        assert_eq!(specs.len(), 28);
        let mut names = std::collections::HashSet::new();
        for spec in &specs {
            assert!(names.insert(spec.name), "nom en double : {}", spec.name);
            // Contraintes de nom de la spécification : 1 à 128 caractères sûrs.
            assert!(
                spec.name.len() <= 128
                    && spec.name.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)),
                "{}",
                spec.name
            );
            assert_eq!(spec.input_schema["type"], "object", "{}", spec.name);
            assert_eq!(spec.output_schema["type"], "object", "{}", spec.name);
            assert!(!spec.description.is_empty() && !spec.title.is_empty(), "{}", spec.name);
            // Les champs requis d'un schéma de sortie existent dans ses propriétés.
            for required in spec.output_schema["required"].as_array().unwrap() {
                let key = required.as_str().unwrap();
                assert!(
                    spec.output_schema["properties"].get(key).is_some(),
                    "{}: {key}",
                    spec.name
                );
            }
            if spec.scope == Scope::Read {
                assert!(!spec.destructive, "{}", spec.name);
            }
        }
        let writers: Vec<&str> =
            specs.iter().filter(|s| s.scope == Scope::Write).map(|s| s.name).collect();
        assert_eq!(
            writers,
            [
                "silence_device",
                "schedule_maintenance",
                "remove_silence",
                "acknowledge_alert",
                "dismiss_alert",
                "probe_device",
                "set_device_enabled",
                "set_rule_enabled",
                "add_device",
                "discover_network",
                "restart_container",
                "test_channel",
                "post_incident"
            ]
        );
        // Chaque outil d'écriture est cité dans les instructions, pour que le
        // modèle sache lesquels demandent la portée `write`.
        for writer in &writers {
            assert!(INSTRUCTIONS.contains(writer), "{writer}");
        }
    }

    #[test]
    fn annotations_describe_the_tool() {
        let catalogue = catalogue();
        let tool = |name: &str| catalogue.iter().find(|t| t["name"] == name).unwrap().clone();
        assert_eq!(tool("get_status")["annotations"]["readOnlyHint"], true);
        assert_eq!(tool("remove_silence")["annotations"]["destructiveHint"], true);
        assert_eq!(tool("silence_device")["annotations"]["destructiveHint"], false);
        assert_eq!(tool("test_channel")["annotations"]["openWorldHint"], true);
        assert_eq!(tool("add_device")["title"], "Add a device");
        assert!(tool("list_devices")["outputSchema"]["properties"]["devices"].is_object());
    }
}
