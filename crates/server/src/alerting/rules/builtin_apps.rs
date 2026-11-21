//! Règles livrées des applications auto-hébergées (Nextcloud, Immich,
//! Paperless, Jellyfin, Plex, GitLab, Forgejo), des résolveurs DNS, des
//! onduleurs NUT, de MikroTik, d'UniFi, de Home Assistant et de vSphere.

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // --- Applications auto-hébergées (`collectors/selfhosted`) ---
        //
        // Ces applications tombent de côté : la page s'ouvre, et ce qui compte
        // (la synchronisation, les miniatures, l'import de documents, le scan
        // de bibliothèque) ne se fait plus. Les règles visent ces états-là ;
        // l'application qui ne répond plus du tout relève de « injoignable ».
        Rule {
            description: "Nextcloud has been in maintenance mode for half an hour: nobody can \
                          log in or sync."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            escalate_after: Some(Duration::from_secs(3 * 3600)),
            ..base(
                "nextcloud_maintenance",
                "Nextcloud stuck in maintenance",
                RuleKind::Threshold,
                "dumbmonit_nextcloud_maintenance",
            )
        },
        // Après une mise à jour du code, Nextcloud refuse tout tant que
        // `occ upgrade` n'a pas tourné.
        Rule {
            description: "Nextcloud waits for occ upgrade after an update and serves nothing \
                          until someone runs it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "nextcloud_upgrade_pending",
                "Nextcloud database upgrade pending",
                RuleKind::Threshold,
                "dumbmonit_nextcloud_needs_db_upgrade",
            )
        },
        Rule {
            description: "A newer Nextcloud release is available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "nextcloud_update_available",
                "Nextcloud update available",
                RuleKind::Threshold,
                "dumbmonit_nextcloud_update_available",
            )
        },
        // OPcache plein : PHP recompile chaque script, tout devient lent.
        Rule {
            description: "PHP OPcache on the Nextcloud server is full: pages get slow. Raise \
                          opcache.memory_consumption."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "nextcloud_opcache_full",
                "Nextcloud OPcache full",
                RuleKind::Threshold,
                "dumbmonit_nextcloud_opcache_full",
            )
        },
        // Nextcloud ne donne que l'espace libre du dossier de données, pas sa
        // taille : un seuil absolu, en Gio.
        Rule {
            description: "Less than 5 GiB free for Nextcloud's data: uploads and syncs will \
                          start failing."
                .to_string(),
            operator: Operator::Lt,
            threshold: 5.0,
            clear_threshold: Some(6.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "GiB".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "nextcloud_free_space_low",
                "Nextcloud free space low",
                RuleKind::Threshold,
                "dumbmonit_nextcloud_free_space_bytes / 1073741824",
            )
        },
        // Immich : des travaux en attente dans une file qui n'est pas en
        // pause et n'en exécute aucun. Une heure couvre les creux entre deux
        // lots d'un gros import.
        Rule {
            description: "Immich jobs have waited for an hour in a queue that is not paused and \
                          runs nothing: new photos get no thumbnail or metadata."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "immich_jobs_stalled",
                "Immich jobs stalled",
                RuleKind::Threshold,
                "dumbmonit_immich_queue_waiting > 0 \
                 and on (target, queue) dumbmonit_immich_queue_active == 0 \
                 unless on (target, queue) dumbmonit_immich_queue_paused == 1",
            )
        },
        Rule {
            description: "An Immich job queue has been paused for a day.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(24 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "immich_queue_paused",
                "Immich queue paused",
                RuleKind::Threshold,
                "dumbmonit_immich_queue_paused",
            )
        },
        Rule {
            description: "The disk holding Immich's library is more than 90% full.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "immich_storage_almost_full",
                "Immich storage almost full",
                RuleKind::Threshold,
                "dumbmonit_immich_storage_used_percent",
            )
        },
        Rule {
            description: "A newer Immich release is available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "immich_update_available",
                "Immich update available",
                RuleKind::Threshold,
                "dumbmonit_immich_update_available",
            )
        },
        // Paperless : 0 OK, 1 avertissement, 2 erreur. Sans Redis ni Celery,
        // plus rien n'est importé.
        Rule {
            description: "Paperless-ngx cannot reach Redis or Celery: nothing new is imported, \
                          from the consume folder or from mail."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "paperless_broker_down",
                "Paperless task queue down",
                RuleKind::Threshold,
                "max by (target) (dumbmonit_paperless_component_status{component=~\"redis|celery\"})",
            )
        },
        // Le contrôle d'intégrité en erreur, c'est un fichier manquant ou
        // dont la somme ne correspond plus : à voir avant la prochaine sauvegarde.
        Rule {
            description: "Paperless-ngx reports an error in its search index, classifier or \
                          sanity check (missing or altered files)."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "paperless_component_error",
                "Paperless index or sanity check error",
                RuleKind::Threshold,
                "dumbmonit_paperless_component_status{component=~\"index|classifier|sanity_check\"}",
            )
        },
        Rule {
            description: "Paperless-ngx has database migrations left to apply: run them before \
                          it misbehaves."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "paperless_migrations_pending",
                "Paperless migrations pending",
                RuleKind::Threshold,
                "dumbmonit_paperless_unapplied_migrations",
            )
        },
        Rule {
            description: "A Paperless-ngx task (an import, a scheduled job) failed recently and \
                          was not dismissed."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "paperless_task_failed",
                "Paperless task failed",
                RuleKind::Threshold,
                "dumbmonit_paperless_tasks_failed_recent",
            )
        },
        Rule {
            description: "The disk holding Paperless-ngx's documents is more than 90% full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "paperless_storage_almost_full",
                "Paperless storage almost full",
                RuleKind::Threshold,
                "dumbmonit_paperless_storage_used_percent",
            )
        },
        Rule {
            description: "A newer Paperless-ngx release is available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "paperless_update_available",
                "Paperless update available",
                RuleKind::Threshold,
                "dumbmonit_paperless_update_available",
            )
        },
        // Jellyfin : le scan de bibliothèque en échec est le cas typique.
        Rule {
            description: "A Jellyfin scheduled task failed on its last run (a library scan, a \
                          cleanup)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "jellyfin_task_failed",
                "Jellyfin scheduled task failed",
                RuleKind::Threshold,
                "dumbmonit_jellyfin_scheduled_tasks_failed",
            )
        },
        Rule {
            description: "A Jellyfin plugin failed to load or does not support this version."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "jellyfin_plugin_broken",
                "Jellyfin plugin broken",
                RuleKind::Threshold,
                "dumbmonit_jellyfin_plugins_broken",
            )
        },
        Rule {
            description: "Jellyfin has waited a day for a restart, after a plugin install or \
                          update."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(24 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "jellyfin_restart_pending",
                "Jellyfin restart pending",
                RuleKind::Threshold,
                "dumbmonit_jellyfin_pending_restart",
            )
        },
        Rule {
            description: "A newer Plex Media Server release is available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "plex_update_available",
                "Plex update available",
                RuleKind::Threshold,
                "dumbmonit_plex_update_available",
            )
        },
        // GitLab (`collectors/selfhosted/gitlab.rs`) : l'indisponibilité pure
        // relève déjà de « Équipement injoignable », la version lue par le
        // premier appel suffisant à faire échouer la sonde si l'instance ne
        // répond plus.
        Rule {
            description: "Every CI/CD runner GitLab knows is offline: no pipeline can run.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "gitlab_runners_all_offline",
                "GitLab runners all offline",
                RuleKind::Threshold,
                "dumbmonit_gitlab_runners_online and on (target) dumbmonit_gitlab_runners_total > 0",
            )
        },
        Rule {
            description: "A Sidekiq queue has over 500 jobs waiting: notifications, webhooks and imports fall behind.".to_string(),
            operator: Operator::Gt,
            threshold: 500.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "gitlab_sidekiq_backlog_high",
                "GitLab Sidekiq backlog high",
                RuleKind::Threshold,
                "max by (target) (dumbmonit_gitlab_sidekiq_queue_backlog)",
            )
        },
        Rule {
            description: "GitLab has database migrations left to apply after an upgrade.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "gitlab_migrations_pending",
                "GitLab migrations pending",
                RuleKind::Threshold,
                "dumbmonit_gitlab_migrations_pending",
            )
        },
        Rule {
            description: "The GitLab license expires in less than 30 days.".to_string(),
            operator: Operator::Lt,
            threshold: 30.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "gitlab_license_expiring",
                "GitLab license expiring",
                RuleKind::Threshold,
                "dumbmonit_gitlab_license_expiry_seconds",
            )
        },
        // Forgejo et Gitea (`collectors/selfhosted/forgejo.rs`).
        Rule {
            description: "Every Actions runner Forgejo or Gitea knows is offline: no workflow can run.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "forgejo_runners_all_offline",
                "Forgejo/Gitea runners all offline",
                RuleKind::Threshold,
                "dumbmonit_forgejo_runners_online and on (target) dumbmonit_forgejo_runners_total > 0",
            )
        },
        Rule {
            description: "A scheduled task (repository check, cleanup, update checker) did not run at its own interval.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "forgejo_cron_task_overdue",
                "Forgejo/Gitea scheduled task overdue",
                RuleKind::Threshold,
                "dumbmonit_forgejo_cron_task_overdue",
            )
        },
        // --- fin du bloc applications auto-hébergées ---

        // Appareils clients (`collectors/client_devices.rs`) : la même règle
        // sert tous les produits qui suivent des machines qui se connectent et
        // sauvegardent — Immich, Proxmox Backup Server (un appareil par groupe
        // de sauvegarde) et Veeam (un appareil par objet protégé, lu sur
        // `/api/v1/restorePoints`) aujourd'hui ; Tailscale, UniFi… demain.
        // Synology Active Backup for Business suit ses appareils par un
        // mécanisme propre, plus riche (rythme appris par appareil, voir
        // `synology::rhythm`), antérieur à celui-ci et non dupliqué ici. Le
        // collecteur calcule déjà l'écart au seuil propre à la cible
        // (`device_stale_days`, 3 jours par défaut) : la règle n'a donc qu'un
        // seuil fixe à zéro, identique pour tout le monde. L'étiquette
        // `device` qui accompagne la série nomme l'appareil dans l'alerte.
        Rule {
            description: "A client device (phone, tablet, desktop…) has not connected or not \
                          backed up within the staleness window configured for this target \
                          (`device_stale_days`, three days by default)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "client_device_stale",
                "Client device stale",
                RuleKind::Threshold,
                "dumbmonit_client_device_stale_seconds",
            )
        },

        // --- Pi-hole (`collectors/pihole`) ---
        //
        // Le blocage se suspend d'un clic pour cinq minutes ou une heure :
        // quinze minutes laissent passer la pause courte, pas l'oubli.
        Rule {
            description: "Pi-hole has not been blocking for fifteen minutes: ads and trackers go \
                          through."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            ..base(
                "pihole_blocking_disabled",
                "Pi-hole blocking disabled",
                RuleKind::Threshold,
                "dumbmonit_pihole_blocking_enabled",
            )
        },
        // Gravity se reconstruit chaque semaine par défaut : huit jours
        // laissent passer une semaine normale, pas deux mises à jour manquées.
        Rule {
            description: "Pi-hole's blocklists have not been rebuilt for more than eight days: \
                          the weekly gravity update fails."
                .to_string(),
            operator: Operator::Gt,
            threshold: 8.0 * 86_400.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pihole_gravity_stale",
                "Pi-hole blocklists stale",
                RuleKind::Threshold,
                "dumbmonit_pihole_gravity_age_seconds",
            )
        },
        Rule {
            description: "A newer Pi-hole is available (the container image, or core, web \
                          interface or FTL outside a container)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pihole_update_available",
                "Pi-hole update available",
                RuleKind::Threshold,
                "dumbmonit_pihole_updates_available",
            )
        },
        Rule {
            description: "Pi-hole lists messages in its diagnosis page: a blocklist that failed \
                          to download, a client rate-limited, a host overloaded."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pihole_diagnosis_messages",
                "Pi-hole diagnosis messages",
                RuleKind::Threshold,
                "dumbmonit_pihole_messages",
            )
        },
        // --- AdGuard Home (`collectors/adguard`) ---
        //
        // La protection coupée : « désactiver pour une heure » oublié, ou
        // désactivée tout court. Quinze minutes laissent passer la pause de
        // dix minutes qu'on prend pour débloquer un site.
        Rule {
            description: "AdGuard Home's protection has been off for fifteen minutes: nothing is \
                          filtered."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "adguard_protection_disabled",
                "AdGuard Home protection off",
                RuleKind::Threshold,
                "dumbmonit_adguard_protection_enabled",
            )
        },
        // Le serveur DNS arrêté alors que l'interface répond : plus aucune
        // résolution pour les clients qui n'ont que lui.
        Rule {
            description: "AdGuard Home's DNS server is not running: clients that use it cannot \
                          resolve names."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "adguard_dns_not_running",
                "AdGuard Home DNS not running",
                RuleKind::Threshold,
                "dumbmonit_adguard_running",
            )
        },
        // Un amont qui échoue au test d'AdGuard Home, ou reste muet quatre
        // secondes. Dix minutes écartent une perte de paquet isolée.
        Rule {
            description: "An upstream DNS server of AdGuard Home fails its test: queries sent to \
                          it fail or wait."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "adguard_upstream_down",
                "AdGuard Home upstream failing",
                RuleKind::Threshold,
                "dumbmonit_adguard_upstream_up",
            )
        },
        // Les listes se mettent à jour toutes les 24 h par défaut : trois
        // jours sans mise à jour, c'est que le téléchargement échoue.
        Rule {
            description: "An enabled AdGuard Home filter list has not been updated for three \
                          days: blocking falls behind."
                .to_string(),
            operator: Operator::Gt,
            threshold: 3.0 * 86_400.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "adguard_filters_stale",
                "AdGuard Home filter lists stale",
                RuleKind::Threshold,
                "dumbmonit_adguard_filter_oldest_update_age_seconds",
            )
        },
        Rule {
            description: "A newer AdGuard Home version is available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "adguard_update_available",
                "AdGuard Home update available",
                RuleKind::Threshold,
                "dumbmonit_adguard_update_available",
            )
        },
        // --- Onduleurs derrière NUT (`collectors/nut`) ---
        //
        // Les séries portent `ups` : la notification dit quel onduleur d'un
        // même serveur NUT. Les drapeaux de `ups.status` valent 0 ou 1 et sont
        // émis à chaque lecture : les règles retombent d'elles-mêmes au retour
        // du secteur. Mêmes délais et gravités que les règles SNMP de l'UPS-MIB.
        Rule {
            description: "The UPS is powering the load from battery.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(900)),
            ..base(
                "nut_on_battery",
                "UPS on battery (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_ups_on_battery",
            )
        },
        // `LB` : le seuil de l'onduleur ou de NUT (`battery.charge.low`,
        // `battery.runtime.low`) est franchi, upsmon déclenche l'arrêt.
        Rule {
            description: "The UPS reports a low battery: the machines it powers are about to \
                          shut down."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(1800)),
            ..base(
                "nut_battery_low",
                "UPS battery low (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_ups_low_battery",
            )
        },
        // `RB` : l'onduleur a jugé la batterie usée, souvent après un autotest.
        Rule {
            description: "The UPS asks for its battery to be replaced: it may not hold the load \
                          through the next outage."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "nut_replace_battery",
                "UPS battery needs replacing (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_ups_replace_battery",
            )
        },
        // Au-delà de 80 %, l'autonomie fond et un pic de démarrage peut
        // déclencher la surcharge.
        Rule {
            description: "The UPS load has stayed above 80% for ten minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            clear_threshold: Some(75.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "nut_load_high",
                "UPS load high (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_ups_load_percent",
            )
        },
        // L'autonomie estimée : sur secteur, moins de cinq minutes dit une
        // batterie usée ou un onduleur trop petit pour sa charge.
        Rule {
            description: "The UPS estimates less than five minutes of runtime on battery."
                .to_string(),
            operator: Operator::Lt,
            threshold: 300.0,
            clear_threshold: Some(360.0),
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "nut_runtime_short",
                "UPS runtime short (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_battery_runtime_seconds",
            )
        },
        // `upsd` répond mais n'a plus de nouvelles de l'onduleur : câble USB,
        // pilote arrêté. Pendant ce temps, une coupure passerait inaperçue.
        Rule {
            description: "The NUT server has no fresh data from the UPS: its cable, driver or \
                          the UPS itself stopped answering, and an outage would go unseen."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            escalate_after: Some(Duration::from_secs(3600)),
            ..base(
                "nut_data_stale",
                "UPS data stale (NUT)",
                RuleKind::Threshold,
                "dumbmonit_nut_ups_data_stale",
            )
        },
        // --- MikroTik RouterOS (`collectors/mikrotik`) ---
        //
        // Les séries d'interface portent `interface` et `type`, celles de
        // capteur `sensor` : la notification dit « ether1 » ou « psu2-state ».
        Rule {
            description: "The router's CPU has been above 90% for fifteen minutes: routing, \
                          the firewall or a script is saturating it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mikrotik_cpu_high",
                "MikroTik CPU high",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_cpu_load_percent",
            )
        },
        // Une fuite (connexions suivies, listes d'adresses gonflées par un
        // script) monte lentement : quinze minutes au-dessus de 90 %.
        Rule {
            description: "More than 90% of the router's memory is in use.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mikrotik_memory_high",
                "MikroTik memory high",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_memory_used_percent",
            )
        },
        // Le stockage flash d'un hAP fait 16 Mo : des fichiers de sauvegarde ou
        // un paquet téléchargé suffisent à le remplir, et la configuration ne
        // s'enregistre plus.
        Rule {
            description: "The router's storage is more than 90% full: configuration changes \
                          and upgrades may fail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "mikrotik_storage_almost_full",
                "MikroTik storage almost full",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_storage_used_percent",
            )
        },
        // Les processeurs des CCR tournent couramment entre 50 et 70 °C ;
        // quatre-vingts degrés tenus dix minutes, c'est un ventilateur ou une
        // armoire qui ne refroidit plus.
        Rule {
            description: "A temperature sensor of the router is above 80 °C.".to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            clear_threshold: Some(75.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mikrotik_temperature_high",
                "MikroTik temperature high",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_temperature_celsius",
            )
        },
        // `psu1-state`, `fan-state` : tout ce qui n'est pas « ok ». Une
        // alimentation redondante morte ne coupe rien — jusqu'à la seconde.
        Rule {
            description: "The router reports a failed power supply or fan.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "mikrotik_hardware_failed",
                "MikroTik power supply or fan failed",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_health_ok",
            )
        },
        // Même seuil que la règle IF-MIB : cinquante erreurs dans l'heure.
        Rule {
            description: "An interface of the router logged more than fifty errors in the last \
                          hour: usually a failing cable, a dirty fibre or a duplex mismatch."
                .to_string(),
            operator: Operator::Gt,
            threshold: 50.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "mikrotik_interface_errors",
                "MikroTik interface accumulating errors",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_mikrotik_interface_rx_errors_total[1h]) \
                 + increase_prometheus(dumbmonit_mikrotik_interface_tx_errors_total[1h])",
            )
        },
        // `link-downs` compte les pertes de lien : plus de deux en trente
        // minutes, c'est un lien qui bagote. Un port débranché pour de bon ne
        // compte qu'une fois, et ne sonne donc pas.
        Rule {
            description: "An interface of the router lost its link more than twice in thirty \
                          minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "mikrotik_link_flapping",
                "MikroTik link flapping",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_mikrotik_interface_link_downs_total[30m])",
            )
        },
        // Seulement si le routeur vérifie lui-même (planificateur de la notice) :
        // un compte en lecture ne peut pas lancer la vérification.
        Rule {
            description: "A newer RouterOS version is available on the router's update channel."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "mikrotik_update_available",
                "MikroTik RouterOS update available",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_update_available",
            )
        },
        // RouterBOOT ne suit pas RouterOS tout seul : après une mise à jour, il
        // faut `/system routerboard upgrade` puis un redémarrage.
        Rule {
            description: "The RouterBOOT firmware is older than the one bundled with the \
                          installed RouterOS: it is upgraded only on request, then at the next \
                          reboot."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "mikrotik_firmware_upgrade_pending",
                "MikroTik firmware upgrade pending",
                RuleKind::Threshold,
                "dumbmonit_mikrotik_firmware_upgrade_pending",
            )
        },
        // --- UniFi Network (`collectors/unifi`) ---
        //
        // Les séries d'équipement portent `device`, `model` et `type` : la
        // notification dit « Living room AP ». Un équipement en attente
        // d'adoption n'a pas de `device_up` : il n'est pas encore à nous.
        Rule {
            description: "A UniFi device adopted by the controller is offline, isolated or failed \
                          to adopt."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            escalate_after: Some(Duration::from_secs(3600)),
            ..base("unifi_device_offline", "UniFi device offline", RuleKind::Threshold, "dumbmonit_unifi_device_up")
        },
        // `www` en erreur : la passerelle ne joint plus Internet. Absent sans
        // passerelle UniFi, la règle ne dit alors rien.
        Rule {
            description: "The UniFi gateway reports no Internet connection.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "unifi_internet_down",
                "UniFi Internet down",
                RuleKind::Threshold,
                "dumbmonit_unifi_internet_up",
            )
        },
        // Un second lien WAN tombé ne coupe rien, et c'est justement pourquoi
        // il faut le dire : la redondance est perdue sans que personne le voie.
        Rule {
            description: "A WAN link of the UniFi gateway is down.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "unifi_wan_link_down",
                "UniFi WAN link down",
                RuleKind::Threshold,
                "dumbmonit_unifi_wan_link_up",
            )
        },
        Rule {
            description: "The UniFi gateway's processor has been above 90% for fifteen minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            ..base(
                "unifi_gateway_cpu_high",
                "UniFi gateway CPU high",
                RuleKind::Threshold,
                "dumbmonit_unifi_device_cpu_percent{type=\"gateway\"}",
            )
        },
        Rule {
            description: "A device appeared on the network and waits to be adopted in UniFi."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "unifi_device_pending_adoption",
                "UniFi device waiting for adoption",
                RuleKind::Threshold,
                "dumbmonit_unifi_devices{state=\"pending\"}",
            )
        },
        Rule {
            description: "A firmware update is available for UniFi devices.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "unifi_firmware_update",
                "UniFi firmware update available",
                RuleKind::Threshold,
                "dumbmonit_unifi_devices_upgradable",
            )
        },
        Rule {
            description: "UniFi logged critical events or raised alarms in the last 24 hours."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("unifi_alarms", "UniFi alarms", RuleKind::Threshold, "dumbmonit_unifi_alarms")
        },
        // --- Home Assistant (`collectors/homeassistant`) ---
        Rule {
            description: "Home Assistant started in recovery mode: its configuration could not be \
                          loaded and automations do not run."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "homeassistant_recovery_mode",
                "Home Assistant in recovery mode",
                RuleKind::Threshold,
                "dumbmonit_homeassistant_recovery_mode",
            )
        },
        // Démarrage bloqué : le cœur répond mais reste en `STARTING` ou `NOT_RUNNING`.
        Rule {
            description: "Home Assistant answers but has not finished starting for ten minutes."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            ..base(
                "homeassistant_not_running",
                "Home Assistant not running",
                RuleKind::Threshold,
                "dumbmonit_homeassistant_running",
            )
        },
        // Une intégration qui décroche fait passer d'un coup toutes ses entités
        // `unavailable` : c'est la hausse qui compte, pas le total, qu'une
        // installation réelle n'a jamais à zéro.
        Rule {
            description: "At least five more entities are unavailable than an hour ago: an \
                          integration, a hub or a radio stopped."
                .to_string(),
            operator: Operator::Ge,
            threshold: 5.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "homeassistant_entities_unavailable",
                "Home Assistant entities went unavailable",
                RuleKind::Threshold,
                "sum by (target) (dumbmonit_homeassistant_entities_unavailable) \
                 - sum by (target) (dumbmonit_homeassistant_entities_unavailable offset 1h)",
            )
        },
        // Une série par pile faible, valeur = niveau (0 pour un capteur binaire) :
        // toute série présente déclenche.
        Rule {
            description: "A battery reported by Home Assistant is low.".to_string(),
            operator: Operator::Ge,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(3 * 24 * 3600)),
            unit: "%".to_string(),
            ..base(
                "homeassistant_battery_low",
                "Home Assistant battery low",
                RuleKind::Threshold,
                "dumbmonit_homeassistant_battery_low",
            )
        },
        Rule {
            description: "Home Assistant has updates waiting: core, operating system, add-ons or \
                          device firmware."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "homeassistant_update_available",
                "Home Assistant update available",
                RuleKind::Threshold,
                "dumbmonit_homeassistant_updates_available",
            )
        },
        Rule {
            description: "Home Assistant has an open repair of error or critical severity."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "homeassistant_repair",
                "Home Assistant repair to address",
                RuleKind::Threshold,
                "dumbmonit_homeassistant_repair{severity=~\"error|critical\"}",
            )
        },
        // --- VMware vSphere (`collectors/vsphere`) ---
        //
        // `host_connection_state` : 2 « ne répond plus ». Un hôte déconnecté par
        // un administrateur (1) est un choix, pas une panne.
        Rule {
            description: "A vSphere host is not responding to vCenter.".to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "vsphere_host_not_responding",
                "vSphere host not responding",
                RuleKind::Threshold,
                "dumbmonit_vsphere_host_connection_state",
            )
        },
        // `overallStatus` : 2 rouge. Le gris (3) dit « inconnu », pas « en panne ».
        Rule {
            description: "vSphere reports a host's overall health as red.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "vsphere_host_red",
                "vSphere host health red",
                RuleKind::Threshold,
                "dumbmonit_vsphere_host_status == 2",
            )
        },
        Rule {
            description: "A vSphere host has been in maintenance mode for a day: its VMs were \
                          moved away and it may have been forgotten there."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(24 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "vsphere_host_maintenance",
                "vSphere host left in maintenance",
                RuleKind::Threshold,
                "dumbmonit_vsphere_host_maintenance",
            )
        },
        Rule {
            description: "A vSphere datastore is 90% full or more.".to_string(),
            operator: Operator::Ge,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "vsphere_datastore_almost_full",
                "vSphere datastore almost full",
                RuleKind::Threshold,
                "dumbmonit_vsphere_datastore_used_percent",
            )
        },
        Rule {
            description: "A vSphere datastore is inaccessible: the VMs stored on it cannot run."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "vsphere_datastore_inaccessible",
                "vSphere datastore inaccessible",
                RuleKind::Threshold,
                "dumbmonit_vsphere_datastore_accessible",
            )
        },
        // 2 : outils installés mais arrêtés dans une VM allumée — souvent une
        // VM figée. Absents (3) : un choix, pas une alerte.
        Rule {
            description: "VMware Tools are installed but not running in a powered-on VM: the \
                          guest may be hung."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "vsphere_vm_tools_not_running",
                "vSphere VM tools not running",
                RuleKind::Threshold,
                "dumbmonit_vsphere_vm_tools_status == 2",
            )
        },
        // Une série par alarme rouge non acquittée, nommée par l'alarme et l'objet.
        Rule {
            description: "vSphere raised a red alarm that nobody acknowledged.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "vsphere_alarm_red",
                "vSphere red alarm",
                RuleKind::Threshold,
                "dumbmonit_vsphere_alarm{status=\"red\"}",
            )
        },
    ]
}
