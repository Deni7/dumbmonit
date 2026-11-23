//! Options masquées renvoyées telles quelles par qui n'administre pas.
//!
//! Un compte qui n'est pas administrateur lit un moniteur avec ses options
//! secrètes masquées (`***`) et l'adresse sans identifiants
//! ([`super::targets::reader_tags`], [`super::targets::redact_userinfo`]). Un
//! opérateur d'équipe ou un rôle personnalisé `monitors.manage` qui modifie ce
//! moniteur renvoie le formulaire tel qu'il l'a reçu : sans ce qui suit, le
//! masque remplacerait le vrai secret. Une valeur renvoyée exactement sous sa
//! forme masquée reprend donc la valeur enregistrée ; toute autre valeur est
//! une saisie, gardée telle quelle.

use dumbmonit_proto::Target;

use super::targets::{TargetPayload, reader_tags, redact_userinfo};

/// Remet dans `payload` les valeurs de `before` que le lecteur a vues masquées
/// et renvoyées sans y toucher.
pub fn restore(payload: &mut TargetPayload, before: &Target) {
    let masked = reader_tags(&before.tags);
    for (key, value) in payload.tags.iter_mut() {
        let (Some(shown), Some(stored)) = (masked.get(key), before.tags.get(key)) else {
            continue;
        };
        if value == shown && shown != stored {
            value.clone_from(stored);
        }
    }
    let shown = redact_userinfo(&before.address);
    if payload.address == shown && shown != before.address {
        payload.address.clone_from(&before.address);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use dumbmonit_proto::Credential;

    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn masked_values_sent_back_keep_the_stored_secret() {
        let before = Target {
            id: 1,
            name: "NAS".into(),
            address: "https://bob:s3cret@nas.lan/api".into(),
            kind: "http".into(),
            profile_id: None,
            parent_id: None,
            interval: Duration::from_secs(60),
            enabled: true,
            tags: tags(&[("password", "hunter2"), ("path", "/health")]),
            credential: Credential::None,
            group_name: String::new(),
            position: 0,
        };
        let mut sent = reader_tags(&before.tags);
        assert_eq!(sent["password"], "***");
        sent.insert("path".into(), "/ready".into());
        let mut payload: TargetPayload = serde_json::from_value(serde_json::json!({
            "name": "NAS",
            "address": redact_userinfo(&before.address),
            "kind": "http",
            "tags": sent,
        }))
        .expect("payload");
        restore(&mut payload, &before);
        assert_eq!(payload.tags["password"], "hunter2", "the mask never replaces the secret");
        assert_eq!(payload.tags["path"], "/ready", "a real edit is kept");
        assert_eq!(payload.address, before.address);

        // Une vraie saisie, même d'un secret, est gardée.
        payload.tags.insert("password".into(), "new-one".into());
        payload.address = "https://nas.lan/api".into();
        restore(&mut payload, &before);
        assert_eq!(payload.tags["password"], "new-one");
        assert_eq!(payload.address, "https://nas.lan/api");
    }
}
