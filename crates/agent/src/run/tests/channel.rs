//! Tests du canal chiffré, des épingles et du repli de transport.

use super::*;

#[tokio::test]
async fn an_unreachable_server_keeps_the_measurements_in_the_buffer() {
    let mut agent = Agent::new(config()).expect("agent");
    let samples = agent.collect().await;
    let count = samples.len();
    agent.buffer.push(samples);

    // L'adresse configurée ne peut accepter aucune connexion : l'envoi échoue
    // à coup sûr, et c'est exactement le scénario « serveur redémarré ».
    match agent.flush().await {
        FlushOutcome::Failed(error) => {
            // Une panne de liaison se réessaie ; c'est ce qui distingue ce cas
            // d'un lot refusé, que l'agent abandonnerait.
            assert!(error.is_retryable(), "{error}");
        }
        other => panic!("échec attendu, obtenu {other:?}"),
    }
    assert_eq!(agent.buffer.len(), count, "aucune mesure ne doit être perdue");

    // La temporisation ne grandit qu'au moment où la boucle la consomme.
    assert_eq!(agent.backoff.attempts(), 0);
    agent.backoff.next_delay();
    assert_eq!(agent.backoff.attempts(), 1);
}

#[tokio::test]
async fn with_a_pinned_server_nothing_is_ever_sent_in_clear() {
    // L'agent installé avec l'empreinte n'a pas encore pu s'enrôler (port
    // des agents injoignable) : ses mesures attendent, elles ne partent pas
    // en clair vers l'ancien canal pour autant.
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let mut config = config();
    config.server_url = url;
    config.secret_path = dir.path().join("agent-secret");
    config.tls_url = Some(format!("https://127.0.0.1:{}", closed_port()));
    config.ca_pin = Some(PIN.to_string());
    let mut agent = Agent::new(config).expect("agent");
    assert!(agent.client.is_encrypted(), "le client ne connaît plus que le canal chiffré");
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    assert!(!agent.buffer.is_empty(), "les mesures sont gardées");
    assert!(calls.all().is_empty(), "rien n'est parti en clair : {:?}", calls.lines());
}

#[tokio::test]
async fn an_agent_migrating_from_the_plain_channel_keeps_reporting_until_enrolled() {
    // Annonce reçue sur l'ancien canal, port des agents fermé : l'agent
    // tente l'enrôlement, échoue, et continue par l'ancien canal — il n'a
    // encore jamais été enrôlé, rien ne l'en empêche.
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let mut agent = agent_for(&url, &dir);
    agent.client.set_offer(dumbmonit_proto::TlsOffer {
        port: closed_port(),
        pins: vec![PIN.to_string()],
        url: None,
    });
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    let state = agent.tls.clone().expect("canal adopté");
    assert!(state.url.starts_with("https://127.0.0.1:"), "{state:?}");
    assert_eq!(state.pins, [PIN]);
    assert!(!state.locked);
    assert_eq!(calls.count("POST", "/api/ingest"), 1);
    assert!(agent.tls_retry_at.is_some(), "nouvel essai d'enrôlement plus tard");
}

#[tokio::test]
async fn the_dedicated_agent_address_announced_by_the_server_is_the_one_dialled() {
    // Le serveur a une adresse dédiée aux agents : l'agent migre vers elle,
    // pas vers l'hôte de son `server_url`.
    let dir = tempfile::tempdir().unwrap();
    let (url, _calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let mut agent = agent_for(&url, &dir);
    let dedicated = format!("https://localhost:{}", closed_port());
    agent.client.set_offer(dumbmonit_proto::TlsOffer {
        port: closed_port(),
        pins: vec![PIN.to_string()],
        url: Some(dedicated.clone()),
    });
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert_eq!(agent.tls.clone().expect("canal adopté").url, dedicated);
}

#[tokio::test]
async fn once_enrolled_an_agent_never_falls_back_to_the_plain_channel() {
    // Un état rangé verrouillé (déjà enrôlé une fois), certificat perdu :
    // l'agent doit se réenrôler, pas reparler en clair.
    let dir = tempfile::tempdir().unwrap();
    let mut state =
        TlsState::new(format!("https://127.0.0.1:{}", closed_port()), vec![PIN.into()]);
    state.locked = true;
    state.save(&crate::tls::beside(&dir.path().join("agent-secret"))).unwrap();
    let (url, calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    assert!(calls.all().is_empty(), "{:?}", calls.lines());
}

#[tokio::test]
async fn in_auto_mode_an_unreachable_agent_port_falls_back_to_the_websocket_and_keeps_it() {
    use crate::ws::Route;

    // Agent enrôlé dont le port des agents est fermé ; l'adresse de
    // l'interface porte la WebSocket (Cloudflare Tunnel, mandataire).
    let dir = tempfile::tempdir().unwrap();
    let (server, pin, calls) = crate::testsupport::fake_channel(channel_ingest).await;
    let tls_path = crate::tls::beside(&dir.path().join("agent-secret"));
    let mut state = enrolled_state(&format!("https://127.0.0.1:{}", closed_port()));
    state.pins = vec![pin];
    state.not_after_ms = Some(i64::MAX);
    state.locked = true;
    state.save(&tls_path).unwrap();
    let interface = format!("http://{server}");
    let mut agent = agent_for(&interface, &dir);
    assert_eq!(agent.client.route(), Route::Direct, "le port des agents d'abord");

    // La vérification d'installation (`--once`) passe : la WebSocket a pris
    // le relais, le lot est accepté à travers elle.
    agent.run_once().await.expect("lot accepté par la WebSocket");
    assert_eq!(agent.client.route(), Route::WebSocket);
    assert_eq!(calls.count("POST", "/api/ingest"), 1);

    // Retenu : au démarrage suivant, la WebSocket d'emblée.
    assert_eq!(TlsState::load(&tls_path).unwrap().transport, Some(Route::WebSocket));
    let restarted = agent_for(&interface, &dir);
    assert_eq!(restarted.client.route(), Route::WebSocket);

    // Imposé dans agent.yaml, `direct` ne bascule jamais.
    let mut config = config();
    config.server_url = interface;
    config.secret_path = dir.path().join("agent-secret");
    config.transport = crate::ws::TransportMode::Direct;
    let direct = Agent::new(config).expect("agent");
    assert_eq!(direct.client.route(), Route::Direct);
    assert_eq!(direct.client.reconsider_route(1).await, None);
}

#[test]
fn an_enrolled_agent_with_a_plain_url_keeps_running_on_its_encrypted_channel_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = enrolled_state("https://192.168.254.240:8443");
    state.save(&crate::tls::beside(&dir.path().join("agent-secret"))).unwrap();
    let agent = Agent::new(plain_config(&dir)).expect("l'agent enrôlé redémarre");
    assert!(agent.client.is_encrypted(), "le canal chiffré rangé, et lui seul");
    assert!(agent.tls_only(), "jamais de repli sur l'ancien canal en clair");
}

#[test]
fn an_agent_never_enrolled_with_a_plain_url_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let refused = match Agent::new(plain_config(&dir)) {
        Ok(_) => panic!("un jeton ne part jamais en clair"),
        Err(error) => error.to_string(),
    };
    assert!(refused.contains("plain http"), "{refused}");
}

#[test]
fn an_enrolled_agent_without_a_usable_channel_does_not_fall_back_to_clear() {
    // Enrôlé un jour (verrouillé), mais sans certificat rangé.
    let dir = tempfile::tempdir().unwrap();
    let mut state = TlsState::new("https://192.168.254.240:8443".into(), vec![PIN.to_string()]);
    state.locked = true;
    state.save(&crate::tls::beside(&dir.path().join("agent-secret"))).unwrap();
    assert!(Agent::new(plain_config(&dir)).is_err(), "certificat absent");
    // Un canal rangé qui ne serait pas en https ne vaut pas mieux.
    let dir = tempfile::tempdir().unwrap();
    let state = enrolled_state("http://192.168.254.240:8443");
    state.save(&crate::tls::beside(&dir.path().join("agent-secret"))).unwrap();
    assert!(Agent::new(plain_config(&dir)).is_err(), "canal rangé en clair");
}

#[test]
fn a_new_pin_in_the_install_command_replaces_the_stored_channel() {
    let mut stored = TlsState::new("https://ancien:8443".into(), vec!["f".repeat(64)]);
    stored.certificate = Some("Y2VydA==".into());
    stored.locked = true;
    let mut config = config();
    config.server_url = "http://serveur:8080".into();
    config.ca_pin = Some(PIN.to_string());
    let state = initial_tls(&config, Some(stored.clone())).expect("canal");
    assert_eq!(state.pins, [PIN]);
    assert_eq!(state.certificate, None, "autre serveur : on repart de zéro");
    assert_eq!(state.url, "https://serveur:8443");

    // La même empreinte : l'état rangé est gardé tel quel.
    config.ca_pin = Some("f".repeat(64));
    assert_eq!(initial_tls(&config, Some(stored.clone())), Some(stored));
    // Ni configuration ni état : pas de canal chiffré (serveur antérieur).
    config.ca_pin = None;
    assert_eq!(initial_tls(&config, None), None);
}
