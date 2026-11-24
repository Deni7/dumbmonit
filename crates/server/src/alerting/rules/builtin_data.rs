//! Règles livrées des invités Proxmox VE, de Synology DSM, des bases de
//! données et des files de messages.

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // --- Proxmox VE : invités, disques, ZFS, paquets (`collectors/proxmox`) ---
        //
        // Les séries d'invité portent `name` et `vmid` : la notification dit
        // « nextcloud (202) ». Celles de disque portent `node` et `disk`, celles
        // de pool `node` et `pool` : « pve1 · /dev/sda », « pve2 · tank ».
        //
        // Processeur d'un invité : le pourcentage est celui des cœurs alloués,
        // une machine à 100 % n'a que ses propres cœurs à saturer. Quinze
        // minutes écartent les pics de démarrage et de sauvegarde.
        Rule {
            description:
                "A VM or container has used more than 90% of its allocated cores for fifteen minutes."
                    .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_guest_cpu_high",
                "VM or container CPU high",
                RuleKind::Threshold,
                "dumbmonit_proxmox_guest_cpu_percent",
            )
        },
        // Mémoire d'un invité : PVE mesure la mémoire consommée vue de l'hôte,
        // qui plafonne naturellement près de 100 % avec un ballon ou un cache
        // de fichiers actif — d'où 95 % et dix minutes.
        Rule {
            description: "A VM or container has used more than 95% of its memory for ten minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 95.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_guest_memory_high",
                "VM or container memory high",
                RuleKind::Threshold,
                "dumbmonit_proxmox_guest_memory_percent",
            )
        },
        // Disque racine d'un invité : connu pour un conteneur, et pour une
        // machine virtuelle dont l'agent QEMU répond. Sans mesure, pas de série
        // et donc pas d'alerte — jamais un faux « 0 % ».
        Rule {
            description: "The root disk of a VM or container is more than 90% full.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_guest_disk_almost_full",
                "VM or container disk almost full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_guest_disk_used_percent",
            )
        },
        // Usure d'un SSD ou NVMe : 0 % neuf, 100 % en fin de vie d'après le
        // constructeur. À 90 %, il est temps de commander le remplaçant.
        Rule {
            description: "An SSD or NVMe disk of a node has used more than 90% of its rated life."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pve_disk_wearout",
                "Proxmox disk wearing out",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_disk_wearout_percent",
            )
        },
        // SMART en échec : le disque lui-même se déclare en fin de vie.
        Rule {
            description: "A disk of a Proxmox node reports a failed SMART health check.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_disk_smart_failed",
                "Proxmox disk SMART failure",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_disk_smart_failed",
            )
        },
        // Pool ZFS dégradé : un disque manque, la redondance est consommée.
        Rule {
            description: "A ZFS pool of a Proxmox node is not ONLINE: a disk is missing or faulted."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_zfs_pool_degraded",
                "Proxmox ZFS pool degraded",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_zfs_pool_degraded",
            )
        },
        // Correctifs de sécurité : un seul suffit, contrairement au décompte
        // général des mises à jour qui attend vingt paquets.
        Rule {
            description: "Security updates are pending on this Proxmox node.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pve_security_updates_pending",
                "Proxmox security updates pending",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_updates_security_pending",
            )
        },
        // Paquets mis à jour : quelqu'un a passé `apt upgrade` sur le nœud. Une
        // information, pas une panne — mais celle qui explique le redémarrage
        // ou le comportement changé qu'on constate ensuite. La série reste
        // publiée une heure avec le détail en étiquette `changes`, puis se
        // résout seule.
        Rule {
            description: "Installed Proxmox packages changed since the previous probe.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_packages_changed",
                "Proxmox packages changed",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_packages_changed",
            )
        },
        // --- fin du bloc Proxmox VE : invités, disques, ZFS, paquets ---
        // --- Synology DSM : stockage, disques, mémoire et rythme Active Backup
        // (`collectors/synology/{metrics,devices,rhythm}.rs`) ---
        //
        // Les états de DSM sont publiés sur une échelle commune : 0 normal, 1 à
        // surveiller, 2 critique — et l'inconnu vaut 1, jamais 0. Une règle sur
        // « > 0 » attrape donc aussi un état inédit, ce qui est voulu : mieux vaut
        // un avertissement qu'un disque en panne sous un libellé nouveau.
        Rule {
            description: "The SMART status of this disk is no longer \"normal\", as reported \
                          by DSM."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_disk_smart_warning",
                "Synology disk SMART warning",
                RuleKind::Threshold,
                "dumbmonit_synology_disk_smart_status",
            )
        },
        // Un disque « crashed » a déjà quitté la grappe : c'est une panne, pas un
        // avertissement.
        Rule {
            description: "DSM reports this disk as crashed or failed.".to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "synology_disk_failed",
                "Synology disk failed",
                RuleKind::Threshold,
                "dumbmonit_synology_disk_status",
            )
        },
        Rule {
            description: "The bad-sector count of this disk exceeds the threshold set in DSM."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_disk_bad_sectors",
                "Synology disk bad sectors",
                RuleKind::Threshold,
                "dumbmonit_synology_disk_bad_sector_exceeded",
            )
        },
        // Des secteurs illisibles qui apparaissent sont le signe le plus précoce
        // d'une panne : `delta` sur une jauge compare la valeur d'il y a un jour à
        // celle d'aujourd'hui. Un disque remplacé repart de zéro, ce qui donne une
        // variation négative et ne déclenche pas.
        Rule {
            description: "New unreadable (UNC) sectors appeared on this disk in the last \
                          24 hours."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_disk_bad_sectors_growing",
                "Synology disk bad sectors growing",
                RuleKind::Threshold,
                "delta(dumbmonit_synology_disk_unc_count[24h])",
            )
        },
        Rule {
            description: "This SSD has less than 10% of its rated life left.".to_string(),
            operator: Operator::Lt,
            threshold: 10.0,
            clear_threshold: Some(12.0),
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "synology_ssd_wearout",
                "Synology SSD wearing out",
                RuleKind::Threshold,
                "dumbmonit_synology_disk_remaining_life_percent",
            )
        },
        // Un volume dégradé fonctionne encore, sans redondance : la prochaine
        // panne est celle qui perd les données. Même seuil pour « crashed ».
        Rule {
            description: "This volume is degraded or crashed: its redundancy is gone.".to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "synology_volume_degraded",
                "Synology volume degraded",
                RuleKind::Threshold,
                "dumbmonit_synology_volume_status",
            )
        },
        // Un groupe de stockage se dégrade avant le volume posé dessus : sur un
        // RAID 5 qui perd un disque, DSM laisse le volume en « normal » tant que
        // la reconstruction tient. Sans cette règle, la perte de redondance ne se
        // voit qu'au second disque — c'est-à-dire trop tard.
        Rule {
            description: "This storage pool is degraded or crashed: a disk failed and the \
                          redundancy is gone or rebuilding."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "synology_pool_degraded",
                "Synology storage pool degraded",
                RuleKind::Threshold,
                "dumbmonit_synology_pool_status or dumbmonit_synology_ssd_cache_status",
            )
        },
        // L'occupation effective, sans les réservations des LUN iSCSI épais : un
        // volume rempli par un LUN épais est plein par construction, et seul ce
        // qui reste à côté peut déborder (`collectors/synology/iscsi.rs`). Sans
        // LUN, elle vaut l'occupation brute ; la brute ne sert que de repli avant
        // la première collecte d'un collecteur à jour.
        Rule {
            description: "Volume 90% full or more, not counting the space reserved by \
                          thick-provisioned iSCSI LUNs."
                .to_string(),
            operator: Operator::Ge,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_volume_almost_full",
                "Synology volume almost full",
                RuleKind::Threshold,
                "dumbmonit_synology_volume_effective_used_percent \
                 or dumbmonit_synology_volume_used_percent",
            )
        },
        // Sur-allocation fine : les LUN fins du volume ont reçu la promesse de
        // plus d'espace qu'il n'en reste de libre. La valeur est l'espace promis
        // et pas encore écrit, en pourcentage de l'espace libre : à 100 %, le
        // prochain octet écrit dans un LUN peut ne plus trouver de place, et
        // l'hôte iSCSI voit alors son disque passer en lecture seule.
        Rule {
            description: "Thin over-allocation: the thin iSCSI LUNs of this volume were \
                          promised more space than the volume still has free (value = \
                          promised and unwritten space, as a percentage of the free space)."
                .to_string(),
            operator: Operator::Ge,
            threshold: 100.0,
            clear_threshold: Some(95.0),
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_thin_overcommit",
                "Synology thin LUN over-allocation",
                RuleKind::Threshold,
                "dumbmonit_synology_volume_thin_overcommit_percent",
            )
        },
        // Un LUN fin dont l'allocation approche sa taille. Un LUN épais n'a pas
        // de série : son remplissage se lit chez l'équipement qui l'utilise
        // (datastore, disque d'un agent), dont les règles alertent déjà.
        Rule {
            description: "This thin iSCSI LUN has 95% or more of its size written.".to_string(),
            operator: Operator::Ge,
            threshold: 95.0,
            clear_threshold: Some(93.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_lun_almost_full",
                "Synology thin LUN almost full",
                RuleKind::Threshold,
                "dumbmonit_synology_lun_used_percent",
            )
        },
        // 55 °C est le seuil à partir duquel les fabricants de disques mécaniques
        // et Synology parlent de surchauffe ; l'hystérésis évite les allers-retours
        // d'un disque qui oscille autour du seuil pendant une reconstruction.
        Rule {
            description: "A disk of this NAS is above 55 °C, or DSM raised its temperature \
                          warning."
                .to_string(),
            operator: Operator::Gt,
            threshold: 55.0,
            clear_threshold: Some(52.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "synology_temperature_high",
                "Synology temperature high",
                RuleKind::Threshold,
                // Le drapeau de DSM vaut 0 ou 1 : multiplié par 100, il dépasse le
                // seuil à lui seul quand le NAS lui-même se déclare en surchauffe.
                "dumbmonit_synology_disk_temperature_celsius \
                 or (100 * dumbmonit_synology_temperature_warning > 55)",
            )
        },
        Rule {
            description: "Memory usage of the NAS above 95% for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 95.0,
            clear_threshold: Some(90.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "synology_memory_high",
                "Synology memory high",
                RuleKind::Threshold,
                "dumbmonit_synology_memory_usage_percent",
            )
        },
        // Rythme Active Backup par appareil : la série vaut 1 seulement quand le
        // modèle juge l'appareil en retard *par rapport à ses propres habitudes*
        // (jours de repos exclus, tolérance dérivée de son intervalle habituel).
        // Une demi-heure de `for` laisse passer une relecture de l'historique.
        // Rien à régler ici (unité `bool`) : l'âge fixe, « plus de N jours », est
        // la règle `synology_abb_device_backup_too_old`.
        Rule {
            description: "Compared with this device's usual rhythm: fires when it is late by \
                          its own standard (median gap between successful runs, off-days \
                          excluded). Nothing to set; for a fixed limit in days, use \"Active \
                          Backup device backup too old\"."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_abb_device_overdue",
                "Active Backup device overdue",
                RuleKind::Threshold,
                "dumbmonit_abb_device_overdue",
            )
        },
        // Deux tentatives échouées d'affilée : ce n'est plus un portable refermé
        // au mauvais moment, c'est un appareil qui ne se sauvegarde plus.
        Rule {
            description: "The last two or more Active Backup for Business attempts of this \
                          device failed."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "synology_abb_device_failing",
                "Active Backup device failing",
                RuleKind::Threshold,
                "dumbmonit_abb_device_consecutive_failures",
            )
        },
        // --- Redis / Valkey (`collectors/redis`) ---
        // Au plafond, Redis évince ou refuse les écritures selon la politique ;
        // sans `maxmemory`, la série n'existe pas et la règle se tait.
        Rule {
            description: "Redis uses more than 90% of maxmemory: at the limit it evicts keys or refuses every write, depending on maxmemory-policy."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("redis_memory_near_limit", "Redis memory near maxmemory", RuleKind::Threshold, "dumbmonit_redis_memory_used_percent")
        },
        Rule {
            description: "Redis refused client connections in the last 15 minutes: maxclients is reached."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("redis_rejecting_connections", "Redis refusing connections", RuleKind::Threshold, "increase_prometheus(dumbmonit_redis_rejected_connections_total[15m])")
        },
        Rule {
            description: "This Redis replica lost its link to the primary: it serves stale data and will not take over cleanly."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("redis_replication_broken", "Redis replication link down", RuleKind::Threshold, "dumbmonit_redis_master_link_up")
        },
        // Vu du primaire : secondes depuis le dernier acquittement d'une réplique,
        // qui en envoie un par seconde.
        Rule {
            description: "A Redis replica has not acknowledged the replication stream for more than 30 seconds."
                .to_string(),
            operator: Operator::Gt,
            threshold: 30.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("redis_replica_lagging", "Redis replica lagging", RuleKind::Threshold, "dumbmonit_redis_replica_lag_seconds")
        },
        Rule {
            description: "The last Redis snapshot (BGSAVE) failed: with stop-writes-on-bgsave-error, Redis now refuses every write."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("redis_save_failed", "Redis snapshot failed", RuleKind::Threshold, "dumbmonit_redis_rdb_last_save_ok")
        },
        Rule {
            description: "Redis could not write its append-only file: recent writes are not on disk."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("redis_aof_write_failed", "Redis append-only file write failed", RuleKind::Threshold, "dumbmonit_redis_aof_last_write_ok")
        },
        // --- MongoDB (`collectors/mongodb`) ---
        Rule {
            description: "The MongoDB replica set has no primary: no write is accepted until one is elected."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("mongodb_no_primary", "MongoDB replica set without primary", RuleKind::Threshold, "dumbmonit_mongodb_replset_primary_present")
        },
        Rule {
            description: "A member of the MongoDB replica set is unreachable from the others."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("mongodb_member_unhealthy", "MongoDB member unreachable", RuleKind::Threshold, "dumbmonit_mongodb_replset_member_health")
        },
        Rule {
            description: "A MongoDB secondary is more than a minute behind the primary: if it is elected, the latest writes are missing."
                .to_string(),
            operator: Operator::Gt,
            threshold: 60.0,
            clear_threshold: Some(30.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("mongodb_replication_lag", "MongoDB replication lag", RuleKind::Threshold, "dumbmonit_mongodb_replset_member_lag_seconds")
        },
        Rule {
            description: "MongoDB uses more than 80% of its available connections: new clients will be refused."
                .to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            clear_threshold: Some(75.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("mongodb_connections_high", "MongoDB connections near the limit", RuleKind::Threshold, "dumbmonit_mongodb_connections_used_percent")
        },
        // 20 % : le seuil `eviction_dirty_trigger` de WiredTiger, au-delà
        // duquel les threads applicatifs évincent eux-mêmes.
        Rule {
            description: "More than 20% of the WiredTiger cache is dirty: application threads have to evict pages themselves and queries slow down."
                .to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            clear_threshold: Some(15.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("mongodb_cache_dirty", "MongoDB cache under pressure", RuleKind::Threshold, "dumbmonit_mongodb_wiredtiger_cache_dirty_percent")
        },
        Rule {
            description: "MongoDB raised regular assertions in the last hour: internal server errors, worth a look in its log."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("mongodb_assertions", "MongoDB internal errors", RuleKind::Threshold, "increase_prometheus(dumbmonit_mongodb_asserts_total{type=\"regular\"}[1h])")
        },
        // --- RabbitMQ (`collectors/rabbitmq`) ---
        Rule {
            description: "A RabbitMQ node raised a memory or disk alarm: every publisher in the cluster is blocked."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("rabbitmq_resource_alarm", "RabbitMQ memory or disk alarm", RuleKind::Threshold, "dumbmonit_rabbitmq_alarm")
        },
        Rule {
            description: "A RabbitMQ cluster node is not running."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("rabbitmq_node_down", "RabbitMQ node down", RuleKind::Threshold, "dumbmonit_rabbitmq_node_running")
        },
        Rule {
            description: "RabbitMQ nodes see a network partition: the cluster is split and queues may diverge."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("rabbitmq_partition", "RabbitMQ network partition", RuleKind::Threshold, "dumbmonit_rabbitmq_node_partitions")
        },
        // Des messages en attente et personne pour les lire : le service qui
        // vidait la file est tombé. La valeur est le nombre de messages.
        Rule {
            description: "A RabbitMQ queue holds messages and has had no consumer for 15 minutes: whatever read it has stopped."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("rabbitmq_queue_no_consumer", "RabbitMQ queue without consumer", RuleKind::Threshold, "dumbmonit_rabbitmq_queue_messages_ready and dumbmonit_rabbitmq_queue_consumers == 0")
        },
        Rule {
            description: "A RabbitMQ queue is not running: a quorum queue without majority, or a queue whose node is down."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("rabbitmq_queue_not_running", "RabbitMQ queue unavailable", RuleKind::Threshold, "dumbmonit_rabbitmq_queue_running")
        },
        // --- SQL Server, Oracle, Elasticsearch, InfluxDB, Kafka ---
        Rule {
            description: "A database is SUSPECT, RECOVERY_PENDING, OFFLINE or EMERGENCY."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(300),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("mssql_database_unavailable", "SQL Server database unavailable", RuleKind::Threshold, "dumbmonit_mssql_database_online")
        },
        Rule {
            description: "A request has been waiting on a lock for more than 5 minutes: a transaction was probably left open."
                .to_string(),
            operator: Operator::Gt,
            threshold: 300.0,
            for_duration: Duration::from_secs(300),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("mssql_blocking", "SQL Server blocked requests", RuleKind::Threshold, "dumbmonit_mssql_longest_block_seconds")
        },
        Rule {
            description: "A volume holding database files is more than 90 % full; clears below 85 %."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("mssql_volume_almost_full", "SQL Server volume almost full", RuleKind::Threshold, "dumbmonit_mssql_volume_used_percent")
        },
        Rule {
            description: "The last run of an enabled SQL Server Agent job failed (backups, maintenance)."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(300),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(86400)),
            ..base("mssql_agent_job_failed", "SQL Server Agent job failed", RuleKind::Threshold, "dumbmonit_mssql_agent_job_last_ok")
        },
        Rule {
            description: "A database replica of an availability group is not synchronizing in good health."
                .to_string(),
            operator: Operator::Lt,
            threshold: 2.0,
            for_duration: Duration::from_secs(600),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("mssql_alwayson_unhealthy", "SQL Server AlwaysOn replica unhealthy", RuleKind::Threshold, "dumbmonit_mssql_alwayson_sync_health")
        },
        Rule {
            description: "User sessions use more than 90 % of the configured connection limit. No series without a limit."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("mssql_sessions_high", "SQL Server connections near the limit", RuleKind::Threshold, "dumbmonit_mssql_sessions_used_percent")
        },
        Rule {
            description: "The instance status is not OPEN (MOUNTED, STARTED...): the database serves nothing."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(120),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("oracle_instance_not_open", "Oracle instance not open", RuleKind::Threshold, "dumbmonit_oracle_instance_open")
        },
        Rule {
            description: "A tablespace is more than 90 % full of its maximum size; clears below 85 %."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("oracle_tablespace_almost_full", "Oracle tablespace almost full", RuleKind::Threshold, "dumbmonit_oracle_tablespace_used_percent")
        },
        Rule {
            description: "Sessions use more than 90 % of the sessions parameter (ORA-00018 when it is reached)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("oracle_sessions_near_limit", "Oracle sessions near the limit", RuleKind::Threshold, "dumbmonit_oracle_sessions_used_percent")
        },
        Rule {
            description: "A standby applies redo more than 10 minutes behind the primary."
                .to_string(),
            operator: Operator::Gt,
            threshold: 600.0,
            for_duration: Duration::from_secs(600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("oracle_dataguard_lag", "Oracle Data Guard apply lag", RuleKind::Threshold, "dumbmonit_oracle_dataguard_apply_lag_seconds")
        },
        Rule {
            description: "The fast recovery area is more than 90 % full: archiving can stall and freeze the database."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(600),
            severity: Severity::Critical,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("oracle_recovery_area_full", "Oracle recovery area almost full", RuleKind::Threshold, "dumbmonit_oracle_recovery_area_used_percent")
        },
        Rule {
            description: "At least one scheduler job run failed in the last 24 hours."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(86400)),
            ..base("oracle_scheduler_job_failed", "Oracle scheduler job failed", RuleKind::Threshold, "dumbmonit_oracle_scheduler_jobs_failed_24h")
        },
        Rule {
            description: "The cluster status is red: at least one primary shard is not allocated and part of the data is unavailable."
                .to_string(),
            operator: Operator::Gt,
            threshold: 1.0,
            for_duration: Duration::from_secs(120),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("elasticsearch_cluster_red", "Elasticsearch cluster red", RuleKind::Threshold, "dumbmonit_elasticsearch_cluster_status")
        },
        Rule {
            description: "The cluster has been yellow for 30 minutes: replicas are missing and the next node failure will lose data."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(1800),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("elasticsearch_cluster_yellow", "Elasticsearch cluster yellow", RuleKind::Threshold, "dumbmonit_elasticsearch_cluster_status == 1")
        },
        Rule {
            description: "Shards have stayed unassigned for 15 minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("elasticsearch_unassigned_shards", "Elasticsearch unassigned shards", RuleKind::Threshold, "dumbmonit_elasticsearch_unassigned_shards")
        },
        Rule {
            description: "A node's data disk is more than 85 % full (the high watermark is 90 %, the read-only flood stage 95 %); clears below 80 %."
                .to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("elasticsearch_disk_almost_full", "Elasticsearch node disk almost full", RuleKind::Threshold, "dumbmonit_elasticsearch_node_disk_used_percent")
        },
        Rule {
            description: "A node's JVM heap stays above 90 % for 15 minutes: garbage collection will slow the node."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("elasticsearch_heap_high", "Elasticsearch JVM heap pressure", RuleKind::Threshold, "dumbmonit_elasticsearch_node_heap_used_percent")
        },
        Rule {
            description: "InfluxDB answers its health check with a failure."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(120),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("influxdb_unhealthy", "InfluxDB unhealthy", RuleKind::Threshold, "dumbmonit_influxdb_healthy")
        },
        Rule {
            description: "InfluxDB is up but does not report itself ready for queries and writes."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(300),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("influxdb_not_ready", "InfluxDB not ready", RuleKind::Threshold, "dumbmonit_influxdb_ready")
        },
        Rule {
            description: "More than 10 API requests answered with a 5xx status in the last 15 minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 10.0,
            for_duration: Duration::from_secs(300),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("influxdb_request_errors", "InfluxDB server errors", RuleKind::Threshold, "increase_prometheus(dumbmonit_influxdb_http_requests_5xx_total[15m])")
        },
        Rule {
            description: "A partition has no leader: reads and writes to it fail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(120),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("kafka_offline_partitions", "Kafka partitions offline", RuleKind::Threshold, "dumbmonit_kafka_offline_partitions")
        },
        Rule {
            description: "Partitions have fewer in-sync replicas than replicas for 10 minutes: a broker is down or lagging."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("kafka_under_replicated", "Kafka under-replicated partitions", RuleKind::Threshold, "dumbmonit_kafka_under_replicated_partitions")
        },
        Rule {
            description: "The cluster reports no controller: no leader election is possible."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(120),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("kafka_no_controller", "Kafka without controller", RuleKind::Threshold, "dumbmonit_kafka_controller_present")
        },
        Rule {
            description: "A consumer group is more than 10 000 messages behind for 15 minutes. Adjust the threshold to the traffic of your topics."
                .to_string(),
            operator: Operator::Gt,
            threshold: 10000.0,
            for_duration: Duration::from_secs(900),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(21600)),
            ..base("kafka_consumer_lag", "Kafka consumer group lag", RuleKind::Threshold, "dumbmonit_kafka_consumer_group_lag_total")
        },
    ]
}
