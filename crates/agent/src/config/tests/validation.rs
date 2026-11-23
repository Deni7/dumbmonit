//! Tests de la configuration (validation).

use super::*;

#[test]
fn too_short_an_interval_is_refused() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_INTERVAL_SECS", "1")]),
    );
    assert!(config.is_err());
}

#[test]
fn services_and_tags_are_read_from_the_environment() {
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[
            ("DUMBMONIT_AGENT_SERVICES", "sshd, docker ,"),
            ("DUMBMONIT_AGENT_TAGS", "role=nas, salle = cave"),
        ]),
    )
    .expect("configuration");
    assert_eq!(config.services, vec!["sshd", "docker"]);
    assert_eq!(config.tags.get("role").map(String::as_str), Some("nas"));
    assert_eq!(config.tags.get("salle").map(String::as_str), Some("cave"));
}

#[test]
fn a_malformed_tag_is_refused() {
    assert!(parse_tags("role").is_err());
    assert!(parse_tags("=cave").is_err());
}

#[test]
fn the_yaml_file_is_parsed_as_expected() {
    let yaml = r#"
server_url: https://serveur:8080
token: dmon_secret
interval_secs: 15
services:
  - sshd
  - nginx
tags:
  role: nas
docker: false
inconnu: 42
system_health:
  check_updates: false
  updates_interval_secs: 900
"#;
    let file: FileConfig = dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide");
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.interval, Duration::from_secs(15));
    assert_eq!(config.services, vec!["sshd", "nginx"]);
    assert!(!config.docker);
    assert!(config.system_health.enabled, "la santé du système reste active par défaut");
    assert!(!config.system_health.check_updates);
    assert!(config.system_health.check_reboot);
    assert_eq!(config.system_health.updates_interval, Duration::from_secs(900));
}

#[test]
fn system_health_defaults_and_environment_overrides() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.system_health, SystemHealthConfig::default());
    assert_eq!(
        config.system_health.updates_interval,
        Duration::from_secs(DEFAULT_UPDATES_INTERVAL_SECS)
    );

    let config = Config::merge(
        file_with_url_and_token(),
        env(&[
            ("DUMBMONIT_AGENT_SYSTEM_HEALTH", "false"),
            ("DUMBMONIT_AGENT_CHECK_FAILED_UNITS", "non"),
            ("DUMBMONIT_AGENT_UPDATES_INTERVAL_SECS", "7200"),
        ]),
    )
    .expect("configuration");
    assert!(!config.system_health.enabled);
    assert!(!config.system_health.check_failed_units);
    assert_eq!(config.system_health.updates_interval, Duration::from_secs(7200));
}

#[test]
fn databases_are_detected_by_default_without_any_backup_path() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.databases, DatabasesConfig::default());
    assert!(config.databases.enabled && config.databases.backups.is_empty());
    let off = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_DATABASES", "false")]),
    )
    .expect("configuration");
    assert!(!off.databases.enabled);
    let listed = Config::merge(with_yaml("modules: [docker]\n"), env(&[])).expect("config");
    assert!(!listed.databases.enabled, "absent de la liste des modules");
}

#[test]
fn database_backup_paths_are_read_per_engine() {
    let yaml = "databases:\n  backups:\n    sqlserver:\n      paths: ['D:\\Backups\\SQL\\*.bak']\n      max_age_hours: 26\n    mariadb:\n      paths: [/var/backups/mysql/*.sql.gz]\n";
    let config = Config::merge(with_yaml(yaml), env(&[])).expect("configuration");
    let rules = &config.databases.backups;
    assert_eq!(rules.len(), 2);
    let sql = rules.iter().find(|r| r.engine == Engine::Sqlserver).expect("sqlserver");
    assert_eq!(sql.paths, vec!["D:\\Backups\\SQL\\*.bak".to_string()]);
    assert_eq!(sql.max_age_hours, 26);
    let mysql = rules.iter().find(|r| r.engine == Engine::Mysql).expect("mysql");
    assert_eq!(mysql.max_age_hours, databases::DEFAULT_MAX_AGE_HOURS);
}

#[test]
fn invalid_database_backup_sections_are_refused() {
    let unknown = "databases:\n  backups:\n    oracle:\n      paths: [/x/*]\n";
    let error = Config::merge(with_yaml(unknown), env(&[])).expect_err("unknown engine");
    assert!(error.to_string().contains("unknown engine 'oracle'"), "{error}");
    let empty = "databases:\n  backups:\n    postgresql:\n      paths: []\n";
    assert!(Config::merge(with_yaml(empty), env(&[])).is_err());
    let zero = "databases:\n  backups:\n    postgresql:\n      paths: [/x/*]\n      max_age_hours: 0\n";
    assert!(Config::merge(with_yaml(zero), env(&[])).is_err());
    let fast = "databases:\n  interval_secs: 5\n";
    assert!(Config::merge(with_yaml(fast), env(&[])).is_err());
}

#[test]
fn too_frequent_an_updates_inventory_is_refused() {
    // Relancer `dnf` toutes les dix secondes coûterait plus que tout le reste.
    let config = Config::merge(
        file_with_url_and_token(),
        env(&[("DUMBMONIT_AGENT_UPDATES_INTERVAL_SECS", "10")]),
    );
    assert!(config.is_err());
}

#[test]
fn the_long_spelling_of_the_updates_interval_is_accepted() {
    let yaml = "system_health:\n  updates_interval_seconds: 1800\n";
    let file = FileConfig {
        server_url: Some("https://s:8080".into()),
        token: Some("dmon_abc".into()),
        ..dumbmonit_collectors::yaml::from_str(yaml).expect("YAML valide")
    };
    let config = Config::merge(file, env(&[])).expect("configuration");
    assert_eq!(config.system_health.updates_interval, Duration::from_secs(1800));
}

#[test]
fn the_outbound_proxy_comes_from_the_file_or_the_environment() {
    use crate::proxy::ProxyMode;
    // Rien : l'environnement habituel (HTTPS_PROXY…), lu par le client.
    let config = Config::merge(file_with_url_and_token(), env(&[])).unwrap();
    assert_eq!(config.proxy.mode, ProxyMode::Environment);

    let file = from_yaml(
        "proxy: http://jo:s3cret@proxy.corp:3128\nno_proxy: [.corp.lan, 10.0.0.0/8]\n",
    );
    let config = Config::merge(file, env(&[])).unwrap();
    let ProxyMode::Explicit(url) = &config.proxy.mode else { panic!("explicite") };
    assert_eq!(url.url, "http://proxy.corp:3128/");
    assert_eq!(url.username.as_deref(), Some("jo"));
    assert_eq!(config.proxy.no_proxy, [".corp.lan", "10.0.0.0/8"]);
    assert!(!format!("{config:?}").contains("s3cret"), "le mot de passe a fuité");

    // La variable prime, la liste se lit séparée par des virgules.
    let file = from_yaml("proxy: http://proxy.corp:3128\n");
    let vars =
        env(&[("DUMBMONIT_AGENT_PROXY", "none"), ("DUMBMONIT_AGENT_NO_PROXY", "a.lan, b.lan")]);
    let config = Config::merge(file, vars).unwrap();
    assert_eq!(config.proxy.mode, ProxyMode::Direct);
    assert_eq!(config.proxy.no_proxy, ["a.lan", "b.lan"]);

    // Identifiants à part, et refus clairs au démarrage.
    let file = from_yaml("proxy: http://proxy.corp:3128\nproxy_user: jo\n");
    let vars = env(&[("DUMBMONIT_AGENT_PROXY_PASSWORD", "p@ss")]);
    let config = Config::merge(file, vars).unwrap();
    let ProxyMode::Explicit(url) = &config.proxy.mode else { panic!("explicite") };
    assert_eq!(url.password.as_deref(), Some("p@ss"));
    assert!(Config::merge(from_yaml("proxy: https://proxy.corp:3128\n"), env(&[])).is_err());
    assert!(Config::merge(from_yaml("proxy: proxy.corp:3128\n"), env(&[])).is_err());
}

#[test]
fn the_channel_transport_comes_from_the_file_or_the_environment() {
    use crate::ws::TransportMode;
    // Rien : le port des agents d'abord, la WebSocket s'il est injoignable.
    let config = Config::merge(file_with_url_and_token(), env(&[])).unwrap();
    assert_eq!(config.transport, TransportMode::Auto);

    let config = Config::merge(from_yaml("transport: websocket\n"), env(&[])).unwrap();
    assert_eq!(config.transport, TransportMode::WebSocket);
    // La variable prime ; vide (docker-compose.agent.yml), elle laisse le
    // fichier décider.
    let vars = env(&[("DUMBMONIT_AGENT_TRANSPORT", "direct")]);
    let config = Config::merge(from_yaml("transport: websocket\n"), vars).unwrap();
    assert_eq!(config.transport, TransportMode::Direct);
    let vars = env(&[("DUMBMONIT_AGENT_TRANSPORT", "")]);
    let config = Config::merge(from_yaml("transport: websocket\n"), vars).unwrap();
    assert_eq!(config.transport, TransportMode::WebSocket);
    // Une faute de frappe se voit au démarrage.
    assert!(Config::merge(from_yaml("transport: tcp\n"), env(&[])).is_err());
}

#[test]
fn the_token_never_appears_in_the_debug_output() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    let rendered = format!("{config:?}");
    assert!(!rendered.contains("dmon_abc"), "le jeton a fuité : {rendered}");
    assert!(rendered.contains("redacted"));
}

#[test]
fn performance_counters_are_read_from_the_file_in_both_forms() {
    let yaml = r#"
perf_counters:
  - '\Processor(_Total)\% Processor Time'
  - path: '\Memory\Available MBytes'
name: memory_available_mb
  - path: '\Paging File(_Total)\% Usage'
mdaemon: true
"#;
    let config = Config::merge(from_yaml(yaml), env(&[])).expect("configuration");
    let counters = &config.perf_counters.counters;
    assert_eq!(counters.len(), 3);
    assert_eq!(counters[0].name, r"\Processor(_Total)\% Processor Time");
    assert_eq!(counters[1].path, r"\Memory\Available MBytes");
    assert_eq!(counters[1].name, "memory_available_mb");
    // Sans nom, l'étiquette est le chemin lui-même.
    assert_eq!(counters[2].name, r"\Paging File(_Total)\% Usage");
    assert_eq!(config.perf_counters.mdaemon, PresetMode::On);
}

#[test]
fn the_mdaemon_preset_is_automatic_by_default_and_follows_the_environment() {
    let config = Config::merge(file_with_url_and_token(), env(&[])).expect("configuration");
    assert_eq!(config.perf_counters.mdaemon, PresetMode::Auto);
    assert!(config.perf_counters.counters.is_empty());

    let config = Config::merge(from_yaml("mdaemon: auto\n"), env(&[])).expect("configuration");
    assert_eq!(config.perf_counters.mdaemon, PresetMode::Auto);
    let config = Config::merge(from_yaml("mdaemon: false\n"), env(&[])).expect("configuration");
    assert_eq!(config.perf_counters.mdaemon, PresetMode::Off);

    let config = Config::merge(
        from_yaml("mdaemon: false\n"),
        env(&[
            ("DUMBMONIT_AGENT_MDAEMON", "on"),
            (
                "DUMBMONIT_AGENT_PERF_COUNTERS",
                r"\Processor Information(0,1)\% Processor Time; \Memory\Available MBytes ;",
            ),
        ]),
    )
    .expect("configuration");
    assert_eq!(config.perf_counters.mdaemon, PresetMode::On);
    // Point-virgule : la virgule appartient ici au nom de l'instance.
    let paths: Vec<&str> =
        config.perf_counters.counters.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(
        paths,
        [r"\Processor Information(0,1)\% Processor Time", r"\Memory\Available MBytes"]
    );
}
