//! Le client contre un vrai serveur HTTP local : ce qui part (jeton, secret de
//! liaison, corps) et la façon dont chaque statut de réponse est qualifié.

use super::*;
use crate::testsupport::fake_http;
use dumbmonit_proto::{AgentIdentity, CommandStatus};

fn batch() -> PushBatch {
    let identity = AgentIdentity {
        hostname: "nas".into(),
        os: "linux".into(),
        os_version: None,
        kernel_version: None,
        arch: None,
        agent_version: "0.0.0".into(),
        commands_enabled: None,
        relay: false,
        site: None,
        relay_control: false,
        relay_local: false,
        machine_id: None,
        tags: Default::default(),
        binding_supported: true,
        self_update: None,
        relay_gateway: None,
        relay_gateway_check: None,
        remediation: None,
    };
    PushBatch::new(identity, Vec::new(), 1_000)
}

fn client(url: &str, secret: Option<&str>) -> PushClient {
    PushClient::new(url, "dmon_jeton", secret.map(str::to_string), Duration::from_secs(5))
        .expect("client")
}

const ACK: &str = r#"{"target_id":12,"accepted":3,"interval_secs":45,"registered":true,
    "agent_secret":"dmab_nouveau","bound":true}"#;

#[tokio::test]
async fn an_accepted_batch_returns_the_ack_and_carries_the_credentials() {
    let (url, calls) = fake_http(|_| (200, ACK.to_string())).await;
    let ack = client(&url, Some("dmab_actuel")).send(&batch()).await.expect("accusé");
    assert_eq!((ack.target_id, ack.accepted, ack.interval_secs), (12, 3, 45));
    assert_eq!(ack.agent_secret.as_deref(), Some("dmab_nouveau"));
    assert!(ack.bound);

    let request = &calls.all()[0];
    assert_eq!((request.method.as_str(), request.path.as_str()), ("POST", "/api/ingest"));
    assert_eq!(request.header("authorization"), Some("Bearer dmon_jeton"));
    assert_eq!(request.header("x-dumbmonit-agent-secret"), Some("dmab_actuel"));
    assert!(request.body.contains("\"hostname\":\"nas\""), "{}", request.body);

    // Pas encore lié : l'en-tête n'est pas envoyé du tout.
    client(&url, None).send(&batch()).await.expect("accusé");
    assert!(calls.all()[1].header("x-dumbmonit-agent-secret").is_none());
}

#[tokio::test]
async fn each_status_is_qualified_for_the_retry_decision() {
    for (status, body, retryable) in [
        (401, "", true),
        (403, r#"{"error":"Allow re-enrolment"}"#, true),
        (429, r#"{"error":"slow down"}"#, true),
        (503, "maintenance", true),
        (400, r#"{"error":"empty batch"}"#, false),
        (422, "", false),
        // Sans l'enveloppe d'erreur du serveur, un 400 ou un 404 vient d'un
        // mandataire (chemin mal routé, maintenance) : le lot n'y est pour rien.
        (400, "<html>400 Bad Request</html>", true),
        (404, "<html>404 Not Found</html>", true),
        (404, r#"{"error":"Unknown command."}"#, false),
        // Trop gros pour un intermédiaire : à couper, pas à jeter.
        (413, "<html>413 Request Entity Too Large</html>", true),
    ] {
        let reply = body.to_string();
        let (url, _) = fake_http(move |_| (status, reply.clone())).await;
        let error = client(&url, None).send(&batch()).await.expect_err("refus");
        assert_eq!(error.is_retryable(), retryable, "{status}: {error}");
        match (status, &error) {
            (401, PushError::Unauthorized) => {}
            (403, PushError::Forbidden(message)) => assert_eq!(message, "Allow re-enrolment"),
            (400 | 404 | 429 | 503, PushError::Transport(message)) => {
                assert!(message.starts_with(&format!("HTTP {status}")), "{message}")
            }
            (413, PushError::TooLarge(_)) => {}
            (400 | 404 | 422, PushError::Rejected { status: got, .. }) => assert_eq!(*got, status),
            other => panic!("qualification inattendue : {other:?}"),
        }
    }
}

#[tokio::test]
async fn an_unreadable_ack_or_a_dead_server_is_a_transport_error() {
    let (url, _) = fake_http(|_| (200, "ça n'est pas du JSON".to_string())).await;
    let error = client(&url, None).send(&batch()).await.unwrap_err();
    assert!(matches!(&error, PushError::Transport(m) if m.contains("unreadable response")));

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let error = client(&dead, None).send(&batch()).await.unwrap_err();
    assert!(matches!(error, PushError::Transport(_)), "{error}");
    assert!(!error.to_string().contains("dmon_jeton"));
    assert!(client(&dead, None).fetch_commands("k").await.is_err());
    assert!(client(&dead, None).report_probe("k", 1, &outcome()).await.is_err());
    let report = CommandReport { status: CommandStatus::Done, result: String::new() };
    assert!(client(&dead, None).report_command("k", 1, &report).await.is_err());
    let wait = Duration::from_secs(1);
    assert!(client(&dead, None).fetch_probes("k", wait, wait).await.is_err());
}

fn outcome() -> ProbeOutcome {
    ProbeOutcome {
        duration_ms: 12,
        error: Some("timeout".into()),
        samples: Vec::new(),
        profile_id: None,
        netconfig: None,
    }
}

#[tokio::test]
async fn commands_are_fetched_and_reported_with_the_machine_key() {
    let (url, calls) = fake_http(|req| match req.method.as_str() {
        "GET" => (
            200,
            r#"[{"id":5,"kind":"container.restart","args":{"name":"web"},"created_at_ms":1}]"#
                .into(),
        ),
        _ => (204, String::new()),
    })
    .await;
    let client = client(&url, Some("dmab_s"));

    let commands = client.fetch_commands("nas salon").await.expect("commandes");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].arg_str("name"), Some("web"));
    assert_eq!(calls.all()[0].path, "/api/agent/commands");
    assert_eq!(calls.all()[0].header("x-dumbmonit-agent-key"), Some("nas salon"));

    let report = CommandReport { status: CommandStatus::Done, result: "ok".into() };
    client.report_command("nas salon", 5, &report).await.expect("compte rendu");
    let sent = &calls.all()[1];
    assert_eq!(sent.path, "/api/agent/commands/5");
    assert_eq!(sent.header("x-dumbmonit-agent-key"), Some("nas salon"));
    assert!(sent.body.contains("\"done\""), "{}", sent.body);
}

#[tokio::test]
async fn relay_probes_are_polled_with_a_wait_and_reported() {
    let (url, calls) = fake_http(|req| match req.method.as_str() {
        "GET" => (200, "[]".to_string()),
        _ => (403, r#"{"error":"not your device"}"#.to_string()),
    })
    .await;
    let client = client(&url, None);
    let jobs = client.fetch_probes("site-a", Duration::from_secs(25), Duration::from_secs(5)).await;
    assert!(jobs.expect("sondes").is_empty());
    assert_eq!(calls.all()[0].path, "/api/agent/relay?wait=25");
    assert_eq!(calls.all()[0].header("x-dumbmonit-agent-key"), Some("site-a"));

    let error = client.report_probe("site-a", 3, &outcome()).await.unwrap_err();
    assert!(matches!(&error, PushError::Forbidden(m) if m == "not your device"), "{error}");
    assert_eq!(calls.all()[1].path, "/api/agent/relay/3");
    assert!(calls.all()[1].body.contains("timeout"));
}

#[tokio::test]
async fn an_older_server_gets_the_key_back_in_the_url_and_it_sticks() {
    // Serveur antérieur à l'en-tête : il réclame `?key=` en 400.
    let (url, calls) = fake_http(|req| {
        if req.path.contains("key=") {
            (200, "[]".to_string())
        } else {
            (400, r#"{"error":"the 'key' query parameter is required"}"#.to_string())
        }
    })
    .await;
    let client = client(&url, None);
    let wait = Duration::from_secs(5);
    assert!(client.fetch_probes("site-a", Duration::ZERO, wait).await.expect("sondes").is_empty());
    assert_eq!(
        calls.lines(),
        ["GET /api/agent/relay?wait=0", "GET /api/agent/relay?key=site-a&wait=0"]
    );
    // Les requêtes suivantes partent directement à l'ancienne.
    client.fetch_commands("site-a").await.expect("commandes");
    assert_eq!(calls.lines()[2], "GET /api/agent/commands?key=site-a");
}

#[tokio::test]
async fn a_transport_error_names_its_root_cause_first_and_hides_the_query() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = client(&dead, None);
    // Même un serveur ancien (clé dans l'URL) ne la fait pas fuiter.
    client.legacy_key.store(true, Ordering::Relaxed);
    let wait = Duration::from_secs(1);
    let error = client.fetch_probes("cle-secrete-0123", wait, wait).await.unwrap_err().to_string();
    assert!(!error.contains("cle-secrete-0123"), "la clé a fuité : {error}");
    assert!(!error.contains("wait="), "paramètres recopiés : {error}");
    assert!(error.starts_with("server unreachable: cannot connect: "), "{error}");
    // La cause système vient avant l'URL, qui ferme le message.
    let cause = error.find("refused").or_else(|| error.find("os error"));
    let cause = cause.unwrap_or_else(|| panic!("cause absente : {error}"));
    assert!(cause < error.find("http://").expect("url"), "{error}");
}

#[tokio::test]
async fn any_favourable_answer_wakes_whoever_waits_for_the_server() {
    let (url, _) = fake_http(|_| (200, "[]".to_string())).await;
    let client = client(&url, None);
    let signal = client.reachable();
    let waiting = signal.notified();
    client.fetch_commands("k").await.expect("commandes");
    tokio::time::timeout(Duration::from_secs(1), waiting).await.expect("réveil attendu");
}

#[tokio::test]
async fn the_encrypted_channel_crosses_an_http_proxy_by_connect_end_to_end() {
    use base64::Engine as _;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    // Faux mandataire : il note la demande CONNECT, l'accepte, puis relève le
    // premier octet du tunnel. Un ClientHello TLS (0x16), pas du HTTP : la
    // session de l'agent va au-delà du mandataire, qui ne voit que du chiffré.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_url = format!("http://jo:s3cret@{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new((String::new(), 0u8)));
    let record = seen.clone();
    tokio::spawn(async move {
        let Ok((mut socket, _)) = listener.accept().await else { return };
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            if socket.read(&mut byte).await.unwrap_or(0) == 0 {
                return;
            }
            head.push(byte[0]);
        }
        let _ = socket.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await;
        let mut first = [0u8; 1];
        let _ = socket.read_exact(&mut first).await;
        *record.lock().unwrap() = (String::from_utf8_lossy(&head).into_owned(), first[0]);
    });

    let proxy = crate::proxy::ProxyConfig::parse(Some(proxy_url.as_str()), Vec::new(), None, None)
        .expect("mandataire");
    let channel = "https://agents.example.test:8443";
    let client = client(channel, None).with_proxy(proxy).expect("client");
    client.set_pins(vec!["a".repeat(64)]);
    client.use_tls(channel, None).expect("canal chiffré");
    // Personne au bout du tunnel : l'envoi échoue, seul compte ce que le
    // mandataire a vu passer.
    assert!(client.send(&batch()).await.is_err());

    let (head, first) = seen.lock().unwrap().clone();
    assert!(head.starts_with("CONNECT agents.example.test:8443 HTTP/1.1\r\n"), "{head}");
    let credentials = base64::engine::general_purpose::STANDARD.encode("jo:s3cret");
    let expected = format!("proxy-authorization: basic {}", credentials.to_lowercase());
    assert!(head.to_lowercase().contains(&expected), "identifiants absents : {head}");
    assert_eq!(first, 0x16, "un ClientHello TLS, pas du HTTP en clair");
    assert!(!head.contains("/api/ingest") && !head.contains("dmon_jeton"), "{head}");
}
