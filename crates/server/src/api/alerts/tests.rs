//! Tests unitaires du module.

use super::*;

/// Message d'un refus de validation.
///
/// `ApiError` n'implémente pas `Debug` — c'est voulu, une erreur d'API n'a pas à
/// être imprimable telle quelle : on l'inspecte donc par filtrage.
fn refus<T>(result: ApiResult<T>) -> String {
    match result {
        Err(ApiError::BadRequest(message) | ApiError::Conflict(message)) => message,
        Err(_) => panic!("une erreur d'utilisateur était attendue"),
        Ok(_) => panic!("la validation aurait dû échouer"),
    }
}

fn accepte<T>(result: ApiResult<T>) -> T {
    match result {
        Ok(value) => value,
        Err(
            ApiError::BadRequest(message)
            | ApiError::Conflict(message)
            | ApiError::NotFound(message),
        ) => panic!("refus inattendu : {message}"),
        Err(_) => panic!("erreur interne inattendue"),
    }
}

#[test]
fn un_nom_francais_donne_un_identifiant_lisible() {
    assert_eq!(slugify("Mémoire saturée"), "memoire_saturee");
    assert_eq!(slugify("CPU > 90 %"), "cpu_90");
    assert_eq!(slugify("  ---  "), "");
}

#[test]
fn les_enumerations_mal_orthographiees_sont_refusees() {
    assert!(refus(parse_severity(Some("warnning"))).contains("severity"));
    assert!(refus(parse_operator(Some("=>"))).contains("operator"));
    assert!(refus(parse_kind(Some("seuil"))).contains("rule type"));
    // L'absence de valeur reste acceptée : elle vaut le défaut du domaine.
    assert_eq!(accepte(parse_severity(None)), Severity::Warning);
    assert_eq!(accepte(parse_operator(None)), Operator::Gt);
    assert_eq!(accepte(parse_kind(None)), RuleKind::Threshold);
}

#[test]
fn les_regles_de_prevision_et_de_derive_sont_reconnues() {
    assert_eq!(accepte(parse_kind(Some("forecast"))), RuleKind::Forecast);
    assert_eq!(accepte(parse_kind(Some("drift"))), RuleKind::Drift);
    assert_eq!(accepte(parse_kind(Some("slo_burn"))), RuleKind::SloBurn);

    let params = accepte(parse_params(Some(serde_json::json!({
        "forecast_warning_days": 21,
        "forecast_critical_days": 5,
        "drift_direction": "down",
        "drift_min_value": 0.05
    }))));
    assert_eq!(params.forecast_warning_days, 21.0);
    assert_eq!(params.forecast_critical_days, 5.0);
    assert_eq!(params.forecast_window_days, 7, "untouched fields keep their default");
    assert_eq!(params.drift_direction, DriftDirection::Down);
    assert_eq!(params.drift_min_value, 0.05);

    // Le critique doit tomber avant l'avertissement.
    let inverted = parse_params(Some(serde_json::json!({
        "forecast_warning_days": 3, "forecast_critical_days": 7
    })));
    assert!(refus(inverted).contains("forecast_warning_days"));
    let window = parse_params(Some(serde_json::json!({ "forecast_window_days": 400 })));
    assert!(refus(window).contains("forecast_window_days"));
    let confidence = parse_params(Some(serde_json::json!({ "forecast_min_confidence": 2 })));
    assert!(refus(confidence).contains("forecast_min_confidence"));
    let floor = parse_params(Some(serde_json::json!({ "drift_min_delta": -1 })));
    assert!(refus(floor).contains("drift_min_delta"));
}

#[test]
fn une_duree_negative_est_refusee() {
    assert!(refus(parse_duration(Some(-1), "The duration")).contains("negative"));
    assert_eq!(accepte(parse_duration(Some(0), "The duration")), None);
    assert_eq!(accepte(parse_duration(Some(30), "The duration")), Some(Duration::from_secs(30)));
}

#[test]
fn l_identifiant_reserve_de_la_regle_injoignable_est_protege() {
    assert!(refus(check_uid_shape(RULE_HOST_DOWN)).contains("reserved"));
    assert!(refus(check_uid_shape("Accentué")).contains("lowercase"));
}

#[test]
fn un_seuil_de_retour_du_mauvais_cote_est_refuse() {
    assert!(refus(check_clear_threshold(Operator::Gt, 90.0, Some(95.0))).contains("below"));
    assert!(refus(check_clear_threshold(Operator::Lt, 10.0, Some(5.0))).contains("above"));
    assert!(refus(check_clear_threshold(Operator::Gt, 90.0, Some(f64::NAN))).contains("finite"));
    accepte(check_clear_threshold(Operator::Gt, 90.0, Some(80.0)));
    accepte(check_clear_threshold(Operator::Le, 10.0, Some(12.0)));
    accepte(check_clear_threshold(Operator::Gt, 90.0, None));
}

#[test]
fn une_fenetre_hebdomadaire_de_duree_nulle_est_refusee() {
    let raw = serde_json::json!({
        "kind": "weekly", "days": [0], "start_minute": 120, "end_minute": 120
    });
    assert!(refus(parse_schedule(raw)).contains("must differ"));
}

#[test]
fn une_fenetre_mensuelle_sans_jour_est_refusee() {
    let raw = serde_json::json!({
        "kind": "monthly", "start_minute": 120, "duration_minutes": 120
    });
    assert!(refus(parse_schedule(raw)).contains("at least one day"));
}

#[test]
fn un_rang_ou_un_quantieme_hors_bornes_est_refuse() {
    let raw = serde_json::json!({
        "kind": "monthly", "days": [32], "start_minute": 0, "duration_minutes": 60
    });
    assert!(refus(parse_schedule(raw)).contains("1 to 31"));

    let raw = serde_json::json!({
        "kind": "monthly", "nth_weekdays": [{"nth": 6, "weekday": 6}],
        "start_minute": 0, "duration_minutes": 60
    });
    assert!(refus(parse_schedule(raw)).contains("rank"));

    let raw = serde_json::json!({
        "kind": "monthly", "nth_weekdays": [{"nth": 1, "weekday": 9}],
        "start_minute": 0, "duration_minutes": 60
    });
    assert!(refus(parse_schedule(raw)).contains("weekday"));
}

#[test]
fn une_duree_mensuelle_nulle_ou_demesuree_est_refusee() {
    for duration in [0, MAX_DURATION_MINUTES + 1] {
        let raw = serde_json::json!({
            "kind": "monthly", "days": [1], "start_minute": 0,
            "duration_minutes": duration
        });
        assert!(refus(parse_schedule(raw)).contains("duration_minutes"));
    }
}

#[test]
fn une_fenetre_mensuelle_valide_est_acceptee() {
    let raw = serde_json::json!({
        "kind": "monthly",
        "nth_weekdays": [{"nth": 1, "weekday": 6}, {"nth": -1, "weekday": 4}],
        "start_minute": 120, "duration_minutes": 120, "timezone": "Europe/Paris"
    });
    let schedule = accepte(parse_schedule(raw));
    let Schedule::Monthly { nth_weekdays, timezone, .. } = &schedule else {
        panic!("a monthly window was expected");
    };
    assert_eq!(nth_weekdays[0], crate::alerting::silence::NthWeekday { nth: 1, weekday: 6 });
    assert_eq!(nth_weekdays[1].nth, LAST_OF_MONTH);
    assert_eq!(timezone.as_deref(), Some("Europe/Paris"));
}

#[test]
fn un_fuseau_inconnu_est_refuse_avec_un_exemple() {
    let raw = serde_json::json!({
        "kind": "weekly", "days": [6], "start_minute": 120, "end_minute": 240,
        "timezone": "Mars/Olympus"
    });
    let message = refus(parse_schedule(raw));
    assert!(message.contains("Mars/Olympus"), "{message}");
    assert!(message.contains("Europe/Paris"), "{message}");
}

#[test]
fn une_fenetre_hebdomadaire_sans_fuseau_reste_acceptee() {
    // Exactement ce qu'envoyaient les versions précédentes de l'interface.
    let raw = serde_json::json!({
        "kind": "weekly", "days": [6], "start_minute": 120, "end_minute": 240,
        "utc_offset_minutes": 60
    });
    let schedule = accepte(parse_schedule(raw));
    assert!(matches!(schedule, Schedule::Weekly { .. }));
}

#[test]
fn une_fenetre_ponctuelle_inversee_est_refusee() {
    let raw = serde_json::json!({
        "kind": "once",
        "starts_at": "2026-01-01T02:00:00Z",
        "ends_at": "2026-01-01T01:00:00Z"
    });
    assert!(refus(parse_schedule(raw)).contains("after its start"));
}

#[test]
fn la_planification_jusqu_au_retablissement_est_acceptee() {
    let raw = serde_json::json!({"kind": "recovery", "starts_at": "2026-03-01T10:00:00Z"});
    assert!(matches!(accepte(parse_schedule(raw)), Schedule::Recovery { .. }));
}

#[test]
fn les_reglages_d_anomalie_incoherents_sont_refuses() {
    assert!(refus(parse_params(Some(serde_json::json!({"alpha": 0})))).contains("alpha"));
    assert!(refus(parse_params(Some(serde_json::json!({"k": -1})))).contains("k"));
    assert!(
        refus(parse_params(Some(serde_json::json!({"min_samples": 0})))).contains("min_samples")
    );
    // Un objet partiel complète avec les valeurs du domaine.
    let params = accepte(parse_params(Some(serde_json::json!({"k": 4.0}))));
    assert_eq!(params.k, 4.0);
    assert_eq!(params.alpha, AnomalyParams::default().alpha);
}
