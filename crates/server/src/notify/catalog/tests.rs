//! Tests unitaires du module.

use std::collections::BTreeSet;

use serde_json::{Map, Value, json};

use super::*;
use crate::notify::channel::test_config;
use crate::notify::{NotifyError, build, http_client};

const DOC: &str = include_str!("../../../../docs/notifications.md");

/// Ancre qu'un moteur Markdown classique attribue à un titre : minuscules,
/// accents conservés, espaces en tirets, ponctuation supprimée.
fn anchor(title: &str) -> String {
    title
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Ancres de tous les titres de troisième niveau de la documentation.
fn doc_anchors() -> BTreeSet<String> {
    DOC.lines()
        .filter_map(|line| line.strip_prefix("### "))
        .map(|title| format!("#{}", anchor(title)))
        .collect()
}

/// Valeur plausible d'un champ, déduite de son exemple : c'est la manière la
/// plus honnête d'éprouver le formulaire, puisque c'est ce que l'utilisateur
/// recopiera.
fn sample(field: &Field) -> Value {
    let text = if field.placeholder.is_empty() { field.default } else { field.placeholder };
    match (field.shape, field.input) {
        ("list", _) => json!([text]),
        ("object", _) => serde_json::from_str(text).expect("invalid JSON object example"),
        (_, "number") => json!(text.parse::<i64>().expect("invalid numeric example")),
        (_, "select") => json!(field.default),
        (_, "boolean") => json!(false),
        _ => json!(text),
    }
}

fn values(fields: &[Field], keep: impl Fn(&Field) -> bool) -> Value {
    Value::Object(
        fields.iter().filter(|f| keep(f)).map(|f| (f.key.to_string(), sample(f))).collect(),
    )
}

/// Réglages qu'un formulaire ne peut pas marquer obligatoires, parce que le
/// notificateur accepte l'un ou l'autre de deux champs.
fn either_or(kind: &str) -> Option<(&'static str, &'static str)> {
    match kind {
        "apprise" => Some(("config_key", "urls")),
        "twilio" => Some(("from", "messaging_service_sid")),
        _ => None,
    }
}

fn find<'a>(fields: &'a [Field], key: &str) -> &'a Field {
    fields.iter().find(|f| f.key == key).unwrap_or_else(|| panic!("field \"{key}\" missing"))
}

#[test]
fn chaque_type_de_canal_est_catalogue() {
    let catalogue = all();
    let kinds: Vec<&str> = catalogue.iter().map(|k| k.kind).collect();
    assert_eq!(kinds, CHANNEL_KINDS, "the catalogue must follow CHANNEL_KINDS, in order");
    for kind in &catalogue {
        assert!(!kind.label.trim().is_empty(), "{}: empty label", kind.kind);
        assert!(!kind.summary.trim().is_empty(), "{}: empty summary", kind.kind);
        assert!(!kind.doc_url.is_empty(), "{}: empty documentation link", kind.kind);
        assert!(
            !kind.settings.is_empty() || !kind.secrets.is_empty(),
            "{}: no field",
            kind.kind
        );
    }
}

#[test]
fn les_champs_sont_coherents() {
    for kind in all() {
        let mut keys = BTreeSet::new();
        for field in kind.settings.iter().chain(&kind.secrets) {
            let key = field.key;
            assert!(keys.insert(key), "{}: \"{key}\" declared twice", kind.kind);
            assert!(!field.label.trim().is_empty(), "{}: \"{key}\" has no label", kind.kind);
            assert!(
                ["text", "url", "number", "boolean", "select", "textarea", "password"]
                    .contains(&field.input),
                "{}: \"{key}\" has an unknown input type \"{}\"",
                kind.kind,
                field.input
            );
            assert!(
                ["scalar", "list", "object"].contains(&field.shape),
                "{}: \"{key}\" has an unknown shape \"{}\"",
                kind.kind,
                field.shape
            );
            if field.input == "select" {
                assert!(!field.options.is_empty(), "{}: \"{key}\" has no options", kind.kind);
                assert!(
                    field.options.contains(&field.default),
                    "{}: the default of \"{key}\" is not among its options",
                    kind.kind
                );
            } else {
                assert!(field.options.is_empty(), "{}: \"{key}\" is not a select", kind.kind);
            }
            if field.required {
                // Le test de construction s'appuie sur l'exemple : il doit exister.
                assert!(
                    !field.placeholder.is_empty(),
                    "{}: \"{key}\" is required but has no example",
                    kind.kind
                );
            }
        }
    }
}

#[test]
fn chaque_lien_de_documentation_pointe_vers_une_ancre_existante() {
    let anchors = doc_anchors();
    assert!(anchors.contains("#email-smtp"), "inconsistent anchor computation: {anchors:?}");
    assert!(anchors.contains("#bark-ios"), "inconsistent anchor computation: {anchors:?}");
    for kind in all() {
        let Some(fragment) = kind.doc_url.strip_prefix(NOTIFICATIONS_DOC) else {
            panic!("{}: unexpected link \"{}\"", kind.kind, kind.doc_url)
        };
        assert!(
            anchors.contains(fragment),
            "{}: anchor \"{fragment}\" does not exist in docs/notifications.md",
            kind.kind
        );
    }
}

#[test]
fn les_seuls_champs_obligatoires_suffisent_a_construire_le_canal() {
    let http = http_client();
    for kind in all() {
        let mut settings = values(&kind.settings, |f| f.required);
        let secrets = values(&kind.secrets, |f| f.required);
        if let Some((first, _)) = either_or(kind.kind) {
            let field = find(&kind.settings, first);
            settings[first] = sample(field);
        }
        let config = test_config(kind.kind, settings, secrets);
        if let Err(error) = build(&http, &config) {
            panic!("{}: the minimal form is not enough — {error}", kind.kind);
        }
    }
}

#[test]
fn un_choix_entre_deux_champs_est_explique_quand_aucun_n_est_rempli() {
    // Ni « from » ni « messaging_service_sid », ni « config_key » ni « urls » ne
    // peuvent être marqués obligatoires ; en échange, le message d'erreur du
    // notificateur nomme les deux possibilités, et l'aide du formulaire aussi.
    let http = http_client();
    for kind in all() {
        let Some((first, second)) = either_or(kind.kind) else { continue };
        let settings = values(&kind.settings, |f| f.required);
        let secrets = values(&kind.secrets, |f| f.required);
        let Err(error) = build(&http, &test_config(kind.kind, settings, secrets)) else {
            panic!("{}: the alternative is no longer required, update the catalogue", kind.kind)
        };
        assert!(matches!(error, NotifyError::Config(_)), "{}: {error}", kind.kind);
        let message = error.to_string();
        assert!(
            message.contains(first) && message.contains(second),
            "{}: {message}",
            kind.kind
        );
        let all_fields: Vec<&Field> = kind.settings.iter().chain(&kind.secrets).collect();
        for key in [first, second] {
            let field =
                all_fields.iter().find(|f| f.key == key).expect("field of the alternative");
            assert!(!field.required, "{}: \"{key}\" cannot be required", kind.kind);
            assert!(
                field.help.contains("Required if"),
                "{}: the help of \"{key}\" must say when it is required",
                kind.kind
            );
        }
    }
}

#[test]
fn tous_les_exemples_sont_acceptes_ensemble() {
    // Chaque exemple doit être une valeur valide : c'est ce que l'utilisateur
    // recopie en premier, et un exemple refusé serait pire qu'aucun exemple.
    let http = http_client();
    for kind in all() {
        let settings = values(&kind.settings, |_| true);
        let secrets = values(&kind.secrets, |_| true);
        let config = test_config(kind.kind, settings, secrets);
        if let Err(error) = build(&http, &config) {
            panic!("{}: an example is rejected — {error}", kind.kind);
        }
    }
}

#[test]
fn le_reglage_de_certificat_n_est_propose_qu_aux_services_autohebergeables() {
    for kind in all() {
        let declared = kind.settings.iter().find(|f| f.key == INSECURE_TLS_KEY);
        assert_eq!(declared.is_some(), supports_insecure_tls(kind.kind), "{}", kind.kind);
        if let Some(field) = declared {
            assert_eq!(field.input, "boolean");
            assert!(!field.required, "{}: must be opt-in", kind.kind);
            assert!(!field.destination);
        }
    }
    for kind in ["zulip", "matrix", "gotify", "ntfy", "webhook"] {
        assert!(supports_insecure_tls(kind), "{kind}");
    }
    assert!(!supports_insecure_tls("discord"));
}

#[test]
fn les_secrets_par_type_sont_ceux_du_catalogue() {
    // C'est cette liste que l'API refuse dans « settings » : elle doit suivre le
    // catalogue à la lettre, et couvrir au moins ce qu'elle refusait déjà avant
    // d'en dépendre.
    for kind in all() {
        let expected: Vec<&str> = kind.secrets.iter().map(|f| f.key).collect();
        assert_eq!(secret_keys(kind.kind), expected.as_slice(), "{}", kind.kind);
    }
    assert!(secret_keys("webhook").contains(&"url"));
    assert!(secret_keys("pushover").contains(&"user_key"));
    assert!(secret_keys("apprise").contains(&"urls"));
    assert!(secret_keys("carrier-pigeon").is_empty());
}

#[test]
fn le_catalogue_se_serialise_avec_le_contrat_de_l_interface() {
    let json = serde_json::to_value(all()).unwrap();
    let Value::Array(kinds) = json else { panic!("an array was expected") };
    let first = kinds.first().and_then(Value::as_object).expect("one object per kind");
    assert!(first.contains_key("template_format"), "\"template_format\" missing from KindInfo");
    for key in ["kind", "label", "summary", "doc_url", "settings", "secrets", "actions"] {
        assert!(first.contains_key(key), "\"{key}\" missing from KindInfo");
    }
    let field: &Map<String, Value> =
        first["secrets"][0].as_object().expect("one object per field");
    for key in ["key", "label", "required", "input", "help", "placeholder", "options"] {
        assert!(field.contains_key(key), "\"{key}\" missing from Field");
    }
}
