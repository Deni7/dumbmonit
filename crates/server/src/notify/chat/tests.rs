//! Tests unitaires du module.

use super::*;
use crate::notify::channel::test_config;
use crate::notify::message::sample_message;

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Message d'un refus de configuration.
///
/// Les notificateurs n'implémentent pas `Debug` — ils portent des secrets —,
/// `unwrap_err` leur est donc inaccessible.
fn refus<T>(result: Result<T, NotifyError>) -> String {
    match result {
        Err(erreur) => erreur.to_string(),
        Ok(_) => panic!("invalid configuration accepted"),
    }
}

// --- Microsoft Teams ---

#[test]
fn teams_produit_une_adaptive_card_enveloppee() {
    let attendu = json!({
        "type": "message",
        "attachments": [{
            "contentType": "application/vnd.microsoft.card.adaptive",
            "contentUrl": null,
            "content": {
                "$schema": "http://adaptivecards.io/schemas/adaptive-card.json",
                "type": "AdaptiveCard",
                "version": "1.5",
                "msteams": {"width": "Full"},
                "body": [
                    {
                        "type": "TextBlock",
                        "size": "Medium",
                        "weight": "Bolder",
                        "color": "warning",
                        "wrap": true,
                        "text": "WARN nas — Disk full",
                    },
                    {
                        "type": "TextBlock",
                        "wrap": true,
                        "text": "Warning · Disk full — 95 % (threshold > 90 %)",
                    },
                    {"type": "FactSet", "facts": [
                        {"title": "Monitor", "value": "nas"},
                        {"title": "Severity", "value": "warning"},
                        {"title": "Timestamp", "value": "2025-09-01T14:12:05+00:00"},
                    ]},
                ],
            },
        }],
    });
    assert_eq!(Teams::payload(&sample_message(false), false), attendu);
    assert_eq!(Teams::payload(&sample_message(false), true), attendu, "no links, no buttons");
}

#[test]
fn teams_interactive_cards_open_the_signed_links() {
    let key = crate::notify::action_link::ActionKey::from_bytes([6; 32]);
    let mut message = sample_message(false);
    message.actions = crate::notify::action_link::build(
        &key,
        "https://m.lan",
        &["a@1".into()],
        chrono::Utc::now(),
    );
    let card = Teams::payload(&message, true);
    let actions = &card["attachments"][0]["content"]["actions"];
    assert_eq!(actions[0]["type"], "Action.OpenUrl");
    assert_eq!(actions[0]["title"], "Acknowledge");
    let plain = Teams::payload(&message, false);
    assert!(plain["attachments"][0]["content"].get("actions").is_none());
}

#[test]
fn teams_marque_une_resolution_en_vert() {
    assert_eq!(Teams::accent(&sample_message(true)), "good");
}

#[test]
fn teams_explique_le_refus_d_un_ancien_connecteur() {
    let message = Teams::explain(400, "").expect("400 must be explained");
    assert!(message.contains("Power Automate"), "{message}");
    assert!(!message.contains("400"), "the user has no use for the HTTP code");
}

#[test]
fn teams_exige_une_url_de_webhook() {
    let sans = test_config("teams", json!({}), json!({}));
    let Err(erreur) = Teams::new(client(), &sans) else { panic!("missing URL accepted") };
    assert!(erreur.to_string().contains("webhook_url"));
}

// --- Matrix ---

#[test]
fn matrix_construit_le_chemin_d_envoi_en_encodant_l_identifiant_de_salon() {
    let config = test_config(
        "matrix",
        json!({"server_url": "https://matrix.exemple.org/", "room_id": "!aBcD:exemple.org"}),
        json!({"token": "syt_jeton_matrix"}),
    );
    let notifier = Matrix::new(client(), &config).expect("valid configuration");
    assert_eq!(
        notifier.send_prefix,
        "https://matrix.exemple.org/_matrix/client/v3/rooms/%21aBcD%3Aexemple.org\
         /send/m.room.message"
    );
}

#[test]
fn matrix_refuse_un_nom_de_salon_affiche() {
    // L'erreur la plus fréquente : coller « #général » ou « Alertes » au lieu de
    // l'identifiant interne.
    let config = test_config(
        "matrix",
        json!({"server_url": "https://matrix.exemple.org", "room_id": "Alertes"}),
        json!({"token": "syt_jeton_matrix"}),
    );
    let Err(erreur) = Matrix::new(client(), &config) else { panic!("display name accepted") };
    assert!(erreur.to_string().contains("room_id"), "{erreur}");
}

#[test]
fn matrix_produit_un_message_texte_et_html() {
    let attendu = json!({
        "msgtype": "m.text",
        "body": "WARN nas — Disk full\nWarning · Disk full — 95 % (threshold > 90 %)",
        "format": "org.matrix.custom.html",
        "formatted_body":
            "<p><strong>WARN nas — Disk full</strong></p>\
             <pre>Warning · Disk full — 95 % (threshold &gt; 90 %)</pre>",
    });
    assert_eq!(Matrix::payload(&sample_message(false)), attendu);
}

#[test]
fn matrix_donne_un_identifiant_de_transaction_different_a_chaque_envoi() {
    // Deux alertes du même cycle partent dans la même milliseconde : sans le
    // compteur, Matrix considérerait la seconde comme un doublon et l'ignorerait.
    assert_ne!(Matrix::transaction_id(), Matrix::transaction_id());
}

#[test]
fn matrix_traduit_le_refus_d_un_jeton() {
    assert!(Matrix::explain(401, "").unwrap().contains("token"));
    assert!(Matrix::explain(403, "").unwrap().contains("member"));
}

// --- Mattermost ---

#[test]
fn mattermost_produit_une_piece_jointe_avec_repli_texte() {
    let config = test_config(
        "mattermost",
        json!({}),
        json!({"webhook_url": "https://mm.exemple.org/hooks/abcdef123456"}),
    );
    let notifier = Mattermost::new(client(), &config).expect("valid configuration");
    let attendu = json!({
        "username": "DumbMonit",
        "text": "WARN **nas — Disk full**",
        "attachments": [{
            "fallback": "Warning · Disk full — 95 % (threshold > 90 %)",
            "color": "#D98A00",
            "title": "nas — Disk full",
            "text": "Warning · Disk full — 95 % (threshold > 90 %)",
            "fields": [
                {"title": "Monitor", "value": "nas", "short": true},
                {"title": "Severity", "value": "warning", "short": true},
            ],
        }],
    });
    assert_eq!(notifier.payload(&sample_message(false)), attendu);
}

#[test]
fn mattermost_ne_surcharge_le_salon_que_s_il_est_demande() {
    let config = test_config(
        "mattermost",
        json!({"channel": "alertes"}),
        json!({"webhook_url": "https://mm.exemple.org/hooks/abcdef123456"}),
    );
    let notifier = Mattermost::new(client(), &config).expect("valid configuration");
    assert_eq!(notifier.payload(&sample_message(false))["channel"], "alertes");
}

// --- Rocket.Chat ---

#[test]
fn rocketchat_produit_la_charge_utile_attendue() {
    let config = test_config(
        "rocketchat",
        json!({}),
        json!({"webhook_url": "https://rc.exemple.org/hooks/abc/def123456"}),
    );
    let notifier = RocketChat::new(client(), &config).expect("valid configuration");
    let attendu = json!({
        "alias": "DumbMonit",
        "text": "WARN nas — Disk full",
        "attachments": [{
            "text": "Warning · Disk full — 95 % (threshold > 90 %)",
            "color": "#D98A00",
            "ts": "2025-09-01T14:12:05+00:00",
        }],
    });
    assert_eq!(notifier.payload(&sample_message(false)), attendu);
}

// --- Google Chat ---

#[test]
fn googlechat_double_la_carte_d_un_texte_brut() {
    let charge = GoogleChat::payload(&sample_message(false));
    assert_eq!(charge["text"], "WARN nas — Disk full");
    assert_eq!(charge["cardsV2"][0]["card"]["header"]["title"], "WARN nas — Disk full");
    assert_eq!(charge["cardsV2"][0]["card"]["header"]["subtitle"], "nas — warning");
    assert_eq!(
        charge["cardsV2"][0]["card"]["sections"][0]["widgets"][0]["textParagraph"]["text"],
        "Warning · Disk full — 95 % (threshold > 90 %)"
    );
}

// --- Zulip ---

#[test]
fn zulip_poste_un_formulaire_sur_le_flux_demande() {
    let config = test_config(
        "zulip",
        json!({
            "server_url": "https://zulip.exemple.org/",
            "email": "robot-bot@zulip.exemple.org",
            "stream": "supervision",
            "topic": "nas"
        }),
        json!({"api_key": "cle-api-zulip"}),
    );
    let notifier = Zulip::new(client(), &config).expect("valid configuration");
    assert_eq!(notifier.url, "https://zulip.exemple.org/api/v1/messages");
    assert_eq!(
        notifier.payload(&sample_message(false)),
        vec![
            ("type".to_string(), "stream".to_string()),
            ("to".to_string(), "supervision".to_string()),
            ("topic".to_string(), "nas".to_string()),
            (
                "content".to_string(),
                "WARN **nas — Disk full**\nWarning · Disk full — 95 % (threshold > 90 %)"
                    .to_string()
            ),
        ]
    );
}

#[test]
fn zulip_exige_un_flux_et_une_cle_d_api() {
    let sans_flux = test_config(
        "zulip",
        json!({"server_url": "https://zulip.exemple.org", "email": "a@b.org"}),
        json!({"api_key": "cle-api-zulip"}),
    );
    assert!(refus(Zulip::new(client(), &sans_flux)).contains("stream"));

    let sans_cle = test_config(
        "zulip",
        json!({"server_url": "https://zulip.exemple.org", "email": "a@b.org", "stream": "s"}),
        json!({}),
    );
    assert!(refus(Zulip::new(client(), &sans_cle)).contains("api_key"));
}

#[test]
fn zulip_reconnait_un_flux_inexistant() {
    let corps = r#"{"code":"STREAM_DOES_NOT_EXIST","result":"error"}"#;
    assert!(Zulip::explain(400, corps).unwrap().contains("does not exist"));
}

// --- Garanties communes ---

#[test]
fn aucun_notificateur_de_ce_module_n_expose_son_secret_a_l_affichage() {
    let jeton = SecretString::new("syt_jeton_matrix_tres_secret");
    assert_eq!(format!("{jeton:?}"), "SecretString(***)");
    assert_eq!(format!("{jeton}"), "***");

    let config = test_config(
        "matrix",
        json!({"server_url": "https://matrix.exemple.org", "room_id": "!a:exemple.org"}),
        json!({"token": "syt_jeton_matrix_tres_secret"}),
    );
    let notifier = Matrix::new(client(), &config).expect("valid configuration");
    assert_eq!(format!("{:?}", notifier.token), "SecretString(***)");
}

#[test]
fn chaque_service_annonce_son_type() {
    let webhook = json!({"webhook_url": "https://exemple.org/hooks/abcdef123456"});
    assert_eq!(
        Teams::new(client(), &test_config("teams", json!({}), webhook.clone())).unwrap().kind(),
        "teams"
    );
    assert_eq!(
        Mattermost::new(client(), &test_config("mattermost", json!({}), webhook.clone()))
            .unwrap()
            .kind(),
        "mattermost"
    );
    assert_eq!(
        RocketChat::new(client(), &test_config("rocketchat", json!({}), webhook.clone()))
            .unwrap()
            .kind(),
        "rocketchat"
    );
    assert_eq!(
        GoogleChat::new(client(), &test_config("googlechat", json!({}), webhook))
            .unwrap()
            .kind(),
        "googlechat"
    );
}
