//! Règles livrées de Proxmox VE, Proxmox Backup Server, Proxmox Datacenter
//! Manager et Proxmox Mail Gateway (complétées par `pve.rs` et `pbs.rs`).

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // Proxmox VE, au-delà des sauvegardes. Sévérités : « warning » de la
        // ligne de produit = `Critical` ici, « advisory » = `Warning`.
        //
        // Machine arrêtée alors qu'elle tournait. `1 - running` vaut 1 pour une
        // machine arrêtée ; le `and` ne retient que celles vues en marche dans les
        // deux dernières heures, pour ne pas alerter sur une machine arrêtée de
        // longue date. Une comparaison MetricsQL renvoie la valeur de gauche, pas
        // un booléen : `running == 0` donnerait 0, invisible d'un seuil « > 0 ».
        Rule {
            description: "The VM or container was running and has been stopped for five minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_guest_stopped",
                "VM or container stopped",
                RuleKind::Threshold,
                "(1 - dumbmonit_proxmox_guest_running) \
                 and (max_over_time(dumbmonit_proxmox_guest_running[2h]) == 1)",
            )
        },
        // Haute disponibilité : une ressource en `error` ou `fence` ne redémarrera
        // pas toute seule, c'est une intervention.
        Rule {
            description: "A high-availability resource is in error or fenced.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "pve_ha_resource_error",
                "HA resource in error",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ha_resource_error",
            )
        },
        // Quorum perdu : plus aucune machine ne peut démarrer sur le cluster, et la
        // HA arrête celles qui tournent.
        Rule {
            description: "The cluster has lost quorum: guests can no longer be started."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(1800)),
            ..base(
                "pve_cluster_no_quorum",
                "Cluster lost quorum",
                RuleKind::Threshold,
                "1 - dumbmonit_proxmox_cluster_quorate",
            )
        },
        // Nœud hors ligne vu du cluster. Distinct de « équipement injoignable » :
        // l'API répond, c'est un membre qui manque.
        Rule {
            description: "A cluster node has been offline for two minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(1800)),
            ..base(
                "pve_node_offline",
                "Proxmox node offline",
                RuleKind::Threshold,
                "1 - dumbmonit_proxmox_node_up",
            )
        },
        // Stockage à 85 % : un cran avant « Disk almost full » (90 %), qui reste
        // l'alerte sérieuse. Un LVM-thin plein bloque toutes les machines qu'il
        // héberge, il vaut mieux prévenir tôt.
        Rule {
            description: "A Proxmox storage is more than 85% full.".to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_storage_almost_full",
                "Proxmox storage almost full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_storage_used_percent",
            )
        },
        // Dernier travail de sauvegarde du nœud en échec (tâches `vzdump`).
        Rule {
            description: "The last backup job on this node failed.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_backup_job_failed",
                "Backup job failed",
                RuleKind::Threshold,
                "1 - dumbmonit_proxmox_backup_job_last_ok",
            )
        },
        // Instantané oublié : il grossit avec le temps et ralentit la machine, et
        // personne ne s'en souvient. Information, pas panne.
        Rule {
            description: "The oldest snapshot of this machine is more than thirty days old."
                .to_string(),
            operator: Operator::Gt,
            threshold: 30.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pve_snapshot_old",
                "Old snapshot",
                RuleKind::Threshold,
                "dumbmonit_proxmox_guest_snapshot_oldest_age_seconds",
            )
        },
        // Réplication en échec : la copie de secours de la machine n'est plus à
        // jour, la bascule restaurerait un état ancien.
        Rule {
            description: "A replication job is failing: the standby copy is stale.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_replication_failed",
                "Replication failed",
                RuleKind::Threshold,
                "dumbmonit_proxmox_replication_job_error",
            )
        },
        // Ceph : 0 OK, 1 WARN, 2 ERR. Les deux règles sont disjointes (`>= 2` et
        // `== 1`) pour qu'un HEALTH_ERR ne déclenche pas aussi l'avertissement.
        Rule {
            description: "Ceph reports HEALTH_ERR: data may be unavailable.".to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(1800)),
            ..base(
                "pve_ceph_health_error",
                "Ceph health error",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ceph_health",
            )
        },
        Rule {
            description: "Ceph reports HEALTH_WARN for more than fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_ceph_health_warning",
                "Ceph health warning",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ceph_health == 1",
            )
        },
        // Mises à jour en attente : information, une fois par semaine suffit.
        Rule {
            description: "More than twenty package updates are pending on this node.".to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pve_updates_pending",
                "Proxmox updates pending",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_updates_pending",
            )
        },
        // Certificat de l'interface d'un nœud : PVE renouvelle les siens lui-même,
        // pas ceux importés à la main.
        Rule {
            description: "A node certificate expires in less than fourteen days.".to_string(),
            operator: Operator::Lt,
            threshold: 14.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "d".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_certificate_expiring",
                "Node certificate expiring",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_certificate_expiry_days",
            )
        },
        // Pool à provisionnement fin saturé : toutes les machines qu'il héberge
        // basculent en lecture seule d'un coup, et un pool plein ne se vide pas
        // en supprimant des fichiers dans les machines. Critique, et plus tôt
        // que pour un stockage ordinaire.
        Rule {
            description: "An LVM thin pool is more than 90% full: its guests will go read-only."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Critical,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_thinpool_almost_full",
                "Thin pool almost full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_thinpool_used_percent",
            )
        },
        // Les métadonnées d'un pool fin tiennent sur un volume minuscule et
        // saturent souvent avant les données — avec exactement les mêmes
        // conséquences, pour une cause que personne ne pense à regarder.
        Rule {
            description: "The metadata volume of an LVM thin pool is more than 80% full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Critical,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_thinpool_metadata_full",
                "Thin pool metadata almost full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_thinpool_metadata_used_percent",
            )
        },
        // Un démon essentiel arrêté est la panne la plus traître de Proxmox :
        // sans `pvestatd`, l'interface continue d'afficher les chiffres du
        // moment où il s'est arrêté, et tout a l'air normal.
        Rule {
            description: "A core Proxmox service is not running on this node.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "pve_service_down",
                "Proxmox service down",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_core_services_down",
            )
        },
        // Pont ou agrégat déclaré au démarrage qui n'est pas monté : toutes les
        // machines qui s'y rattachent sont coupées du réseau, et elles-mêmes
        // tournent parfaitement.
        Rule {
            description: "A network interface set to start at boot is down on this node."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pve_interface_offline",
                "Node network interface down",
                RuleKind::Threshold,
                "dumbmonit_proxmox_node_interface_offline",
            )
        },
        // OSD tombé : Ceph recopie ailleurs, le cluster reste utilisable, mais
        // la redondance fond et le prochain incident coûte des données.
        Rule {
            description: "A Ceph OSD has been down for five minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "pve_ceph_osd_down",
                "Ceph OSD down",
                RuleKind::Threshold,
                "1 - dumbmonit_proxmox_ceph_osd_up",
            )
        },
        // Un seul OSD à 85 % suffit à bloquer les écritures de tous les pools
        // qui s'appuient dessus : la moyenne du cluster ne dit rien de ce
        // risque-là.
        Rule {
            description: "A Ceph OSD is more than 85% full: writes stop when it reaches its limit."
                .to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            clear_threshold: Some(82.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_ceph_osd_nearly_full",
                "Ceph OSD nearly full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ceph_osd_used_percent",
            )
        },
        Rule {
            description: "A Ceph pool is more than 85% full.".to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            clear_threshold: Some(82.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_ceph_pool_almost_full",
                "Ceph pool almost full",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ceph_pool_used_percent",
            )
        },
        // `noout` posé le temps d'un redémarrage puis oublié : Ceph ne sortira
        // plus jamais un OSD mort du cluster, et la santé reste au vert pendant
        // que la redondance disparaît. Deux heures, c'est plus long que toute
        // maintenance normale.
        Rule {
            description: "The Ceph noout flag has been set for two hours: rebalancing is disabled."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_ceph_noout_set",
                "Ceph noout flag left on",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ceph_flag{flag=\"noout\"}",
            )
        },
        // Un LRM qui n'écrit plus n'exécute plus rien : les machines en haute
        // disponibilité de ce nœud ne seront ni relancées ni déplacées, et rien
        // d'autre ne le dit.
        Rule {
            description: "The HA local resource manager of a node has stopped reporting."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "pve_ha_lrm_stale",
                "HA manager not reporting",
                RuleKind::Threshold,
                "dumbmonit_proxmox_ha_lrm_stale",
            )
        },
        // Sauvegarde qui réussit tous les soirs sans rien contenir : un disque
        // porte `backup=0` et personne ne le sait avant d'avoir à restaurer.
        // Les exclusions normales (lecteur de CD, cloudinit) ne comptent pas.
        Rule {
            description: "A backup job skips a disk of one of the guests it covers.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pve_backup_excludes_disk",
                "Backup job excludes a disk",
                RuleKind::Threshold,
                "dumbmonit_proxmox_backup_job_guest_excluded_volumes",
            )
        },
        // Verrou oublié : une sauvegarde interrompue laisse la machine
        // verrouillée, et plus rien ne peut être fait dessus — pas même la
        // sauvegarde du lendemain. Six heures dépassent toute opération normale.
        Rule {
            description: "A guest has been locked for six hours: no operation can run on it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(6 * 3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pve_guest_locked",
                "Guest locked",
                RuleKind::Threshold,
                "dumbmonit_proxmox_guest_locked",
            )
        },
        // Proxmox Backup Server : synchronisation vers un site distant en échec.
        // La série vaut 1 (dernier passage réussi) ou 0 ; « < 1 » ne vise que les
        // échecs, comme pour la vérification.
        Rule {
            description: "The last run of this PBS sync job failed.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_sync_failed",
                "PBS sync job failed",
                RuleKind::Threshold,
                "dumbmonit_pbs_sync_job_last_ok",
            )
        },
        Rule {
            description: "More than twenty package updates are pending on the backup server."
                .to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pbs_updates_pending",
                "PBS updates pending",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_updates_pending",
            )
        },
        // --- Proxmox Backup Server : travaux, GC, disques (`collectors/pbs`) ---
        //
        // Un travail de purge ou de vérification en échec n'empêche pas les
        // sauvegardes de tourner : c'est justement pourquoi personne ne le voit
        // avant que le datastore soit plein ou qu'une restauration échoue. Les
        // séries `job_last_ok` valent 1 ou 0 et n'existent pas pour un travail
        // qui n'a jamais tourné ; « < 1 » ne vise donc que les vrais échecs. Le
        // `for` de dix minutes absorbe une lecture prise pendant la relance.
        Rule {
            description: "The last run of this PBS prune job failed: nothing is pruned and the \
                          datastore keeps filling up."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_prune_failed",
                "PBS prune job failed",
                RuleKind::Threshold,
                "dumbmonit_pbs_job_last_ok{kind=\"prune\"}",
            )
        },
        Rule {
            description: "The last run of this PBS verification job failed: at least one \
                          snapshot could not be verified."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_verify_job_failed",
                "PBS verification job failed",
                RuleKind::Threshold,
                "dumbmonit_pbs_job_last_ok{kind=\"verify\"}",
            )
        },
        // `gc_last_run_ok` vient de `/gc` (PBS 3.3+) ou, avant, de la GC la plus
        // récente de la fenêtre de tâches.
        Rule {
            description: "The last garbage collection on this PBS datastore failed: freed \
                          space is not reclaimed."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_gc_failed",
                "PBS garbage collection failed",
                RuleKind::Threshold,
                "dumbmonit_pbs_gc_last_run_ok",
            )
        },
        // Disques du serveur de sauvegarde, mêmes séries que pour un nœud PVE :
        // un SMART en échec est le dernier avertissement avant de perdre le
        // datastore avec les sauvegardes qu'il porte. La série n'existe que si
        // SMART a rendu un verdict.
        Rule {
            description: "A disk of the backup server reports SMART health FAILED.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_disk_smart_failed",
                "PBS disk SMART failure",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_disk_smart_failed",
            )
        },
        Rule {
            description: "This SSD of the backup server has used more than 90% of its rated \
                          endurance."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pbs_disk_wearout",
                "PBS SSD worn out",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_disk_wearout_percent",
            )
        },
        // `zfs_pool_degraded` vaut 0 pour ONLINE ; DEGRADED, FAULTED, OFFLINE
        // ou UNAVAIL donnent 1. Un pool dégradé fonctionne encore : c'est le
        // moment de remplacer le disque, pas après le second.
        Rule {
            description: "A ZFS pool of the backup server is not ONLINE (degraded, faulted or \
                          unavailable)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pbs_zpool_degraded",
                "PBS ZFS pool degraded",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_zfs_pool_degraded",
            )
        },
        // Une unité arrêtée sur le serveur de sauvegarde : sans le mandataire,
        // plus aucune sauvegarde n'entre, et l'interface reste pourtant
        // joignable tant que l'API tourne. La série porte `expected = "1"` pour
        // les seules unités indispensables ; les autres — postfix, un agent de
        // journalisation — produisent leur série sans réveiller personne.
        Rule {
            description: "A service that Proxmox Backup Server needs is not running: no backup \
                          can be taken in while it is down."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pbs_service_down",
                "PBS service down",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_service_active{expected=\"1\"}",
            )
        },
        // Le certificat de l'interface. La série n'existe que si l'option est
        // ouverte : PBS garde cette lecture derrière un privilège d'écriture.
        // Trois semaines laissent le temps de renouveler à la main un
        // certificat qu'ACME ne gère pas.
        Rule {
            description: "The certificate served by the backup server expires in less than \
                          three weeks."
                .to_string(),
            operator: Operator::Lt,
            threshold: 21.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(48 * 3600)),
            ..base(
                "pbs_certificate_expiring",
                "PBS certificate expiring",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_certificate_expires_seconds",
            )
        },
        // Le paquet a été mis à niveau, le démon tourne encore sur l'ancien
        // code. Rien ne casse tout de suite, et c'est le problème : le correctif
        // que l'on croit appliqué ne l'est pas.
        Rule {
            description: "The Proxmox Backup Server package has been upgraded but the running \
                          daemon is still the old one: restart its services to apply it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(6 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pbs_version_stale",
                "PBS restart pending after upgrade",
                RuleKind::Threshold,
                "dumbmonit_pbs_node_running_version_stale",
            )
        },
        // Une bande est la copie que rien en ligne ne peut atteindre, et
        // personne ne regarde une bandothèque : un travail en échec y reste des
        // semaines. `last_ok` vaut 1 ou 0, et n'existe pas avant le premier
        // passage.
        Rule {
            description: "The last tape backup run failed: the offline copy is not being made."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_tape_job_failed",
                "PBS tape backup failed",
                RuleKind::Threshold,
                "dumbmonit_pbs_tape_backup_job_last_ok",
            )
        },
        // Un travail activé, planifié, et qui n'a jamais tourné : ni échec ni
        // succès, donc invisible partout ailleurs. Deux jours laissent passer un
        // travail créé hier soir pour une exécution hebdomadaire.
        Rule {
            description: "This PBS job is enabled and scheduled but has never run: check the \
                          schedule, or that the service that triggers it is up."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 24 * 3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pbs_job_never_run",
                "PBS job never ran",
                RuleKind::Threshold,
                "dumbmonit_pbs_job_never_run",
            )
        },
        // Des chunks que la GC a trouvés illisibles et laissés en place. Ce
        // n'est pas de la place à reprendre : ce sont des sauvegardes que l'on
        // ne pourra pas restaurer.
        Rule {
            description: "The garbage collection found unreadable chunks on this datastore: \
                          some backups can no longer be restored."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_gc_bad_chunks",
                "PBS corrupt chunks found",
                RuleKind::Threshold,
                "dumbmonit_pbs_gc_bad_chunks",
            )
        },
        // Un datastore amovible débranché ne rend pas d'erreur : il est
        // simplement absent, et la sauvegarde de ce soir n'aura pas lieu. Six
        // heures laissent passer un disque de rotation que l'on emporte la
        // journée.
        Rule {
            description: "A removable datastore of the backup server is not mounted: nothing \
                          can be written to it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(6 * 3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pbs_datastore_unmounted",
                "PBS datastore not mounted",
                RuleKind::Threshold,
                "dumbmonit_pbs_datastore_removable_unmounted",
            )
        },
        // --- fin du bloc Proxmox Backup Server ---
        // --- Proxmox Datacenter Manager : parc fédéré (`collectors/pdm`) ---
        //
        // Une console de datacenter ne se surveille pas pour elle-même : elle se
        // surveille pour les instances qu'elle fédère. La série vaut 1 (la console
        // a obtenu une réponse) ou 0, et porte `remote` et `type` : la notification
        // dit « site-b (pve) ».
        Rule {
            description: "The datacenter console cannot reach this Proxmox VE cluster or backup \
                          server."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pdm_remote_unreachable",
                "Federated instance unreachable",
                RuleKind::Threshold,
                "dumbmonit_pdm_remote_reachable",
            )
        },
        // La série n'existe que pour une instance dont la version a été lue, et ne
        // compare que des instances du même produit entre elles : c'est le cluster
        // que l'on a oublié de mettre à jour, pas un retard sur l'amont.
        Rule {
            description: "This instance runs an older version than another instance of the same \
                          product in the estate."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pdm_remote_version_behind",
                "Federated instance behind",
                RuleKind::Threshold,
                "dumbmonit_pdm_remote_version_behind",
            )
        },
        Rule {
            description: "At least one task failed on this federated instance in the review \
                          window."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pdm_task_failed",
                "Task failed on a federated instance",
                RuleKind::Threshold,
                "dumbmonit_pdm_remote_tasks_failed",
            )
        },
        // Le disque racine de la console porte son cache de métriques : plein, la
        // console cesse d'enregistrer ce qu'elle voit et n'a plus rien à montrer.
        Rule {
            description: "The root filesystem of the datacenter console is more than 90% full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pdm_node_disk_almost_full",
                "Datacenter console disk almost full",
                RuleKind::Threshold,
                "dumbmonit_pdm_node_rootfs_percent",
            )
        },
        Rule {
            description: "A certificate of the datacenter console expires in less than fourteen \
                          days."
                .to_string(),
            operator: Operator::Lt,
            threshold: 14.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "d".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pdm_certificate_expiring",
                "Datacenter console certificate expiring",
                RuleKind::Threshold,
                "dumbmonit_pdm_node_certificate_expiry_days",
            )
        },
        Rule {
            description: "More than twenty package updates are pending on the datacenter console."
                .to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pdm_updates_pending",
                "Datacenter console updates pending",
                RuleKind::Threshold,
                "dumbmonit_pdm_node_updates_pending",
            )
        },
        // --- fin du bloc Proxmox Datacenter Manager ---
        // --- Proxmox Mail Gateway : files, filtrage, signatures (`collectors/pmg`) ---
        //
        // Une passerelle de messagerie tombe rarement d'un coup : elle filtre de
        // moins en moins bien, ou elle garde le courrier sans le dire. Les règles
        // qui suivent visent ces pannes silencieuses avant la panne franche.
        //
        // La file différée est le premier symptôme d'un relais aval injoignable :
        // elle grossit alors qu'aucune série ne bouge par ailleurs. On mesure la
        // croissance et non le niveau, parce qu'une passerelle chargée garde en
        // permanence quelques dizaines de messages différés sans que rien n'aille
        // mal.
        Rule {
            description: "The deferred mail queue has grown by more than a hundred messages in \
                          two hours: the next hop is probably refusing mail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pmg_queue_growing",
                "Mail queue growing",
                RuleKind::Threshold,
                "delta(dumbmonit_pmg_queue_messages{queue=\"deferred\"}[2h])",
            )
        },
        // `queue_oldest_age_seconds` est un minorant : `qshape` ne donne que des
        // tranches, et on retient la borne basse de la plus haute tranche occupée.
        // Quatre heures dépassent largement les retentatives normales de Postfix.
        Rule {
            description: "A message has been waiting in this queue for more than four hours: mail \
                          is stuck, not slow."
                .to_string(),
            operator: Operator::Gt,
            threshold: 4.0 * 3600.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Critical,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pmg_queue_stuck",
                "Mail stuck in the queue",
                RuleKind::Threshold,
                "dumbmonit_pmg_queue_oldest_age_seconds",
            )
        },
        // Les unités qui font le travail : sans `pmg-smtp-filter`, Postfix accepte
        // le courrier et ne le filtre plus ; sans `postfix`, il ne l'accepte même
        // plus. La série vaut 1 ou 0 et n'existe pas pour une unité non installée :
        // « < 1 » ne vise que les vrais arrêts. Le `for` absorbe un redémarrage.
        Rule {
            description: "A service the gateway needs to accept and filter mail is stopped."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pmg_filter_service_down",
                "Mail gateway service stopped",
                RuleKind::Threshold,
                "dumbmonit_pmg_service_running{service=~\"postfix|pmg-smtp-filter|pmgpolicy|pmgproxy|pmgdaemon\"}",
            )
        },
        // Signatures antivirus : ClamAV publie plusieurs fois par jour. Deux jours
        // sans mise à jour veut dire que `freshclam` ne tourne plus — la passerelle
        // continue de filtrer, avec les virus de l'avant-veille. La règle ne vise
        // que la base quotidienne : `main` date de plusieurs mois par construction.
        Rule {
            description: "The ClamAV daily virus signatures are more than two days old: freshclam \
                          has stopped updating them."
                .to_string(),
            operator: Operator::Gt,
            threshold: 2.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pmg_virus_signatures_stale",
                "Virus signatures out of date",
                RuleKind::Threshold,
                "dumbmonit_pmg_signature_age_seconds{family=\"virus\",database=\"daily\"}",
            )
        },
        // Règles antispam : `sa-update` publie environ une fois par semaine. Huit
        // jours laissent passer une publication décalée sans crier. La série
        // n'existe pas pour un canal jamais daté, donc un canal secondaire
        // inactif ne réveille personne.
        Rule {
            description: "The SpamAssassin rules are more than eight days old: sa-update has \
                          stopped bringing new ones."
                .to_string(),
            operator: Operator::Gt,
            threshold: 8.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pmg_spam_rules_stale",
                "Spam rules out of date",
                RuleKind::Threshold,
                "dumbmonit_pmg_signature_age_seconds{family=\"spam\"}",
            )
        },
        // Quarantaine : c'est la croissance qui alerte, pas la taille. Une
        // quarantaine bien réglée garde des milliers de messages en régime
        // normal ; mille de plus en six heures signale une campagne, ou une règle
        // qui vient de basculer tout un domaine légitime en indésirable.
        Rule {
            description: "The quarantine has taken in more than a thousand messages in six \
                          hours: a campaign, or a rule that just went wrong."
                .to_string(),
            operator: Operator::Gt,
            threshold: 1000.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "pmg_quarantine_growing",
                "Quarantine filling up",
                RuleKind::Threshold,
                "delta(dumbmonit_pmg_quarantine_messages{kind=\"spam\"}[6h])",
            )
        },
        // Grappe : `cluster_node_insync` vaut 0 quand la base de règles d'un nœud
        // n'a pas reçu les dernières modifications. Un nœud désynchronisé filtre
        // avec des règles périmées, sans rien signaler de lui-même. La série
        // n'existe pas sur une installation autonome.
        Rule {
            description: "A gateway of the cluster is no longer in sync with the others: it \
                          filters with an out-of-date rule database."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pmg_cluster_degraded",
                "Mail gateway cluster degraded",
                RuleKind::Threshold,
                "dumbmonit_pmg_cluster_node_insync",
            )
        },
        // Le certificat que sert l'interface — et, avec la même paire, le TLS
        // entrant. Quatorze jours laissent le temps de renouveler à la main un
        // certificat qu'ACME n'a pas repris.
        Rule {
            description: "A certificate of the mail gateway expires in less than fourteen days."
                .to_string(),
            operator: Operator::Lt,
            threshold: 14.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pmg_certificate_expiring",
                "Mail gateway certificate expiring",
                RuleKind::Threshold,
                "dumbmonit_pmg_certificate_expires_in_seconds",
            )
        },
        Rule {
            description: "More than twenty package updates are pending on the mail gateway."
                .to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "pmg_updates_pending",
                "Mail gateway updates pending",
                RuleKind::Threshold,
                "dumbmonit_pmg_node_updates_pending",
            )
        },
        // --- fin du bloc Proxmox Mail Gateway ---
    ]
}
