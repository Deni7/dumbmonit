//! Tests unitaires du module.

use super::*;

#[test]
fn le_test_dit_ce_qui_retiendrait_une_vraie_alerte() {
    use crate::alerting::model::Severity;
    let now = Utc::now();
    let open = ChannelPolicy::default();
    assert!(test_filters(true, &open, now).is_empty());
    assert_eq!(test_filters(false, &open, now), vec!["disabled"]);
    let strict = ChannelPolicy { min_severity: Severity::Critical, ..Default::default() };
    assert_eq!(test_filters(true, &strict, now), vec!["min_severity_critical"]);
}

fn payload(kind: &str, settings: Value, secrets: Option<Value>) -> ChannelPayload {
    ChannelPayload {
        name: "canal".to_string(),
        kind: kind.to_string(),
        enabled: None,
        settings: Some(settings),
        secrets,
        policy: None,
    }
}

/// Message d'un refus de validation. `ApiError` n'implémente pas `Debug`, on
/// l'inspecte donc par filtrage plutôt que par `unwrap_err`.
fn refus<T>(result: ApiResult<T>) -> String {
    match result {
        Err(ApiError::BadRequest(message) | ApiError::Conflict(message)) => message,
        Err(_) => panic!("une erreur d'utilisateur était attendue"),
        Ok(_) => panic!("la validation aurait dû échouer"),
    }
}

#[test]
fn un_secret_depose_dans_les_reglages_est_refuse() {
    let message = refus(
        payload("discord", json!({ "webhook_url": "https://exemple/x" }), None).validate(None),
    );
    assert!(message.contains("secrets"), "message inattendu : {message}");
    assert!(!message.contains("exemple"), "l'URL a fuité : {message}");
}

#[test]
fn l_url_d_un_webhook_generique_ne_peut_pas_vivre_dans_les_reglages() {
    let message =
        refus(payload("webhook", json!({ "url": "https://exemple/x" }), None).validate(None));
    assert!(message.contains("secrets"), "message inattendu : {message}");
}

#[test]
fn un_type_de_canal_inconnu_est_refuse_avec_la_liste() {
    let message = refus(payload("pigeon-voyageur", json!({}), None).validate(None));
    assert!(message.contains("discord"), "message inattendu : {message}");
}

#[test]
fn une_configuration_incomplete_est_refusee_avant_enregistrement() {
    // ntfy sans sujet : le notificateur lui-même dit ce qui manque.
    let message = refus(payload("ntfy", json!({}), None).validate(None));
    assert!(message.contains("topic"), "message inattendu : {message}");
}

#[test]
fn la_politique_d_un_canal_se_complete_par_dessus_l_existante() {
    let current = ChannelPolicy {
        min_interval_secs: 300,
        notify_resolved: false,
        ..ChannelPolicy::default()
    };
    let Ok(policy) = parse_policy(Some(json!({ "min_severity": "critical" })), &current) else {
        panic!("a partial policy should be accepted");
    };
    assert_eq!(policy.min_severity, crate::alerting::model::Severity::Critical);
    assert_eq!(policy.min_interval_secs, 300, "untouched field kept");
    assert!(!policy.notify_resolved);

    let Ok(kept) = parse_policy(None, &current) else {
        panic!("absent policy keeps the current one");
    };
    assert_eq!(kept, current);

    let message = refus(parse_policy(Some(json!({ "min_severity": "loud" })), &current));
    assert!(message.contains("severity"), "{message}");
    let message = refus(parse_policy(Some(json!({ "min_interval_secs": -1 })), &current));
    assert!(message.contains("negative"), "{message}");
    let message = refus(parse_policy(
        Some(
            json!({ "quiet_hours": { "kind": "once", "starts_at": "2026-01-01T00:00:00Z", "ends_at": "2026-01-02T00:00:00Z" } }),
        ),
        &current,
    ));
    assert!(message.contains("weekly"), "{message}");

    let Ok(cleared) = parse_policy(
        Some(json!({ "quiet_hours": null })),
        &ChannelPolicy {
            quiet_hours: Some(Schedule::Weekly {
                days: vec![0],
                start_minute: 0,
                end_minute: 60,
                utc_offset_minutes: 0,
                timezone: None,
            }),
            ..ChannelPolicy::default()
        },
    ) else {
        panic!("null clears the quiet hours");
    };
    assert!(cleared.quiet_hours.is_none());
}

#[test]
fn les_secrets_conserves_valident_la_configuration() {
    let current = ChannelConfig {
        id: 1,
        name: "ancien".to_string(),
        kind: "discord".to_string(),
        enabled: true,
        settings: json!({}),
        secrets: json!({ "webhook_url": "https://exemple.test/hook" }),
    };
    // Renommage sans secrets : la validation s'appuie sur ceux déjà en base, et
    // le brouillon n'en porte aucun, ce qui préserve la colonne chiffrée.
    let Ok(draft) = payload("discord", json!({}), None).validate(Some(&current)) else {
        panic!("le renommage aurait dû être accepté");
    };
    assert!(draft.secrets.is_none(), "les secrets doivent rester en base");
}

fn pushover(secrets: Value) -> ChannelConfig {
    ChannelConfig {
        id: 1,
        name: "ancien".to_string(),
        kind: "pushover".to_string(),
        enabled: true,
        settings: json!({}),
        secrets,
    }
}

#[test]
fn un_secret_soumis_seul_conserve_les_autres() {
    let current = pushover(json!({ "user_key": "utilisateur", "token": "ancien" }));
    let Ok(draft) = payload("pushover", json!({}), Some(json!({ "token": "nouveau" })))
        .validate(Some(&current))
    else {
        panic!("la modification aurait dû être acceptée");
    };
    assert_eq!(draft.secrets, Some(json!({ "user_key": "utilisateur", "token": "nouveau" })));
}

#[test]
fn une_chaine_vide_efface_la_cle_et_les_inconnues_sont_ignorees() {
    let merged = merge_secrets(
        "pushover",
        &json!({ "user_key": "u", "token": "t", "obsolete": "x" }),
        &json!({ "token": "", "inconnue": "y" }),
    );
    assert_eq!(merged, json!({ "user_key": "u" }));
}

#[test]
fn un_objet_vide_efface_toujours_les_secrets() {
    // `"secrets": {}` est l'effacement explicite de l'API (et du bouton
    // « effacer » de l'interface) : la fusion ne doit pas en faire un « tout
    // garder ».
    let current = ChannelConfig {
        id: 1,
        name: "ancien".to_string(),
        kind: "ntfy".to_string(),
        enabled: true,
        settings: json!({ "topic": "alertes" }),
        secrets: json!({ "token": "tk_ancien" }),
    };
    let Ok(draft) = payload("ntfy", json!({ "topic": "alertes" }), Some(json!({})))
        .validate(Some(&current))
    else {
        panic!("l'effacement aurait dû être accepté");
    };
    assert_eq!(draft.secrets, Some(json!({})));
}
