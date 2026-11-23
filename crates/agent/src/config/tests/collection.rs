//! Tests de la configuration (collection).

use super::*;

#[test]
fn collection_scope_follows_the_environment() {
    let yaml = "interfaces_ignore: ^veth\ncpu_per_core: true\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(
        file,
        env(&[
            ("DUMBMONIT_AGENT_INTERFACES_IGNORE", "^br-, docker0"),
            ("DUMBMONIT_AGENT_MOUNTS_IGNORE", "^/boot"),
            ("DUMBMONIT_AGENT_CPU_PER_CORE", "no"),
            ("DUMBMONIT_AGENT_DOCKER_MAX_CONTAINERS", "0"),
        ]),
    )
    .expect("configuration");
    assert_eq!(config.probe.interfaces_ignore.source(), ["^br-", "docker0"]);
    assert!(config.probe.keeps_interface("veth0"), "le fichier est remplacé, pas fusionné");
    assert!(!config.probe.keeps_interface("docker0"));
    assert_eq!(config.probe.mounts_ignore.source(), ["^/boot"]);
    assert!(!config.probe.cpu_per_core);
    assert_eq!(config.docker_max_containers, 0, "zéro : compter sans détailler");
}

#[test]
fn an_empty_list_disables_a_filter() {
    let yaml = "interfaces_ignore: []\nmounts_ignore: []\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert!(config.probe.interfaces_ignore.is_empty());
    assert!(config.probe.keeps_interface("veth0"));
    assert!(config.probe.keeps_mount("/var/lib/docker/volumes/x", "ext4"));
}

#[test]
fn an_invalid_pattern_is_refused_at_startup_with_its_key() {
    for (variable, key) in [
        ("DUMBMONIT_AGENT_INTERFACES_IGNORE", "interfaces_ignore"),
        ("DUMBMONIT_AGENT_INTERFACES_ONLY", "interfaces_only"),
        ("DUMBMONIT_AGENT_MOUNTS_IGNORE", "mounts_ignore"),
    ] {
        let error = Config::merge(file_with_url_and_token(), env(&[(variable, "^(oops")]))
            .expect_err("expression invalide")
            .to_string();
        assert!(error.contains(key), "{variable}: {error}");
    }

    let yaml = "mounts_ignore:\n  - '[unclosed'\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let error = Config::merge(file, env(&[])).expect_err("expression invalide").to_string();
    assert!(error.contains("mounts_ignore"), "{error}");
    assert!(error.contains("[unclosed"), "{error}");

    assert!(
        Config::merge(
            file_with_url_and_token(),
            env(&[("DUMBMONIT_AGENT_DOCKER_MAX_CONTAINERS", "beaucoup")])
        )
        .is_err()
    );
}

#[test]
fn docker_and_command_flags_follow_the_environment() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[
            ("DUMBMONIT_AGENT_DOCKER_UPDATE_CHECK", "false"),
            ("DUMBMONIT_AGENT_COMMANDS", "no"),
        ]),
    )
    .expect("configuration");
    assert!(!config.docker_update_check);
    assert!(!config.commands);
    assert!(
        Config::merge(
            file_with_url_and_token(),
            env(&[("DUMBMONIT_AGENT_COMMANDS", "peut-être")])
        )
        .is_err()
    );
}

#[test]
fn plakar_klosets_are_read_from_the_file_or_the_environment() {
    let yaml = "plakar_klosets:\n  - /srv/backups/main\n  - ' '\nplakar_bin: /opt/plakar\nplakar_home: /root\nplakar_interval_secs: 120\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert!(config.plakar.enabled, "la détection est active par défaut");
    assert_eq!(config.plakar.klosets, vec!["/srv/backups/main"]);
    assert_eq!(config.plakar.bin, "/opt/plakar");
    assert_eq!(config.plakar.home.as_deref(), Some("/root"));
    assert_eq!(config.plakar.interval, Duration::from_secs(120));

    let config = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_PLAKAR_KLOSETS", "/a, ptar:/b ,")]),
    )
    .expect("configuration");
    assert_eq!(config.plakar.klosets, vec!["/a", "ptar:/b"]);
}

#[test]
fn plakar_can_be_switched_off_from_the_file_or_the_environment() {
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str("plakar: false\n").expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert!(!config.plakar.enabled);

    let config =
        Config::merge(file_with_url_and_token(), env(&[("DUMBMONIT_AGENT_PLAKAR", "0")]))
            .expect("configuration");
    assert!(!config.plakar.enabled);
    assert!(
        Config::merge(file_with_url_and_token(), env(&[("DUMBMONIT_AGENT_PLAKAR", "maybe")]))
            .is_err()
    );
}

#[test]
fn libvirt_reads_the_system_uri_by_default_and_both_sources() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.libvirt, LibvirtConfig::default());
    assert_eq!(config.libvirt.uri, "qemu:///system");

    let yaml = "libvirt_bin: /usr/bin/virsh\nlibvirt_uri: qemu+ssh://kvm/system\n\
                libvirt_interval_secs: 30\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config =
        Config::merge(file, env(&[("DUMBMONIT_AGENT_LIBVIRT_URI", "qemu:///session")]))
            .expect("configuration");
    assert_eq!(config.libvirt.bin, "/usr/bin/virsh");
    assert_eq!(config.libvirt.uri, "qemu:///session", "l'environnement prime");
    assert_eq!(config.libvirt.interval, Duration::from_secs(30));

    let too_fast = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_LIBVIRT_INTERVAL_SECS", "5")]),
    );
    assert!(too_fast.is_err());
    let off =
        Config::merge(file_with_url_and_token(), env(&[("DUMBMONIT_AGENT_LIBVIRT", "0")]))
            .expect("configuration");
    assert!(!off.libvirt.enabled);
}

#[test]
fn too_frequent_a_plakar_reading_is_refused() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_PLAKAR_INTERVAL_SECS", "5")]),
    );
    assert!(config.is_err());
}

#[test]
fn hardware_collection_is_on_by_default_and_reads_the_file() {
    // Par défaut, tout est allumé : ces trois lectures se détectent
    // elles-mêmes et ne coûtent rien là où il n'y a rien à lire.
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert!(config.sensors);
    assert_eq!(config.smart, SmartConfig::default());
    assert_eq!(config.zfs, ZfsConfig::default());

    let yaml = "sensors: false\nsmart_bin: /usr/local/sbin/smartctl\n\
                smart_interval_secs: 900\nzfs_bin: /sbin/zpool\nzfs_interval_secs: 60\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert!(!config.sensors);
    assert_eq!(config.smart.bin, "/usr/local/sbin/smartctl");
    assert_eq!(config.smart.interval, Duration::from_secs(900));
    assert_eq!(config.zfs.bin, "/sbin/zpool");
    assert_eq!(config.zfs.interval, Duration::from_secs(60));
}

#[test]
fn hardware_collection_can_be_switched_off_from_the_environment() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[
            ("DUMBMONIT_AGENT_SENSORS", "false"),
            ("DUMBMONIT_AGENT_SMART", "false"),
            ("DUMBMONIT_AGENT_ZFS", "0"),
        ]),
    )
    .expect("configuration");
    assert!(!config.sensors);
    assert!(!config.smart.enabled);
    assert!(!config.zfs.enabled);
}

#[test]
fn too_frequent_a_hardware_reading_is_refused() {
    // Interroger `smartctl` toutes les secondes réveillerait des disques en
    // veille à longueur de journée : le plancher n'est pas négociable.
    assert!(
        Config::merge(
            file_with_url_and_token(),
            env(&[("DUMBMONIT_AGENT_SMART_INTERVAL_SECS", "5")])
        )
        .is_err()
    );
    assert!(
        Config::merge(
            file_with_url_and_token(),
            env(&[("DUMBMONIT_AGENT_ZFS_INTERVAL_SECS", "1")])
        )
        .is_err()
    );
}

#[test]
fn the_environment_wins_over_the_file() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[
            ("DUMBMONIT_AGENT_URL", "https://autre:9000"),
            ("DUMBMONIT_AGENT_INTERVAL_SECS", "60"),
        ]),
    )
    .expect("configuration");
    assert_eq!(config.server_url, "https://autre:9000");
    assert_eq!(config.interval, Duration::from_secs(60));
}

#[test]
fn a_missing_url_or_token_is_refused() {
    let no_url = FileConfig { token: Some("dmon_abc".into()), ..FileConfig::default() };
    assert!(Config::merge(no_url, env(&[])).is_err());

    let no_token =
        FileConfig { server_url: Some("https://s:8080".into()), ..FileConfig::default() };
    assert!(Config::merge(no_token, env(&[])).is_err());
}

#[test]
fn an_url_without_a_scheme_is_refused() {
    let file = FileConfig {
        server_url: Some("serveur:8080".into()),
        token: Some("dmon_abc".into()),
        ..FileConfig::default()
    };
    // Sans schéma, reqwest échouerait à chaque envoi : autant le dire au
    // démarrage plutôt qu'à la première tentative.
    assert!(Config::merge(file, env(&[])).is_err());
}
