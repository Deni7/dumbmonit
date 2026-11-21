//! Règles livrées de Tailscale, des équipements réseau d'éditeurs, des NAS,
//! du stockage objet, des hyperviseurs, de l'instance elle-même, de la note de
//! sécurité, des failles connues et de l'inventaire du réseau.

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // --- Tailscale (`collectors/tailscale`) ---
        // Seuls les appareils surveillés : un téléphone éteint n'est pas une panne.
        Rule {
            description: "A Tailscale device that must stay online (a tagged device by default) has been disconnected for more than 15 minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 15.0 * 60.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "tailscale_device_offline",
                "Tailscale device offline",
                RuleKind::Threshold,
                "dumbmonit_tailscale_device_offline_seconds and on (target, device) dumbmonit_tailscale_device_watched > 0",
            )
        },
        Rule {
            description: "A Tailscale device's node key expires in less than 14 days: it will leave the tailnet until someone signs it in again.".to_string(),
            operator: Operator::Lt,
            threshold: 14.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "tailscale_key_expiring",
                "Tailscale node key expiring",
                RuleKind::Threshold,
                "dumbmonit_tailscale_device_key_expiry_seconds",
            )
        },
        Rule {
            description: "A device joined the tailnet and awaits approval: approve it if it is expected, remove it otherwise.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "tailscale_device_unapproved",
                "Tailscale device awaiting approval",
                RuleKind::Threshold,
                "dumbmonit_tailscale_device_authorized",
            )
        },
        Rule {
            description: "A Tailscale device that must stay online runs a client with an update available.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(6 * 3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(168 * 3600)),
            ..base(
                "tailscale_update_available",
                "Tailscale client update",
                RuleKind::Threshold,
                "dumbmonit_tailscale_device_update_available and on (target, device) dumbmonit_tailscale_device_watched > 0",
            )
        },
        // --- FortiGate (`collectors/fortigate`) ---
        Rule {
            description: "No phase 2 of a FortiGate site-to-site IPsec tunnel is up: the site behind it is cut off.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_ipsec_tunnel_down",
                "FortiGate IPsec tunnel down",
                RuleKind::Threshold,
                "dumbmonit_fortigate_ipsec_tunnel_up",
            )
        },
        Rule {
            description: "An enabled FortiGate interface with an address has no link: cable, transceiver or the device in front.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_interface_down",
                "FortiGate interface without link",
                RuleKind::Threshold,
                "dumbmonit_fortigate_interface_down",
            )
        },
        Rule {
            description: "The members of a FortiGate HA cluster no longer have the same configuration checksum: a failover would run another configuration.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_ha_out_of_sync",
                "FortiGate HA out of sync",
                RuleKind::Threshold,
                "dumbmonit_fortigate_ha_in_sync",
            )
        },
        // Le nombre attendu de membres n'est publié nulle part : le maximum d'une journée en tient lieu. Un membre retiré pour de bon cesse d'alerter le lendemain.
        Rule {
            description: "A FortiGate HA cluster has fewer members than in the last 24 hours: there is no standby any more.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_ha_member_lost",
                "FortiGate HA member lost",
                RuleKind::Threshold,
                "max_over_time(dumbmonit_fortigate_ha_members[1d]) - dumbmonit_fortigate_ha_members",
            )
        },
        Rule {
            description: "FortiGate memory has stayed above 85 % for ten minutes: at 88 % (by default) it enters conserve mode and stops inspecting new sessions.".to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_memory_high",
                "FortiGate memory near conserve mode",
                RuleKind::Threshold,
                "dumbmonit_fortigate_memory_used_percent",
            )
        },
        Rule {
            description: "FortiGate CPU has stayed above 90 % for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "fortigate_cpu_high",
                "FortiGate CPU high",
                RuleKind::Threshold,
                "dumbmonit_fortigate_cpu_usage_percent",
            )
        },
        Rule {
            description: "A FortiGuard or FortiCare licence of the FortiGate expires in less than 30 days, or has expired.".to_string(),
            operator: Operator::Lt,
            threshold: 30.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "fortigate_license_expiring",
                "FortiGate licence expiring",
                RuleKind::Threshold,
                "dumbmonit_fortigate_license_expiry_seconds",
            )
        },
        Rule {
            description: "FortiGuard offers a newer patch release of the FortiOS branch the FortiGate runs.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(168 * 3600)),
            ..base(
                "fortigate_firmware_update",
                "FortiGate firmware update",
                RuleKind::Threshold,
                "dumbmonit_fortigate_firmware_update_available",
            )
        },
        // --- Sophos Firewall (`collectors/sophos`) ---
        Rule {
            description: "A Sophos Firewall interface that is switched on and bound to a zone has no link.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "sophos_interface_down",
                "Sophos interface without link",
                RuleKind::Threshold,
                "dumbmonit_sophos_interface_down",
            )
        },
        // --- TP-Link Omada (`collectors/omada`) ---
        Rule {
            description: "An Omada access point, switch or gateway has been disconnected from the controller for ten minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "omada_device_offline",
                "Omada device offline",
                RuleKind::Threshold,
                "dumbmonit_omada_device_offline",
            )
        },
        Rule {
            description: "An Omada device has had its CPU above 90 % for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "omada_cpu_high",
                "Omada device CPU high",
                RuleKind::Threshold,
                "dumbmonit_omada_device_cpu_usage_percent",
            )
        },
        Rule {
            description: "An Omada device has had its memory above 90 % for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "omada_memory_high",
                "Omada device memory high",
                RuleKind::Threshold,
                "dumbmonit_omada_device_memory_used_percent",
            )
        },
        Rule {
            description: "The Omada controller offers a firmware update for a device.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(168 * 3600)),
            ..base(
                "omada_firmware_update",
                "Omada firmware update",
                RuleKind::Threshold,
                "dumbmonit_omada_device_upgrade_available",
            )
        },
        Rule {
            description: "Aruba Central reports an access point, switch or gateway offline.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "arubacentral_device_offline",
                "Aruba Central device offline",
                RuleKind::Threshold,
                "dumbmonit_arubacentral_device_online",
            )
        },
        Rule {
            description: "Aruba Central has at least one active critical alert.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "arubacentral_critical_alert",
                "Aruba Central critical alert",
                RuleKind::Threshold,
                "dumbmonit_arubacentral_alerts_active_by_severity{severity=\"critical\"}",
            )
        },
        Rule {
            description: "Aruba Central rates at least one device in poor health for an hour.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(168 * 3600)),
            ..base(
                "arubacentral_devices_poor_health",
                "Aruba Central devices in poor health",
                RuleKind::Threshold,
                "dumbmonit_arubacentral_devices_poor_health",
            )
        },
        // --- Stormshield SNS, par SNMP (`profiles/stormshield.yaml`) ---
        Rule {
            description: "A node of a Stormshield HA cluster is declared dead: there is no standby any more.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "stormshield_ha_node_dead",
                "Stormshield HA node dead",
                RuleKind::Threshold,
                "dumbmonit_stormshield_ha_dead_nodes",
            )
        },
        Rule {
            description: "A link between the nodes of a Stormshield HA cluster is faulty.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "stormshield_ha_link_faulty",
                "Stormshield HA link faulty",
                RuleKind::Threshold,
                "dumbmonit_stormshield_ha_faulty_links",
            )
        },
        Rule {
            description: "A QNAP volume is more than 90 % full: shares, snapshots and apps stop being able to write.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "qnap_volume_full",
                "QNAP volume nearly full",
                RuleKind::Threshold,
                "dumbmonit_qnap_volume_used_percent",
            )
        },
        Rule {
            description: "The SMART health of a QNAP disk is no longer good (warning or abnormal): replace it before the RAID loses redundancy.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "qnap_disk_unhealthy",
                "QNAP disk health not good",
                RuleKind::Threshold,
                "dumbmonit_qnap_disk_ok",
            )
        },
        Rule {
            description: "A QNAP disk has stayed above 55 °C for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 55.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "qnap_disk_hot",
                "QNAP disk too hot",
                RuleKind::Threshold,
                "dumbmonit_qnap_disk_temperature_celsius",
            )
        },
        Rule {
            description: "The QNAP processor has stayed above 90 °C for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "qnap_cpu_hot",
                "QNAP processor too hot",
                RuleKind::Threshold,
                "dumbmonit_qnap_cpu_temperature_celsius",
            )
        },
        Rule {
            description: "QNAP offers a newer QTS or QuTS hero release.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(168 * 3600)),
            ..base(
                "qnap_firmware_update",
                "QNAP firmware update",
                RuleKind::Threshold,
                "dumbmonit_qnap_firmware_update_available",
            )
        },
        // --- Ceph (`collectors/ceph`) ---
        Rule {
            description: "The Ceph cluster reports HEALTH_ERR: data may be unavailable or at risk.".to_string(),
            operator: Operator::Gt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "ceph_health_error",
                "Ceph health error",
                RuleKind::Threshold,
                "dumbmonit_ceph_health_status",
            )
        },
        Rule {
            description: "The Ceph cluster has reported HEALTH_WARN for 30 minutes: see its health checks.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "ceph_health_warn",
                "Ceph health warning",
                RuleKind::Threshold,
                "dumbmonit_ceph_health_status",
            )
        },
        Rule {
            description: "One or more Ceph OSDs are down: the cluster rebuilds their data elsewhere and loses redundancy meanwhile.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "ceph_osd_down",
                "Ceph OSD down",
                RuleKind::Threshold,
                "dumbmonit_ceph_osds_down",
            )
        },
        Rule {
            description: "Some Ceph placement groups have not been active+clean for 30 minutes: data is degraded, misplaced or inactive.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "ceph_pg_unclean",
                "Ceph placement groups not clean",
                RuleKind::Threshold,
                "dumbmonit_ceph_pgs_unclean",
            )
        },
        Rule {
            description: "Ceph reports unfound objects: no copy of some data can be found.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "ceph_objects_unfound",
                "Ceph objects unfound",
                RuleKind::Threshold,
                "dumbmonit_ceph_objects_unfound",
            )
        },
        Rule {
            description: "The Ceph cluster is more than 80 % full: it turns read-only at 95 %, and rebuilding after a failure needs free space.".to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "ceph_capacity_high",
                "Ceph capacity high",
                RuleKind::Threshold,
                "dumbmonit_ceph_capacity_used_percent",
            )
        },
        Rule {
            description: "A Ceph OSD is more than 85 % full (the default nearfull ratio): writes to it stop at the full ratio.".to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "ceph_osd_nearly_full",
                "Ceph OSD nearly full",
                RuleKind::Threshold,
                "dumbmonit_ceph_osd_used_percent",
            )
        },
        Rule {
            description: "A Ceph pool is more than 90 % full.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "ceph_pool_nearly_full",
                "Ceph pool nearly full",
                RuleKind::Threshold,
                "dumbmonit_ceph_pool_used_percent",
            )
        },
        Rule {
            description: "A Ceph monitor is out of the quorum: one more loss and the cluster stops.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "ceph_mon_out_of_quorum",
                "Ceph monitor out of quorum",
                RuleKind::Threshold,
                "dumbmonit_ceph_mons_out_of_quorum",
            )
        },
        // --- MinIO (`collectors/minio`) ---
        Rule {
            description: "A MinIO node is offline: the cluster runs with less parity.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "minio_node_offline",
                "MinIO node offline",
                RuleKind::Threshold,
                "dumbmonit_minio_nodes_offline",
            )
        },
        Rule {
            description: "A MinIO drive is offline: objects are rebuilt from parity until it is back.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "minio_drive_offline",
                "MinIO drive offline",
                RuleKind::Threshold,
                "dumbmonit_minio_drives_offline",
            )
        },
        Rule {
            description: "The usable capacity of the MinIO cluster is more than 85 % used.".to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "minio_capacity_high",
                "MinIO capacity high",
                RuleKind::Threshold,
                "dumbmonit_minio_capacity_usable_used_percent",
            )
        },
        Rule {
            description: "MinIO could not heal some objects in its current self-healing run.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "minio_heal_failures",
                "MinIO heal failures",
                RuleKind::Threshold,
                "dumbmonit_minio_heal_objects_failed",
            )
        },
        Rule {
            description: "Objects of a MinIO bucket failed to replicate to the remote site.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "minio_replication_failed",
                "MinIO replication failures",
                RuleKind::Threshold,
                "dumbmonit_minio_replication_failed_objects",
            )
        },
        Rule {
            description: "More than 5 % of the S3 requests MinIO served in the last 10 minutes failed with a server error (5xx).".to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "minio_s3_server_errors",
                "MinIO S3 server errors",
                RuleKind::Threshold,
                "100 * rate(dumbmonit_minio_s3_requests_5xx_total[10m]) / rate(dumbmonit_minio_s3_requests_total[10m])",
            )
        },
        // --- KVM par l'agent, à travers libvirt (`agent/src/collect/libvirt.rs`) ---
        Rule {
            description: "libvirt reports a KVM domain as crashed: the hypervisor had to stop it.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "libvirt_domain_crashed",
                "KVM domain crashed",
                RuleKind::Threshold,
                "dumbmonit_agent_libvirt_domain_crashed",
            )
        },
        Rule {
            description: "A libvirt storage pool's allocation reaches 90% of its capacity.".to_string(),
            operator: Operator::Ge,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "libvirt_pool_almost_full",
                "libvirt pool almost full",
                RuleKind::Threshold,
                "dumbmonit_agent_libvirt_pool_used_percent",
            )
        },
        // --- Synology DSM par l'agent du paquet `.spk` (`agent/src/collect/dsm.rs`) ---
        // L'onduleur branché au NAS est rapporté sous les noms du collecteur NUT :
        // ses règles (`nut_on_battery`, `nut_battery_low`…) s'appliquent telles
        // quelles. Le SMART, quand `smartctl` est lisible, passe par
        // `disk_smart_failed`.
        //
        // `agent_dsm_raid_status` : 0 normal, 1 resynchronisation, 2 dégradée
        // (reconstruction comprise), 3 inactive.
        Rule {
            description: "A RAID array of the NAS lost a disk or is rebuilding: its redundancy \
                          is gone, read from /proc/mdstat by the agent package."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "dsm_raid_degraded",
                "NAS RAID degraded",
                RuleKind::Threshold,
                "dumbmonit_agent_dsm_raid_status",
            )
        },
        // DSM remonte en lecture seule un volume qu'il juge planté.
        Rule {
            description: "A volume of the NAS is mounted read-only: DSM does this to a crashed \
                          volume to protect its data."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "dsm_volume_read_only",
                "NAS volume read-only",
                RuleKind::Threshold,
                "dumbmonit_agent_dsm_volume_read_only",
            )
        },
        Rule {
            description: "A disk of the NAS is above 55 °C, as read by the agent package."
                .to_string(),
            operator: Operator::Gt,
            threshold: 55.0,
            clear_threshold: Some(52.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "dsm_disk_hot",
                "NAS disk temperature high",
                RuleKind::Threshold,
                "dumbmonit_agent_dsm_disk_temperature_celsius",
            )
        },
        Rule {
            description: "New unreadable (UNC) sectors appeared on a disk of the NAS in the last \
                          24 hours."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "dsm_disk_bad_sectors_growing",
                "NAS disk bad sectors growing",
                RuleKind::Threshold,
                "delta(dumbmonit_agent_dsm_disk_unc_count[24h])",
            )
        },
        Rule {
            description: "An SSD of the NAS has less than 10% of its rated life left.".to_string(),
            operator: Operator::Lt,
            threshold: 10.0,
            clear_threshold: Some(12.0),
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "dsm_ssd_wearout",
                "NAS SSD wearing out",
                RuleKind::Threshold,
                "dumbmonit_agent_dsm_disk_remaining_life_percent",
            )
        },
        // Seulement un paquet qui doit tourner (témoin `enabled`) et que
        // `synopkg` dit arrêté : un paquet arrêté à la main ne déclenche rien.
        Rule {
            description: "A package of the NAS that should be running is stopped.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "dsm_package_down",
                "NAS package stopped",
                RuleKind::Threshold,
                "dumbmonit_agent_dsm_package_down",
            )
        },
        // --- XCP-ng / XenServer (`collectors/xcpng`) ---
        Rule {
            description: "A host of the XCP-ng pool no longer answers the coordinator.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "xcpng_host_offline",
                "XCP-ng host offline",
                RuleKind::Threshold,
                "dumbmonit_xcpng_host_live",
            )
        },
        Rule {
            description: "An XCP-ng storage repository is more than 90% full.".to_string(),
            operator: Operator::Ge,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "xcpng_sr_almost_full",
                "XCP-ng SR almost full",
                RuleKind::Threshold,
                "dumbmonit_xcpng_sr_used_percent",
            )
        },
        // Une VM que la haute disponibilité du pool doit garder en marche
        // (`ha_restart_priority` non vide, HA active) est arrêtée : HA ne l'a
        // pas relancée, ou n'a pas pu (ressources insuffisantes).
        Rule {
            description: "A VM protected by the pool's high availability is halted.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "xcpng_vm_ha_protected_down",
                "XCP-ng HA-protected VM down",
                RuleKind::Threshold,
                "dumbmonit_xcpng_vm_ha_protected_down",
            )
        },
        Rule {
            description: "The pool is overcommitted: it no longer has a plan to survive the host failures it is set to tolerate.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            escalate_after: Some(Duration::from_secs(3600)),
            ..base(
                "xcpng_pool_ha_overcommitted",
                "XCP-ng pool HA overcommitted",
                RuleKind::Threshold,
                "dumbmonit_xcpng_pool_ha_overcommitted",
            )
        },
        // --- Nutanix, par Prism Central (`collectors/nutanix`) ---
        Rule {
            description: "Prism reports a Nutanix host as degraded.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "nutanix_host_degraded",
                "Nutanix host degraded",
                RuleKind::Threshold,
                "dumbmonit_nutanix_host_degraded",
            )
        },
        Rule {
            description: "A Nutanix storage container is more than 90% full.".to_string(),
            operator: Operator::Ge,
            threshold: 90.0,
            clear_threshold: Some(88.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "nutanix_storage_container_almost_full",
                "Nutanix storage container almost full",
                RuleKind::Threshold,
                "dumbmonit_nutanix_storage_container_used_percent",
            )
        },
        Rule {
            description: "Prism Central reports one or more unresolved critical alerts.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "nutanix_alerts_critical",
                "Nutanix critical alerts",
                RuleKind::Threshold,
                "dumbmonit_nutanix_alerts_critical",
            )
        },
        // --- Hyper-V, par l'agent Windows (`agent/src/collect/perf_counters.rs`) ---
        Rule {
            description: "Hyper-V reports a virtual machine in critical health: a resource it needs, most often disk space, is exhausted.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "hyperv_vm_critical",
                "Hyper-V VM health critical",
                RuleKind::Threshold,
                "dumbmonit_hyperv_vms_health_critical",
            )
        },
        // Le compteur de l'hyperviseur, pas `\Processor(_Total)` : celui-là ne voit que la partition racine.
        Rule {
            description: "The Hyper-V host's logical processors have been more than 90 % busy for fifteen minutes, guests included.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "hyperv_host_cpu_high",
                "Hyper-V host CPU high",
                RuleKind::Threshold,
                "dumbmonit_hyperv_host_cpu_percent",
            )
        },
        Rule {
            description: "A Hyper-V virtual disk reported I/O errors in the last hour.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "hyperv_vhd_errors",
                "Hyper-V virtual disk errors",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_hyperv_vhd_errors_total[1h])",
            )
        },
        // --- fin du bloc Synology DSM ---
        // La sauvegarde locale de DumbMonit lui-même.
        //
        // Elle est la seule chose qui rende une mise à jour rattrapable, et
        // c'est aussi la seule que personne ne va vérifier : on ne regarde un
        // répertoire de sauvegardes que le jour où l'on en a besoin. La série
        // n'existe qu'une fois la première sauvegarde écrite — instance dont la
        // planification est coupée, pas d'alerte, pas de faux reproche — et
        // porte l'heure de la dernière **réussite**, jamais celle d'une
        // tentative ratée. Deux jours au seuil : le rythme livré est quotidien,
        // une nuit manquée peut être un redémarrage.
        Rule {
            description: "The scheduled local backup of the DumbMonit database has not run for \
                          more than two days."
                .to_string(),
            operator: Operator::Gt,
            threshold: 2.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "instance_backup_missing",
                "DumbMonit backup did not run",
                RuleKind::Threshold,
                "time() - dumbmonit_instance_backup_last_success_seconds",
            )
        },
        // Le reste de l'auto-surveillance (`selfmon.rs`), publié chaque minute.
        // Ce qu'aucune règle ne peut voir — VictoriaMetrics lui-même arrêté, qui
        // porte leur évaluation — c'est le battement de cœur sortant qui le dit.
        //
        // Volume `/data` presque plein : la base, les sauvegardes locales et les
        // séries y vivent ensemble, et SQLite comme VictoriaMetrics refusent
        // d'écrire quand il n'y a plus de place.
        Rule {
            description: "The volume that holds DumbMonit's data (/data) has less than 10% free \
                          space: free some, or grow the volume, before writes start to fail."
                .to_string(),
            operator: Operator::Lt,
            threshold: 10.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "instance_data_volume_low",
                "DumbMonit data volume almost full",
                RuleKind::Threshold,
                "100 * dumbmonit_instance_data_free_bytes / dumbmonit_instance_data_size_bytes",
            )
        },
        // Le superviseur relance VictoriaMetrics après chaque arrêt, avec un délai
        // croissant : trois relances en une demi-heure, c'est une boucle (disque
        // plein, mémoire trop basse, fichier corrompu), pas un accident.
        Rule {
            description: "The embedded VictoriaMetrics was restarted three times or more in the \
                          last 30 minutes: read the server log for the reason."
                .to_string(),
            operator: Operator::Ge,
            threshold: 3.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "instance_vm_restarting",
                "DumbMonit metrics database restarting in a loop",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_instance_vm_restarts_total[30m])",
            )
        },
        // La boucle du planificateur tourne chaque seconde : deux minutes sans
        // tour, plus rien n'est interrogé.
        Rule {
            description: "DumbMonit's scheduler has not completed a round for more than two \
                          minutes: devices are no longer being polled."
                .to_string(),
            operator: Operator::Gt,
            threshold: 120.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "instance_scheduler_stalled",
                "DumbMonit scheduler stalled",
                RuleKind::Threshold,
                "dumbmonit_instance_scheduler_lag_seconds",
            )
        },
        // Des cibles restées dues après chaque tour : plus de sondes que de
        // jetons d'exécution. Un quart d'heure, ce n'est plus un pic.
        Rule {
            description: "Devices stay overdue after every scheduling round for 15 minutes: \
                          polls are late. Raise DUMBMONIT_MAX_CONCURRENT_PROBES or lengthen \
                          some intervals."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "instance_scheduler_behind",
                "DumbMonit polls running late",
                RuleKind::Threshold,
                "min_over_time(dumbmonit_instance_scheduler_backlog[15m])",
            )
        },
        // Lots refusés par VictoriaMetrics, ou mesures perdues faute de place
        // dans le tampon : des trous dans les graphes et des règles aveugles.
        Rule {
            description: "DumbMonit could not write some measurements to its metrics database \
                          in the last 15 minutes: graphs will have gaps."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "instance_write_errors",
                "DumbMonit metric writes failing",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_instance_sample_writes_failed_total[15m]) \
                 + increase_prometheus(dumbmonit_instance_samples_dropped_total[15m])",
            )
        },
        // Base de configuration qui double en une journée, au-delà de 100 Mo :
        // un journal qui ne se purge plus, pas l'ajout de quelques équipements.
        Rule {
            description: "DumbMonit's SQLite database more than doubled in a day and is over \
                          100 MB: something keeps writing to it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 2.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "×".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "instance_database_growth",
                "DumbMonit database growing fast",
                RuleKind::Threshold,
                "dumbmonit_instance_database_bytes / dumbmonit_instance_database_bytes offset 1d \
                 and dumbmonit_instance_database_bytes > 100e6",
            )
        },

        // --- Note de sécurité (`security/`) ---
        //
        // La note n'est recalculée que toutes les quinze minutes et bouge
        // rarement : une baisse d'au moins dix points sur la journée, c'est un
        // nouveau défaut (mise à jour de sécurité publiée, sauvegarde en échec,
        // certificat proche de l'échéance). Le maximum glissant sert de
        // référence : une note restée basse cesse d'alerter le lendemain — la
        // règle « note basse » prend alors le relais.
        Rule {
            description: "A device's security score dropped by 10 points or more within a day: \
                          open its Security card to see the newly failing check."
                .to_string(),
            operator: Operator::Ge,
            threshold: 10.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: " pts".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "security_score_drop",
                "Security score dropped",
                RuleKind::Threshold,
                "max_over_time(dumbmonit_security_score[1d]) - dumbmonit_security_score",
            )
        },
        // Seuil réglable comme toute règle livrée : 40 est la limite de la
        // lettre F. Rappel hebdomadaire : c'est un chantier, pas une panne.
        Rule {
            description: "A device's security score is below 40 (grade F): several vendor best \
                          practices are not followed."
                .to_string(),
            operator: Operator::Lt,
            threshold: 40.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "security_score_low",
                "Security score low",
                RuleKind::Threshold,
                "dumbmonit_security_score",
            )
        },

        // --- Failles connues (`vulns/`) ---
        //
        // Une série par CVE et par composant vulnérable (étiquettes `cve` et
        // `component`), republiée tous les quarts d'heure ; une CVE ignorée
        // dans l'interface n'est plus publiée. Seuls deux cas alertent : la
        // faille exploitée (catalogue KEV de la CISA) et la faille critique
        // (CVSS ≥ 9). Le reste s'affiche sans notifier ; « à vérifier » ne
        // notifie jamais. Rappel hebdomadaire : c'est un chantier, pas une
        // panne.
        Rule {
            description: "A component of this device is vulnerable to a CVE that is actively \
                          exploited (CISA KEV catalog): update it, or accept the risk in \
                          Security → CVE & vulnerabilities."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "cve_exploited_vulnerable",
                "Exploited vulnerability not patched",
                RuleKind::Threshold,
                "dumbmonit_cve_exploited_vulnerable",
            )
        },
        Rule {
            description: "A component of this device is vulnerable to a critical CVE (CVSS 9 or \
                          more) that is not patched yet."
                .to_string(),
            operator: Operator::Ge,
            threshold: 9.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: " CVSS".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "cve_critical_vulnerable",
                "Critical vulnerability not patched",
                RuleKind::Threshold,
                "dumbmonit_cve_vulnerable",
            )
        },
        // Inventaire du réseau (`netinventory/`). Désactivées par défaut : sur un
        // réseau qui bouge (invités, DHCP), elles ne servent qu'à qui veut les
        // lire, et sévérité « info » pour ne jamais réveiller personne. Une
        // mesure par élément inconnu : une alerte par nouvel appareil ou port,
        // qui dure jusqu'à ce qu'il soit marqué connu. Pas de rappel.
        Rule {
            description: "A scheduled network scan found a device that is not in the \
                          inventory. It stays reported until it is marked as known."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            // La mesure est réécrite à chaque cycle : 60 s écartent un balayage
            // interrompu sans retarder l'annonce.
            for_duration: Duration::from_secs(60),
            severity: Severity::Info,
            repeat_interval: None,
            enabled: false,
            ..base(
                RULE_NETSCAN_NEW_DEVICE,
                "New device on the network",
                RuleKind::Threshold,
                "dumbmonit_netscan_unknown_device",
            )
        },
        Rule {
            description: "A scheduled network scan found a new open port on a known device. It \
                          stays reported until it is marked as known."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            // La mesure est réécrite à chaque cycle : 60 s écartent un balayage
            // interrompu sans retarder l'annonce.
            for_duration: Duration::from_secs(60),
            severity: Severity::Info,
            repeat_interval: None,
            enabled: false,
            ..base(
                RULE_NETSCAN_NEW_PORT,
                "New open port on the network",
                RuleKind::Threshold,
                "dumbmonit_netscan_unknown_port",
            )
        },
        Rule {
            description: "A device appeared on the address of a known device, under another MAC \
                          address: spoofing, or replaced hardware. It stays reported until it \
                          is marked as known or ignored."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Warning,
            repeat_interval: None,
            enabled: false,
            ..base(
                RULE_NETSCAN_MAC_CHANGE,
                "MAC address changed for an IP",
                RuleKind::Threshold,
                "dumbmonit_netscan_mac_change",
            )
        },
    ]
}
