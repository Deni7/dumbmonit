//! Tests des réglages venus du serveur, de l'inventaire et de la collecte.

use super::*;

#[test]
fn the_server_can_tighten_or_relax_the_sampling_period() {
    let mut agent = Agent::new(config()).expect("agent");
    agent.adopt_interval(10);
    assert_eq!(agent.interval, Duration::from_secs(10));

    agent.adopt_interval(120);
    assert_eq!(agent.interval, Duration::from_secs(120));
}

#[test]
fn an_absurd_period_from_the_server_is_clamped() {
    // Un serveur qui renverrait zéro transformerait l'agent en boucle folle sur
    // la machine surveillée : la borne est appliquée côté agent, par principe.
    let mut agent = Agent::new(config()).expect("agent");
    agent.adopt_interval(0);
    assert_eq!(agent.interval, Duration::from_secs(MIN_INTERVAL_SECS));
}

#[tokio::test]
async fn the_relay_mode_set_from_the_server_switches_on_and_off_without_restart() {
    let mut agent = Agent::new(config()).expect("agent");
    let (trigger, shutdown) = crate::shutdown::channel();
    assert!(agent.identity.relay_control, "ce binaire sait suivre le serveur");
    assert!(!agent.identity.relay_local);

    // Serveur antérieur : rien ne change.
    agent.adopt_relay(None);
    agent.sync_relay(&shutdown);
    assert!(agent.relay.is_none());
    assert!(!agent.identity.relay);

    // Allumé dans l'interface : la boucle démarre, l'identité l'annonce.
    agent.adopt_relay(setting(true, Some(" Agence de Lyon ")));
    agent.sync_relay(&shutdown);
    assert!(agent.relay.is_some(), "boucle du relais lancée à chaud");
    assert!(agent.identity.relay);
    assert_eq!(agent.identity.site.as_deref(), Some("Agence de Lyon"));

    // Un second accusé identique ne relance rien.
    agent.adopt_relay(setting(true, Some("Agence de Lyon")));
    agent.sync_relay(&shutdown);
    assert!(agent.relay.is_some());

    // Éteint : la boucle s'arrête, l'identité le dit au lot suivant.
    agent.adopt_relay(setting(false, None));
    agent.sync_relay(&shutdown);
    assert!(agent.relay.is_none(), "boucle du relais arrêtée à chaud");
    assert!(!agent.identity.relay);
    assert_eq!(agent.identity.site, None);
    trigger.fire();
}

#[tokio::test]
async fn the_gateway_opens_with_the_relay_on_the_port_set_from_the_server() {
    let mut agent = Agent::new(config()).expect("agent");
    let (_trigger, shutdown) = crate::shutdown::channel();
    // Un port libre, pour ne gêner personne.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();

    // Pas de mode relais : pas de passerelle, même avec un port.
    agent.adopt_relay(Some(RelaySetting {
        enabled: false,
        site: None,
        gateway_port: Some(port),
        gateway_host: None,
    }));
    agent.sync_gateway(&shutdown).await;
    assert!(agent.gateway.is_none());
    assert_eq!(agent.identity.relay_gateway, None);

    let report = agent.client.report_requested();
    agent.adopt_relay(Some(RelaySetting {
        enabled: true,
        site: None,
        gateway_port: Some(port),
        gateway_host: Some("192.168.10.5".into()),
    }));
    agent.sync_gateway(&shutdown).await;
    assert_eq!(agent.identity.relay_gateway, Some(port), "le port est annoncé au serveur");
    let state = agent.identity.relay_gateway_state.clone().expect("état annoncé");
    assert_eq!(state.listen, Some(format!("0.0.0.0:{port}")), "toutes les interfaces");
    assert_eq!(state.error, None);
    assert!(!state.local, "port réglé depuis l'interface");
    // … tout de suite, par un lot demandé sans attendre la période.
    tokio::time::timeout(Duration::from_secs(5), report.notified())
        .await
        .expect("lot immédiat demandé");
    // Pas de canal chiffré dans ce test : pas d'essai, rien d'inventé.
    assert!(agent.gateway_check_task.is_none());
    assert_eq!(agent.identity.relay_gateway_check, None);

    // Relais sans passerelle : elle se ferme, le relais continue.
    agent.adopt_relay(Some(RelaySetting {
        enabled: true,
        site: None,
        gateway_port: None,
        gateway_host: None,
    }));
    agent.sync_gateway(&shutdown).await;
    assert!(agent.gateway.is_none());
    assert_eq!(agent.identity.relay_gateway, None);
    assert_eq!(agent.identity.relay_gateway_state, None, "rien de demandé, rien à dire");
}

#[tokio::test]
async fn a_gateway_that_cannot_listen_tells_the_server_why_and_retries() {
    let mut agent = Agent::new(config()).expect("agent");
    let (_trigger, shutdown) = crate::shutdown::channel();
    // Le port est tenu par un autre programme sur toutes les interfaces.
    let held = std::net::TcpListener::bind("0.0.0.0:0").expect("port");
    let port = held.local_addr().unwrap().port();
    let open = || {
        Some(RelaySetting {
            enabled: true,
            site: None,
            gateway_port: Some(port),
            gateway_host: None,
        })
    };

    let report = agent.client.report_requested();
    agent.adopt_relay(open());
    agent.sync_gateway(&shutdown).await;
    assert!(agent.gateway.is_none());
    assert_eq!(agent.identity.relay_gateway, None, "rien n'écoute");
    let state = agent.identity.relay_gateway_state.clone().expect("échec annoncé");
    assert_eq!(state.wanted, format!("0.0.0.0:{port}"));
    assert_eq!(state.listen, None);
    let error = state.error.expect("cause");
    assert!(error.contains("already used by another program"), "{error}");
    // L'échec part tout de suite : l'écran d'ajout ne doit pas attendre en silence.
    tokio::time::timeout(Duration::from_secs(5), report.notified())
        .await
        .expect("lot immédiat demandé");

    // Le port se libère : la tentative suivante (au lot suivant) l'ouvre.
    drop(held);
    agent.sync_gateway(&shutdown).await;
    assert_eq!(agent.identity.relay_gateway, Some(port));
    let state = agent.identity.relay_gateway_state.clone().expect("état");
    assert_eq!(state.error, None);
    assert!(state.listen.is_some());
}

#[tokio::test]
async fn relay_true_in_the_local_configuration_wins_over_the_server() {
    let mut config = config();
    config.relay = true;
    config.site = Some("Datacenter".into());
    let mut agent = Agent::new(config).expect("agent");
    let (trigger, shutdown) = crate::shutdown::channel();
    assert!(agent.identity.relay_local, "le serveur doit savoir que c'est verrouillé");

    agent.adopt_relay(setting(false, Some("Ailleurs")));
    agent.sync_relay(&shutdown);
    assert!(agent.relay.is_some(), "le relais local ne s'éteint pas depuis le serveur");
    assert!(agent.identity.relay);
    assert_eq!(agent.identity.site.as_deref(), Some("Datacenter"), "le site local prime");
    trigger.fire();
}

#[tokio::test]
async fn the_inventory_rides_the_next_batch_and_is_forgotten_once_accepted() {
    let dir = tempfile::tempdir().unwrap();
    let answer = std::sync::Arc::new(std::sync::Mutex::new((503u16, "{}".to_string())));
    let shared = answer.clone();
    let (url, calls) = crate::testsupport::fake_http(move |_| shared.lock().unwrap().clone()).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    // Activé après la collecte : la tâche de fond n'a pas à tourner ici.
    agent.config.inventory = true;
    let relever = || SoftwareInventory {
        collected_at_ms: 42,
        source: "dpkg".into(),
        ..SoftwareInventory::default()
    };
    *inventory::lock(&agent.inventory) = Some(relever());

    // Échec réseau côté serveur : l'inventaire part, puis reste en attente.
    agent.buffer.push(samples.clone());
    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    assert!(calls.bodies("POST", "/")[0].contains("\"inventory\""));
    assert!(inventory::lock(&agent.inventory).is_some(), "conservé pour le lot suivant");

    // Accepté : il est oublié.
    *answer.lock().unwrap() = (200, ACK.to_string());
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(calls.bodies("POST", "/")[1].contains("\"inventory\""));
    assert!(inventory::lock(&agent.inventory).is_none());

    // Le lot suivant n'en porte plus.
    agent.buffer.push(samples.clone());
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(!calls.bodies("POST", "/")[2].contains("\"inventory\""));

    // Décoché dans l'interface : rien n'est joint, même s'il en reste un.
    *inventory::lock(&agent.inventory) = Some(relever());
    agent.adopt_disabled_modules(vec!["inventory".into()]);
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(!calls.bodies("POST", "/")[3].contains("\"inventory\""));
}

#[tokio::test]
async fn the_ack_of_a_server_switches_the_relay_on() {
    let dir = tempfile::tempdir().unwrap();
    let ack = r#"{"target_id":3,"accepted":1,"interval_secs":60,"bound":true,
        "relay":{"enabled":true,"site":"Lyon"}}"#;
    let (url, _calls) = crate::testsupport::fake_http(move |_| (200, ack.to_string())).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(agent.identity.relay);
    assert_eq!(agent.identity.site.as_deref(), Some("Lyon"));
}

#[tokio::test]
async fn a_module_unticked_on_the_server_is_no_longer_collected() {
    let mut config = config();
    config.speedtest.min_download_bytes_per_second = Some(1_000_000.0);
    let mut agent = Agent::new(config).expect("agent");
    let threshold = |samples: &[Sample]| {
        samples.iter().any(|s| s.metric == "agent_speedtest_download_min_bytes_per_second")
    };
    assert!(threshold(&agent.collect().await), "le seuil est publié tant que le module vit");

    agent.adopt_disabled_modules(vec!["speedtest".into()]);
    assert!(!agent.wants("speedtest"));
    assert!(agent.wants("docker"), "les autres modules continuent");
    assert!(!threshold(&agent.collect().await));

    // Recoché : l'accusé suivant ne le nomme plus, la collecte reprend.
    agent.adopt_disabled_modules(Vec::new());
    assert!(threshold(&agent.collect().await));
}

#[tokio::test]
async fn a_collection_cycle_produces_metrics_for_the_local_machine() {
    let mut agent = Agent::new(config()).expect("agent");
    let samples = agent.collect().await;

    assert!(!samples.is_empty());
    // Le point qui compte : rien d'invalide ne doit atteindre le serveur.
    assert!(samples.iter().all(|s| s.value.is_finite()));
    assert!(samples.iter().any(|s| s.metric == "memory_total_bytes"));
    assert!(samples.iter().any(|s| s.metric == "agent_collect_seconds"));
    // Le périmètre par défaut : pas de détail par cœur, pas d'interface
    // virtuelle, pas de montage de conteneur, et une entrée d'E/S par disque.
    assert!(samples.iter().all(|s| s.metric != "cpu_core_usage_percent"));
    assert!(
        samples
            .iter()
            .all(|s| { s.labels.get("ifname").is_none_or(|name| !name.starts_with("veth")) })
    );
    assert!(samples.iter().all(|s| {
        s.labels.get("mountpoint").is_none_or(|m| !m.starts_with("/var/lib/docker/"))
    }));
    assert!(
        samples
            .iter()
            .all(|s| { !s.metric.starts_with("disk_") || !s.labels.contains_key("mountpoint") })
    );
}

#[tokio::test]
async fn the_mount_list_is_reread_only_every_tenth_cycle() {
    // Deux cycles consécutifs : le second ne relit pas la liste des montages,
    // et doit pourtant remonter les mêmes systèmes de fichiers que le premier.
    let mut probe = SystemProbe::new(&crate::collect::ProbeConfig::default());
    let first = probe.read();
    let second = probe.read();
    let mounts = |snapshot: &crate::collect::Snapshot| -> Vec<String> {
        snapshot.filesystems.iter().map(|fs| fs.mount_point.clone()).collect()
    };
    assert_eq!(mounts(&first), mounts(&second));
    assert_eq!(first.disk_io.len(), second.disk_io.len());
    assert_eq!(first.cpu.core_count, second.cpu.core_count);
}
