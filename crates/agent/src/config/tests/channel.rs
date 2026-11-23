//! Tests de la configuration (channel).

use super::*;

#[test]
fn the_server_is_reached_over_https_or_the_loopback_only() {
    assert!(is_loopback_http("http://127.0.0.1:8080"));
    assert!(is_loopback_http("http://localhost:8080/x"));
    assert!(is_loopback_http("http://[::1]:8080"));
    assert!(!is_loopback_http("http://192.168.1.5:8080"));
    assert!(!is_loopback_http("http://127.0.0.1.evil.example"));
    assert!(!is_loopback_http("https://127.0.0.1"));
    let plain = FileConfig {
        server_url: Some("http://serveur:8080".into()),
        token: Some("dmon_x".into()),
        ..FileConfig::default()
    };
    // Lu (un agent déjà enrôlé doit pouvoir redémarrer), mais marqué : sans
    // canal chiffré rangé, l'agent refusera de démarrer (voir `run.rs`).
    assert!(Config::merge(plain, env(&[])).unwrap().plain_server_url);
    let local = FileConfig {
        server_url: Some("http://127.0.0.1:8080".into()),
        token: Some("dmon_x".into()),
        ..FileConfig::default()
    };
    assert!(!Config::merge(local, env(&[])).unwrap().plain_server_url);
    let ftp = FileConfig {
        server_url: Some("ftp://serveur".into()),
        token: Some("dmon_x".into()),
        ..FileConfig::default()
    };
    assert!(Config::merge(ftp, env(&[])).is_err());
    let pinned = FileConfig {
        server_url: Some("https://serveur:8080".into()),
        token: Some("dmon_x".into()),
        server_pin: Some(format!("sha256//{}=", "A".repeat(43))),
        ..FileConfig::default()
    };
    assert!(Config::merge(pinned, env(&[])).unwrap().server_pin.is_some());
    let bad = FileConfig {
        server_url: Some("https://serveur:8080".into()),
        token: Some("dmon_x".into()),
        server_pin: Some("sha256//AAAA".into()),
        ..FileConfig::default()
    };
    assert!(Config::merge(bad, env(&[])).is_err());
}

#[test]
fn the_old_variable_names_are_still_read_and_reported() {
    let config = Config::merge(
        FileConfig::default(),
        env(&[("EZYMONIT_AGENT_URL", "https://old:8080"), ("DUMBMONIT_AGENT_TOKEN", "dmon_x")]),
    )
    .expect("configuration");
    assert_eq!(config.server_url, "https://old:8080");
    assert_eq!(
        config.deprecated_env,
        vec![("EZYMONIT_AGENT_URL".to_string(), "DUMBMONIT_AGENT_URL".to_string())]
    );
}

#[test]
fn the_encrypted_channel_comes_from_the_install_command() {
    let pin = "AB:".repeat(31) + "AB";
    let file = FileConfig {
        tls_url: Some("https://serveur:8443/".into()),
        ca_pin: Some(pin),
        ..file_with_url_and_token()
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.tls_url.as_deref(), Some("https://serveur:8443"));
    assert_eq!(config.ca_pin.as_deref(), Some("ab".repeat(32).as_str()));
    assert!(!format!("{config:?}").contains("dmon_abc"), "le jeton reste caché");

    let plain =
        FileConfig { tls_url: Some("http://serveur:8443".into()), ..file_with_url_and_token() };
    assert!(Config::merge(plain, env(&[])).is_err(), "jamais de canal des agents en clair");
    let short = FileConfig { ca_pin: Some("abcd".into()), ..file_with_url_and_token() };
    assert!(Config::merge(short, env(&[])).is_err(), "une empreinte tronquée est refusée");
}

#[test]
fn the_disk_buffer_bound_comes_from_the_file_or_the_environment() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.buffer_max_mb, DEFAULT_BUFFER_MAX_MB);

    let file = FileConfig { buffer_max_mb: Some(64), ..file_with_url_and_token() };
    assert_eq!(Config::merge(file, env(&[])).expect("configuration").buffer_max_mb, 64);

    let file = FileConfig { buffer_max_mb: Some(64), ..file_with_url_and_token() };
    let config =
        Config::merge(file, env(&[("DUMBMONIT_AGENT_BUFFER_MAX_MB", "0")])).expect("configuration");
    assert_eq!(config.buffer_max_mb, 0, "zéro coupe le tampon disque");

    let bad = env(&[("DUMBMONIT_AGENT_BUFFER_MAX_MB", "beaucoup")]);
    assert!(Config::merge(file_with_url_and_token(), bad).is_err());
}

#[test]
fn server_urls_lists_the_backup_servers_in_order() {
    // Sans `server_url`, la première adresse de la liste est la primaire.
    let yaml = "token: dmon_x
server_urls:
  - https://a:8080/
  - https://b:8080
  - url: https://c:8080
    tls_url: https://agents-c:9443/
";
    let file: FileConfig = dumbmonit_collectors::yaml::from_str(yaml).expect("yaml");
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.server_url, "https://a:8080");
    assert_eq!(config.tls_url, None);
    assert_eq!(
        config.fallback_servers,
        vec![
            ServerEndpoint { url: "https://b:8080".into(), tls_url: None },
            ServerEndpoint {
                url: "https://c:8080".into(),
                tls_url: Some("https://agents-c:9443".into()),
            },
        ]
    );

    // Avec `server_url`, la liste ne fait qu'ajouter des secours ; la
    // primaire répétée et les doublons sont écartés.
    let file = FileConfig {
        server_urls: Some(ServerListFile::One(
            "https://serveur:8080, https://b:8080,https://b:8080".into(),
        )),
        ..file_with_url_and_token()
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.server_url, "https://serveur:8080");
    assert_eq!(
        config.fallback_servers,
        vec![ServerEndpoint { url: "https://b:8080".into(), tls_url: None }]
    );

    // L'environnement remplace la liste du fichier.
    let file = FileConfig {
        server_urls: Some(ServerListFile::One("https://b:8080".into())),
        ..file_with_url_and_token()
    };
    let config = Config::merge(file, env(&[("DUMBMONIT_AGENT_URLS", "https://d:8080")]))
        .expect("configuration");
    assert_eq!(config.fallback_servers[0].url, "https://d:8080");
    assert_eq!(config.fallback_servers.len(), 1);

    // Rien ne change sans la clé.
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert!(config.fallback_servers.is_empty());
}

#[test]
fn a_backup_server_is_never_reached_in_clear() {
    let file = FileConfig {
        server_urls: Some(ServerListFile::One("http://b:8080".into())),
        ..file_with_url_and_token()
    };
    assert!(Config::merge(file, env(&[])).is_err());
    let file = FileConfig {
        server_urls: Some(ServerListFile::Many(vec![ServerEntryFile::Full {
            url: "https://b:8080".into(),
            tls_url: Some("http://b:8443".into()),
        }])),
        ..file_with_url_and_token()
    };
    assert!(Config::merge(file, env(&[])).is_err());
    // Seule une liste suffit à donner l'adresse du serveur.
    let only_list = FileConfig {
        token: Some("dmon_x".into()),
        server_urls: Some(ServerListFile::One("https://a:8080,https://b:8080".into())),
        ..FileConfig::default()
    };
    assert_eq!(Config::merge(only_list, env(&[])).unwrap().fallback_servers.len(), 1);
    let nothing = FileConfig { token: Some("dmon_x".into()), ..FileConfig::default() };
    assert!(Config::merge(nothing, env(&[])).is_err());
}

#[test]
fn relay_mode_and_site_come_from_the_file_or_the_environment() {
    let file = FileConfig {
        relay: Some(true),
        site: Some("  Agence de Lyon ".into()),
        ..file_with_url_and_token()
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert!(config.relay);
    assert_eq!(config.site.as_deref(), Some("Agence de Lyon"));

    let config = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_RELAY", "true"), ("DUMBMONIT_AGENT_SITE", "Datacenter")]),
    )
    .expect("configuration");
    assert!(config.relay);
    assert_eq!(config.site.as_deref(), Some("Datacenter"));

    // Un site vide n'est pas un site.
    let config = Config::merge(file_with_url_and_token(), env(&[("DUMBMONIT_AGENT_SITE", "  ")]))
        .expect("configuration");
    assert_eq!(config.site, None);
}

#[test]
fn the_relay_gateway_is_read_from_the_file_or_the_environment() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.gateway, crate::gateway::GatewayConfig::default());

    let file = FileConfig {
        gateway_listen: Some(" 0.0.0.0:9443 ".into()),
        gateway_max_connections: Some(10),
        ..file_with_url_and_token()
    };
    let config = Config::merge(file, env(&[("DUMBMONIT_AGENT_GATEWAY_RATE_KIB", "512")]))
        .expect("configuration");
    assert_eq!(config.gateway.listen, Some("0.0.0.0:9443".parse().unwrap()));
    assert_eq!(config.gateway.max_connections, 10);
    assert_eq!(config.gateway.rate_kib, 512);

    let bad = FileConfig { gateway_listen: Some("relais".into()), ..file_with_url_and_token() };
    assert!(Config::merge(bad, env(&[])).is_err(), "une adresse sans port est refusée");
    let zero = env(&[("DUMBMONIT_AGENT_GATEWAY_MAX_CONNECTIONS", "0")]);
    assert!(Config::merge(file_with_url_and_token(), zero).is_err());
    // Au-delà, la connexion multiplexée vers le serveur pourrait déborder.
    let most = env(&[("DUMBMONIT_AGENT_GATEWAY_MAX_CONNECTIONS", "2048")]);
    let config = Config::merge(file_with_url_and_token(), most).expect("configuration");
    assert_eq!(config.gateway.max_connections, 2048);
    let beyond = env(&[("DUMBMONIT_AGENT_GATEWAY_MAX_CONNECTIONS", "2049")]);
    assert!(Config::merge(file_with_url_and_token(), beyond).is_err());
}

#[test]
fn a_minimal_file_is_enough() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    // La barre oblique finale est retirée : sans cela l'URL construite
    // contiendrait un double séparateur et le serveur répondrait 404.
    assert_eq!(config.server_url, "https://serveur:8080");
    assert_eq!(config.interval, Duration::from_secs(DEFAULT_INTERVAL_SECS));
    assert!(config.docker, "la découverte Docker est active par défaut");
    assert!(config.docker_update_check);
    assert!(config.commands, "les actions du serveur sont acceptées par défaut");
    assert!(!config.relay, "un agent ne relaie rien tant qu'on ne le lui demande pas");
    assert_eq!(config.site, None);
    assert_eq!(config.plakar, PlakarConfig::default());
    assert_eq!(config.probe, ProbeConfig::default());
    assert_eq!(config.docker_max_containers, DEFAULT_MAX_CONTAINERS);
    assert!(!config.probe.cpu_per_core, "le détail par cœur est désactivé par défaut");
    assert!(config.probe.interfaces_only.is_empty());
    assert!(!config.probe.keeps_interface("veth0abc"));
    assert!(!config.probe.keeps_mount("/var/lib/docker/volumes/x", "ext4"));
}

#[test]
fn collection_scope_is_read_from_the_file() {
    let yaml = r#"
interfaces_ignore:
  - ^veth
  - wlan0
interfaces_only: "eth0, ^en"
mounts_ignore: ^/mnt/scratch
cpu_per_core: true
docker_max_containers: 20
"#;
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.probe.interfaces_ignore.source(), ["^veth", "wlan0"]);
    assert_eq!(config.probe.interfaces_only.source(), ["eth0", "^en"]);
    assert_eq!(config.probe.mounts_ignore.source(), ["^/mnt/scratch"]);
    assert!(config.probe.cpu_per_core);
    assert_eq!(config.docker_max_containers, 20);
    // La liste explicite remplace la valeur par défaut : un montage Docker
    // n'est plus ignoré par son chemin (mais `overlay` l'est par son type).
    assert!(config.probe.keeps_mount("/var/lib/docker/volumes/x", "ext4"));
    assert!(!config.probe.keeps_mount("/mnt/scratch", "ext4"));
    // L'allow-list prime : `eth0` passe, `wlan0` non, `enp3s0` oui.
    assert!(config.probe.keeps_interface("eth0"));
    assert!(config.probe.keeps_interface("enp3s0"));
    assert!(!config.probe.keeps_interface("wlan0"));
}
