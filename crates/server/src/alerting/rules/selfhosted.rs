//! Règles livrées des applications de `collectors/selfhosted` (famille *arr,
//! qBittorrent, Frigate, Keycloak, authentik, Vaultwarden, mailcow).

use super::*;

/// Durée en minutes, pour les règles des applications.
fn app_minutes(n: u64) -> Duration {
    Duration::from_secs(n * 60)
}

/// Durée en heures, pour les règles des applications.
fn app_hours(n: u64) -> Duration {
    Duration::from_secs(n * 3600)
}

/// Une règle de seuil d'application : retenue, gravité et rappel.
#[allow(clippy::too_many_arguments)]
fn app_rule(
    uid: &str,
    name: &str,
    description: &str,
    query: &str,
    operator: Operator,
    threshold: f64,
    hold: Duration,
    severity: Severity,
    repeat: Duration,
) -> Rule {
    Rule {
        description: description.to_string(),
        operator,
        threshold,
        for_duration: hold,
        severity,
        repeat_interval: Some(repeat),
        ..base(uid, name, RuleKind::Threshold, query)
    }
}

/// Règles des applications de `collectors/selfhosted` : Sonarr, Radarr, Lidarr,
/// Prowlarr, qBittorrent, Frigate, Keycloak, authentik, Vaultwarden et
/// mailcow. Une application qui ne répond plus du tout relève de
/// la règle « injoignable » ; celles-ci surveillent ce qui continue de répondre
/// pendant que le service est en panne.
pub(super) fn apps_rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    // La famille *arr : mêmes métriques, préfixées par le produit.
    // Prowlarr n'a ni file de téléchargements ni `/diskspace` : pas de règle.
    for (kind, product, has_queue, has_disk) in [
        ("sonarr", "Sonarr", true, true),
        ("radarr", "Radarr", true, true),
        ("lidarr", "Lidarr", true, true),
        ("prowlarr", "Prowlarr", false, false),
    ] {
        rules.push(app_rule(
            &format!("{kind}_health_error"),
            &format!("{product} health error"),
            &format!(
                "{product} reports an error on its System → Status page: a download client or \
                 indexer that fails, a missing root folder."
            ),
            &format!("dumbmonit_{kind}_health_errors"),
            Operator::Gt,
            0.0,
            app_minutes(15),
            Severity::Warning,
            app_hours(6),
        ));
        if has_queue {
            rules.push(app_rule(
                &format!("{kind}_queue_errors"),
                &format!("{product} queue has errors"),
                &format!(
                    "An item in {product}'s download queue has an error (failed download, import \
                     blocked) and nothing cleared it for half an hour."
                ),
                &format!("dumbmonit_{kind}_queue_has_errors"),
                Operator::Gt,
                0.0,
                app_minutes(30),
                Severity::Warning,
                app_hours(6),
            ));
        }
        if has_disk {
            rules.push(Rule {
                clear_threshold: Some(88.0),
                unit: "%".to_string(),
                ..app_rule(
                    &format!("{kind}_disk_almost_full"),
                    &format!("{product} disk almost full"),
                    &format!("A disk {product} reports is more than 90% full."),
                    &format!("dumbmonit_{kind}_disk_used_percent"),
                    Operator::Gt,
                    90.0,
                    app_minutes(30),
                    Severity::Warning,
                    app_hours(24),
                )
            });
        }
        rules.push(app_rule(
            &format!("{kind}_update_available"),
            &format!("{product} update available"),
            &format!("A newer {product} release is available."),
            &format!("dumbmonit_{kind}_update_available"),
            Operator::Gt,
            0.0,
            app_hours(1),
            Severity::Info,
            app_hours(7 * 24),
        ));
    }
    rules.push(app_rule(
        "prowlarr_indexers_disabled",
        "Prowlarr indexers disabled",
        "Prowlarr disabled indexers after repeated failures: Sonarr, Radarr and Lidarr stop \
         finding releases through them.",
        "dumbmonit_prowlarr_indexers_disabled",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Warning,
        app_hours(24),
    ));

    // qBittorrent.
    rules.push(app_rule(
        "qbittorrent_torrents_errored",
        "qBittorrent torrents in error",
        "Torrents are in error or have lost their files (disk full, folder moved): they never \
         resume by themselves.",
        "dumbmonit_qbittorrent_torrents_errored",
        Operator::Gt,
        0.0,
        app_minutes(15),
        Severity::Warning,
        app_hours(24),
    ));
    rules.push(app_rule(
        "qbittorrent_disconnected",
        "qBittorrent disconnected",
        "qBittorrent is not connected to the network (closed port, VPN down): nothing is \
         downloaded or seeded.",
        "dumbmonit_qbittorrent_connected == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(15),
        Severity::Warning,
        app_hours(6),
    ));
    rules.push(Rule {
        clear_threshold: Some(6.0),
        unit: "GiB".to_string(),
        ..app_rule(
            "qbittorrent_free_space_low",
            "qBittorrent free space low",
            "Less than 5 GiB free in qBittorrent's download folder.",
            "dumbmonit_qbittorrent_free_space_bytes / 1073741824",
            Operator::Lt,
            5.0,
            app_minutes(30),
            Severity::Warning,
            app_hours(24),
        )
    });

    // Frigate.
    rules.push(app_rule(
        "frigate_camera_offline",
        "Frigate camera offline",
        "A camera delivers no frames to Frigate (stream down, camera off): its recording and \
         detection have stopped.",
        "dumbmonit_frigate_camera_online == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(5),
        Severity::Warning,
        app_hours(6),
    ));
    rules.push(Rule {
        unit: "s".to_string(),
        ..app_rule(
            "frigate_detector_stuck",
            "Frigate detector stuck",
            "A detector has been busy on the same inference for more than 30 seconds (Coral \
             or GPU hung).",
            "dumbmonit_frigate_detector_busy_seconds",
            Operator::Gt,
            30.0,
            app_minutes(2),
            Severity::Warning,
            app_hours(1),
        )
    });
    rules.push(Rule {
        clear_threshold: Some(88.0),
        unit: "%".to_string(),
        ..app_rule(
            "frigate_storage_almost_full",
            "Frigate storage almost full",
            "The disk holding Frigate's recordings is more than 90% full.",
            "dumbmonit_frigate_storage_used_percent",
            Operator::Gt,
            90.0,
            app_minutes(30),
            Severity::Warning,
            app_hours(24),
        )
    });
    rules.push(app_rule(
        "frigate_update_available",
        "Frigate update available",
        "A newer Frigate release is available.",
        "dumbmonit_frigate_update_available",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Info,
        app_hours(7 * 24),
    ));

    // Keycloak.
    rules.push(app_rule(
        "keycloak_not_ready",
        "Keycloak not ready",
        "Keycloak answers but reports itself not ready (database down): nobody can log in.",
        "dumbmonit_keycloak_ready == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(5),
        Severity::Critical,
        app_hours(1),
    ));
    rules.push(app_rule(
        "keycloak_realm_disabled",
        "Keycloak realm disabled",
        "A Keycloak realm is disabled: its users cannot log in.",
        "dumbmonit_keycloak_realms_disabled",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Info,
        app_hours(7 * 24),
    ));

    // authentik.
    rules.push(app_rule(
        "authentik_not_ready",
        "authentik not ready",
        "authentik answers but reports itself not ready (database or cache down): logins fail.",
        "dumbmonit_authentik_ready == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(5),
        Severity::Critical,
        app_hours(1),
    ));
    rules.push(app_rule(
        "authentik_no_worker",
        "authentik has no worker",
        "No authentik worker is running: source synchronisation, e-mails and clean-up tasks \
         stop.",
        "dumbmonit_authentik_workers == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(10),
        Severity::Critical,
        app_hours(1),
    ));
    rules.push(app_rule(
        "authentik_tasks_failed",
        "authentik task failed",
        "A scheduled system task (a source synchronisation, a blueprint) failed on its last \
         run.",
        "dumbmonit_authentik_tasks_failed",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Warning,
        app_hours(24),
    ));
    rules.push(app_rule(
        "authentik_outpost_disconnected",
        "authentik outpost disconnected",
        "An outpost that serves providers has not reported for five minutes: the applications \
         it protects do not answer.",
        "dumbmonit_authentik_outposts_disconnected",
        Operator::Gt,
        0.0,
        app_minutes(10),
        Severity::Warning,
        app_hours(6),
    ));
    rules.push(app_rule(
        "authentik_update_available",
        "authentik update available",
        "A newer authentik release is available.",
        "dumbmonit_authentik_update_available",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Info,
        app_hours(7 * 24),
    ));

    // Vaultwarden.
    rules.push(app_rule(
        "vaultwarden_signups_open",
        "Vaultwarden sign-ups open",
        "Anyone who can reach this Vaultwarden can create an account (SIGNUPS_ALLOWED).",
        "dumbmonit_vaultwarden_signups_open",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Info,
        app_hours(7 * 24),
    ));

    // mailcow.
    rules.push(app_rule(
        "mailcow_container_down",
        "mailcow container down",
        "A mailcow container is not running (Postfix, Dovecot, rspamd…): mail is not delivered, \
         read or filtered.",
        "dumbmonit_mailcow_container_up == bool 0",
        Operator::Gt,
        0.0,
        app_minutes(5),
        Severity::Critical,
        app_hours(1),
    ));
    rules.push(Rule {
        unit: "h".to_string(),
        ..app_rule(
            "mailcow_queue_stuck",
            "mailcow mail queue stuck",
            "The oldest message in Postfix's queue has waited more than four hours.",
            "dumbmonit_mailcow_queue_oldest_seconds / 3600",
            Operator::Gt,
            4.0,
            app_minutes(15),
            Severity::Warning,
            app_hours(6),
        )
    });
    rules.push(Rule {
        clear_threshold: Some(88.0),
        unit: "%".to_string(),
        ..app_rule(
            "mailcow_vmail_almost_full",
            "mailcow mail volume almost full",
            "The volume holding mailcow's mailboxes is more than 90% full.",
            "dumbmonit_mailcow_vmail_used_percent",
            Operator::Gt,
            90.0,
            app_minutes(30),
            Severity::Warning,
            app_hours(24),
        )
    });
    rules.push(app_rule(
        "mailcow_mailbox_near_quota",
        "mailcow mailbox near quota",
        "A mailbox is at 90% or more of its quota: incoming mail will soon be refused.",
        "dumbmonit_mailcow_mailboxes_near_quota",
        Operator::Gt,
        0.0,
        app_hours(1),
        Severity::Info,
        app_hours(24),
    ));
    rules
}
