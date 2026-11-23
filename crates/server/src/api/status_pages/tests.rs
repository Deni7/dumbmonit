//! Tests unitaires du module.

use super::*;

#[test]
fn calendar_day_states() {
    assert_eq!(calendar_state(None, 0), "nodata");
    assert_eq!(calendar_state(None, 2), "nodata");
    assert_eq!(calendar_state(Some(100.0), 0), "operational");
    assert_eq!(calendar_state(Some(100.0), 1), "degraded");
    assert_eq!(calendar_state(Some(99.0), 0), "degraded");
    assert_eq!(calendar_state(Some(80.0), 0), "outage");
}

#[test]
fn slug_rule() {
    assert!(is_valid_slug("homelab"));
    assert!(is_valid_slug("my-site-2"));
    assert!(!is_valid_slug("a"));
    assert!(!is_valid_slug("My-Site"));
    assert!(!is_valid_slug("lab_1"));
    assert!(!is_valid_slug(&"x".repeat(41)));
}

#[test]
fn slug_from_title() {
    assert_eq!(slugify("My Homelab"), "my-homelab");
    assert_eq!(slugify("  Réseau — maison  "), "r-seau-maison");
    assert_eq!(slugify("!!!"), "");
}

#[test]
fn device_queries_merge_every_series_of_a_target() {
    // Un renommage ou un tag modifié ouvre une nouvelle série `up` : les
    // requêtes doivent agréger par cible, sinon l'historique repart de zéro.
    let sel = r#"target=~"3|4""#;
    let presence = presence_query(sel, "1d");
    assert!(
        presence
            .contains(r#"sum by (target) (count_over_time(dumbmonit_up{target=~"3|4"}[5m]))"#),
        "{presence}"
    );
    assert!(presence.starts_with("avg_over_time(((sum by (target)"), "{presence}");
    assert!(presence.ends_with("[1d:5m]) * 100"), "{presence}");
    assert_eq!(
        first_up_query(sel, "90d"),
        r#"min by (target) (tfirst_over_time(dumbmonit_up{target=~"3|4"}[90d]))"#
    );
    assert_eq!(
        last_up_query(sel, "90d"),
        r#"max by (target) (tlast_over_time(dumbmonit_up{target=~"3|4"}[90d]))"#
    );
}

#[test]
fn probe_queries_merge_every_series_of_a_target() {
    // Même raison que pour `up` : un changement d'étiquettes ne doit pas
    // effacer les jours d'avant. Succès et mesures sont sommés par cible.
    let sel = r#"target=~"1|2""#;
    assert_eq!(
        probe_uptime_query(sel, "7d", ""),
        "sum by (target) (sum_over_time(dumbmonit_probe_success{target=~\"1|2\"}[7d])) \
         / sum by (target) (count_over_time(dumbmonit_probe_success{target=~\"1|2\"}[7d])) * 100"
    );
    assert!(probe_uptime_query(sel, "1d", " offset 1d").contains("[1d] offset 1d))"));
    assert_eq!(
        probe_failed_query(sel, "1d"),
        "sum by (target) (count_over_time(dumbmonit_probe_success{target=~\"1|2\"}[1d])) \
         - sum by (target) (sum_over_time(dumbmonit_probe_success{target=~\"1|2\"}[1d]))"
    );
}

#[test]
fn latency_query_merges_every_series_of_a_target() {
    assert_eq!(
        latency_query(r#"target=~"1|2""#, "2h"),
        "avg by (target) (avg_over_time(dumbmonit_probe_duration_seconds{target=~\"1|2\"}[2h])) \
         * 1000"
    );
}

#[test]
fn presence_rescaled_to_covered_window() {
    // 12 % mesuré sur 24 h, mais la cible n'existe que depuis 3 h : 96 %.
    let value = rescale(12.0, 86_400.0, 3.0 * 3_600.0).unwrap();
    assert!((value - 96.0).abs() < 0.01);
    assert_eq!(rescale(100.0, 86_400.0, 86_400.0), Some(100.0));
    assert_eq!(rescale(50.0, 86_400.0, 0.0), None);
}

#[test]
fn disabled_device_stops_counting_at_last_sample() {
    let day = 86_400.0;
    let now = 10.0 * day + 3_600.0; // 01:00 le jour 10
    let first = 0.0;
    let last = 7.0 * day + 12.0 * 3_600.0; // pause à midi le jour 7
    let inactive = Some(Some(last));
    // Jour de la pause : seulement la demi-journée mesurée est couverte.
    let paused = device_covered(7.0 * day, 8.0 * day, first, inactive, now);
    assert_eq!(paused, 12.0 * 3_600.0);
    // 12 h de présence sur 12 h couvertes = 100 %, pas 50 %.
    assert_eq!(rescale(50.0, day, paused), Some(100.0));
    // Les jours suivants : aucune couverture, donc aucune valeur (gris).
    for d in 8..=10 {
        let covered =
            device_covered(d as f64 * day, (d + 1) as f64 * day, first, inactive, now);
        assert!(covered <= 0.0);
        assert_eq!(rescale(0.0, day, covered), None);
    }
    // Jamais mesuré et inactif : rien.
    assert!(device_covered(0.0, day, first, Some(None), now) <= 0.0);
}

#[test]
fn active_unreachable_device_stays_down() {
    let day = 86_400.0;
    let now = 10.0 * day;
    // Actif : la couverture va jusqu'à maintenant, l'absence de mesure vaut 0 %.
    let covered = device_covered(9.0 * day, 10.0 * day, 0.0, None, now);
    assert_eq!(covered, day);
    assert_eq!(rescale(0.0, day, covered), Some(0.0));
}

#[test]
fn service_without_samples_has_no_percentage() {
    // Aucune mesure : pas de couverture, donc ni barre rouge ni pourcentage.
    assert_eq!(rescale(0.0, 86_400.0, 0.0), None);
    assert_eq!(rescale(0.0, 90.0 * 86_400.0, -3_600.0), None);
}

#[test]
fn service_with_data_only_since_today_is_not_diluted() {
    // Premier échantillon il y a 2 h sur une fenêtre de 90 j : deux heures
    // de présence complète valent 100 %, pas 0,09 %.
    let window = 90.0 * 86_400.0;
    let measured = 24.0 / (window / 300.0) * 100.0;
    let value = rescale(measured, window, 2.0 * 3_600.0).unwrap();
    assert!((value - 100.0).abs() < 0.01, "{value}");
    // Une seule tranche : trop peu pour un chiffre, donc « sans données ».
    assert_eq!(rescale(0.0035, window, 60.0), None);
}

#[test]
fn badge_names_the_state() {
    let svg = render_badge("major", badges::Style::Flat);
    assert!(svg.contains("major outage"));
    assert!(svg.starts_with("<svg"));
}
