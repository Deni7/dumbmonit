//! Tests du relais et de la bascule entre serveurs.

use super::*;

// ------------------------------------------- coupures et agent relais

#[test]
fn the_retry_delay_never_exceeds_a_third_of_the_server_silence_limit() {
    // Le serveur déclare une machine muette après 90 s au moins : un agent qui
    // attend plus longtemps entre deux tentatives fabrique des coupures.
    let mut backoff = Backoff::new(BACKOFF_BASE, BACKOFF_MAX);
    for _ in 0..50 {
        assert!(backoff.next_delay() <= Duration::from_secs(30));
    }
}

/// Accusé qui règle la période à 5 s, le plancher : les tests restent courts.
const FAST_ACK: &str = r#"{"target_id":3,"accepted":1,"interval_secs":5,"bound":true}"#;

#[tokio::test]
async fn a_slow_relayed_probe_never_delays_the_reports_of_the_relay_itself() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let (device, held) = tarpit().await;
    let target = dumbmonit_proto::Target {
        id: 41,
        name: "pve002".into(),
        address: format!("http://{device}/"),
        kind: "http".into(),
        profile_id: None,
        parent_id: None,
        interval: Duration::from_secs(60),
        enabled: true,
        // Budget de la sonde HTTP au maximum (5 s par défaut) : elle reste
        // pendue bien au-delà de la période des rapports.
        tags: [
            ("allow_private_targets".to_string(), "true".to_string()),
            ("timeout_seconds".to_string(), "60".to_string()),
        ]
        .into(),
        credential: dumbmonit_proto::Credential::None,
        group_name: String::new(),
        position: 0,
    };
    let job = dumbmonit_proto::ProbeJob { target, timeout_secs: 60, discover: false };
    let jobs = serde_json::to_string(&[job.into_command(1, 0)]).unwrap();
    let polls = std::sync::Arc::new(AtomicUsize::new(0));
    let seen = polls.clone();
    let (url, calls) = crate::testsupport::fake_http(move |req| match req.method.as_str() {
        "POST" if req.path == "/api/ingest" => (200, FAST_ACK.to_string()),
        // La sonde lente part au premier tour ; ensuite le canal est en panne,
        // pour que le relais temporise au lieu de tourner à vide.
        "GET" if seen.fetch_add(1, Ordering::SeqCst) == 0 => (200, jobs.clone()),
        _ => (503, "busy".to_string()),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let agent = relay_agent_for(&url, &dir);
    let (_trigger, shutdown) = crate::shutdown::channel();

    let check = async {
        let started = std::time::Instant::now();
        // La sonde est entre les mains de l'équipement muet…
        while held.load(Ordering::SeqCst) == 0 {
            assert!(started.elapsed() < Duration::from_secs(20), "sonde jamais lancée");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // …et le rapport suivant de la machine part à son heure (période de
        // 5 s), sans attendre la fin de la sonde (60 s). Le plafond laisse
        // de la marge à une machine chargée tout en restant loin des 60 s.
        let before = calls.count("POST", "/api/ingest");
        let stuck = std::time::Instant::now();
        while calls.count("POST", "/api/ingest") <= before {
            assert!(
                stuck.elapsed() < Duration::from_secs(30),
                "rapport bloqué derrière la sonde relayée"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let reported = calls.count("POST", "/api/agent/relay/1");
        assert_eq!(reported, 0, "la sonde devait être encore en vol");
    };
    tokio::select! {
        _ = agent.run(shutdown) => panic!("la boucle s'est arrêtée seule"),
        () = check => {}
    }
}

#[tokio::test]
async fn the_relay_channel_coming_back_ends_the_retry_delay_at_once() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let ingests = std::sync::Arc::new(AtomicUsize::new(0));
    let failed = std::sync::Arc::new(AtomicBool::new(false));
    let answered = std::sync::Arc::new(AtomicBool::new(false));
    let (i, f, a) = (ingests.clone(), failed.clone(), answered.clone());
    let (url, calls) = crate::testsupport::fake_http(move |req| match req.method.as_str() {
        // Premier lot accepté (il lance le relais), le second refusé : la
        // coupure. Puis tout repasse.
        "POST" if req.path == "/api/ingest" => match i.fetch_add(1, Ordering::SeqCst) {
            1 => {
                f.store(true, Ordering::SeqCst);
                (503, "restarting".to_string())
            }
            _ => (200, FAST_ACK.to_string()),
        },
        // Le canal du relais répond une fois, après la coupure des mesures.
        "GET" if f.load(Ordering::SeqCst) && !a.swap(true, Ordering::SeqCst) => {
            (200, "[]".to_string())
        }
        _ => (503, "restarting".to_string()),
    })
    .await;
    let dir = tempfile::tempdir().unwrap();
    let mut agent = relay_agent_for(&url, &dir);
    // Sans le réveil, la reprise attendrait une minute entière.
    agent.backoff = Backoff::new(Duration::from_secs(60), Duration::from_secs(60));
    let (_trigger, shutdown) = crate::shutdown::channel();

    let check = async {
        let started = std::time::Instant::now();
        while !answered.load(Ordering::SeqCst) {
            assert!(started.elapsed() < Duration::from_secs(40), "relais jamais revenu");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        // Bien avant la fin de la temporisation (60 s), même sur une machine chargée.
        let back = std::time::Instant::now();
        while calls.count("POST", "/api/ingest") < 3 {
            assert!(
                back.elapsed() < Duration::from_secs(30),
                "les mesures attendent la fin de la temporisation"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        _ = agent.run(shutdown) => panic!("la boucle s'est arrêtée seule"),
        () = check => {}
    }
}

// ------------------------------------------------ bascule entre serveurs

#[tokio::test]
async fn an_unreachable_primary_hands_over_to_the_backup_then_back() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    // La primaire répond comme un serveur en attente (503) tant qu'elle
    // n'est pas revenue ; revenue, son canal ne connaît pas la route de
    // l'essai (404) et accepte les lots.
    let up = Arc::new(AtomicBool::new(false));
    let primary_up = up.clone();
    let (primary, primary_calls) = crate::testsupport::fake_http(move |request| {
        if !primary_up.load(Ordering::SeqCst) {
            return (503, r#"{"error":"standby"}"#.to_string());
        }
        if request.path.starts_with(failover::PROBE_PATH) {
            return (404, String::new());
        }
        (200, ACK.to_string())
    })
    .await;
    let (backup, backup_calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let dir = tempfile::tempdir().unwrap();
    let mut config = config();
    config.server_url = primary.clone();
    config.fallback_servers =
        vec![crate::config::ServerEndpoint { url: backup.clone(), tls_url: None }];
    config.secret_path = dir.path().join("agent-secret");
    let mut agent = Agent::new(config).expect("agent");
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    // Les premiers échecs ne font pas basculer : un redémarrage du serveur
    // dure quelques secondes.
    for attempt in 1..failover::SWITCH_AFTER {
        assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)), "{attempt}");
        assert_eq!(agent.failover.active(), 0);
    }
    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    assert_eq!(agent.failover.active(), 1, "bascule sur le secours");
    assert_eq!(agent.client.base_url(), backup);
    assert!(agent.failover.take_switched(), "la boucle réessaie sans attendre");

    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    let sent = backup_calls.bodies("POST", "/api/ingest");
    assert_eq!(sent.len(), 1, "le retard part chez le secours");
    let batch: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
    let link = &batch["identity"]["server_link"];
    assert_eq!(link["active"], 1);
    assert_eq!(link["servers"], 2);
    assert_eq!(link["url"], backup.as_str());
    assert!(link["since_ms"].is_i64());
    assert!(link["primary_error"].as_str().unwrap().contains("503"), "{link}");

    // Réessayée, la primaire répond encore en serveur en attente : l'agent
    // reste sur le secours.
    agent.failover.recheck_now();
    agent.check_primary().await;
    assert_eq!(agent.failover.active(), 1);

    // Revenue : l'agent y retourne, et le lot suivant part chez elle.
    up.store(true, Ordering::SeqCst);
    agent.failover.recheck_now();
    agent.check_primary().await;
    assert_eq!(agent.failover.active(), 0);
    assert_eq!(agent.client.base_url(), primary);
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert_eq!(primary_calls.count("GET", failover::PROBE_PATH), 2);
    let last = primary_calls.bodies("POST", "/api/ingest").pop().unwrap();
    let batch: serde_json::Value = serde_json::from_str(&last).unwrap();
    assert_eq!(batch["identity"]["server_link"]["active"], 0);
    assert!(batch["identity"]["server_link"].get("since_ms").is_none());
    assert_eq!(backup_calls.count("POST", "/api/ingest"), 1);
}

#[tokio::test]
async fn without_server_urls_nothing_changes() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("http://127.0.0.1:{}", closed_port());
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    for _ in 0..(failover::SWITCH_AFTER * 3) {
        assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    }
    assert_eq!(agent.client.base_url(), url);
    assert!(!agent.failover.take_switched());
    assert_eq!(agent.identity.server_link, None, "rien n'est annoncé au serveur");
}
