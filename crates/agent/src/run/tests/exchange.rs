//! Tests de la liaison, des lots refusés et de la boucle d'envoi.

use super::*;

#[tokio::test]
async fn the_binding_secret_issued_by_the_server_is_kept_then_forgotten_on_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let answer = std::sync::Arc::new(std::sync::Mutex::new((200u16, ACK.to_string())));
    let shared = answer.clone();
    let (url, calls) = crate::testsupport::fake_http(move |_| shared.lock().unwrap().clone()).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    // Le secret est rangé sur disque, la période adoptée, le tampon vidé.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("agent-secret")).unwrap().trim(),
        "dmab_attribue"
    );
    assert_eq!(agent.interval, Duration::from_secs(60));
    assert!(agent.buffer.is_empty());

    // Au tour suivant, le secret accompagne le lot.
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    agent.flush().await;
    assert_eq!(calls.all()[1].header("x-dumbmonit-agent-secret"), Some("dmab_attribue"));

    // Le serveur ne reconnaît plus la liaison : on l'oublie, on garde les
    // mesures, et le refus n'est signalé qu'une fois.
    *answer.lock().unwrap() =
        (403, serde_json::json!({ "error": dumbmonit_proto::BINDING_MISMATCH }).to_string());
    let samples = agent.collect().await;
    let count = samples.len();
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Failed(PushError::Forbidden(_))));
    assert_eq!(agent.buffer.len(), count);
    assert!(agent.binding_refused);
    assert!(!dir.path().join("agent-secret").exists());
    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
}

#[tokio::test]
async fn an_enrolment_waiting_for_approval_keeps_everything_and_retries() {
    // Le serveur fait attendre la machine : ni secret effacé, ni mesure
    // perdue, et le journal le dit une seule fois.
    let dir = tempfile::tempdir().unwrap();
    let body = serde_json::json!({ "error": dumbmonit_proto::ENROLMENT_PENDING });
    let pending = (403u16, body.to_string());
    let answer = std::sync::Arc::new(std::sync::Mutex::new(pending));
    let shared = answer.clone();
    let (url, _calls) =
        crate::testsupport::fake_http(move |_| shared.lock().unwrap().clone()).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    let count = samples.len();
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Failed(PushError::Forbidden(_))));
    assert!(agent.awaiting_approval);
    assert!(!agent.binding_refused, "une attente n'est pas un refus de liaison");
    assert_eq!(agent.buffer.len(), count, "les mesures restent en tampon");

    // Acceptée : le lot suivant passe et l'attente est close.
    *answer.lock().unwrap() = (200, ACK.to_string());
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(!agent.awaiting_approval);
}

#[tokio::test]
async fn a_403_that_is_not_the_binding_refusal_keeps_the_secret() {
    // Un mandataire inverse ou un pare-feu applicatif répond 403 : rien ne
    // dit que le serveur a oublié la liaison. Effacer le secret obligerait à
    // réenrôler à la main toutes les machines du parc.
    let dir = tempfile::tempdir().unwrap();
    let answer = std::sync::Arc::new(std::sync::Mutex::new((200u16, ACK.to_string())));
    let shared = answer.clone();
    let (url, calls) = crate::testsupport::fake_http(move |_| shared.lock().unwrap().clone()).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));

    for body in [
        "<html><body>403 Forbidden</body></html>".to_string(),
        r#"{"error":"Request blocked by WAF"}"#.to_string(),
        r#"{"error":"Operator or admin role required."}"#.to_string(),
    ] {
        *answer.lock().unwrap() = (403, body.clone());
        let samples = agent.collect().await;
        let count = samples.len();
        agent.buffer.push(samples);
        assert!(matches!(agent.flush().await, FlushOutcome::Failed(PushError::Forbidden(_))));
        assert!(agent.buffer.len() >= count, "les mesures restent en tampon");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("agent-secret")).unwrap().trim(),
            "dmab_attribue",
            "secret effacé sur un 403 étranger : {body}"
        );
    }
    // Et il accompagne toujours les requêtes suivantes.
    *answer.lock().unwrap() = (200, ACK.to_string());
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    let last = calls.all().pop().expect("requête");
    assert_eq!(last.header("x-dumbmonit-agent-secret"), Some("dmab_attribue"));
}

#[tokio::test]
async fn a_batch_the_server_rejects_is_dropped_rather_than_resent() {
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) =
        crate::testsupport::fake_http(|_| (400, r#"{"error":"bad batch"}"#.to_string())).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    agent.buffer.push(samples);
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(agent.buffer.is_empty(), "un lot refusé ne se réémet pas");
    assert!(matches!(agent.flush().await, FlushOutcome::Idle));
    assert_eq!(calls.all().len(), 1);
}

#[tokio::test]
async fn a_proxy_error_without_the_server_envelope_keeps_the_backlog() {
    // Un mandataire répond 404 pendant une maintenance : tout le retard
    // accumulé doit survivre, et repartir une fois le chemin rétabli.
    let dir = tempfile::tempdir().unwrap();
    let answer = std::sync::Arc::new(std::sync::Mutex::new((
        404u16,
        "<html><body>404 Not Found</body></html>".to_string(),
    )));
    let shared = answer.clone();
    let (url, _) = crate::testsupport::fake_http(move |_| shared.lock().unwrap().clone()).await;
    let mut agent = agent_for(&url, &dir);
    let samples = agent.collect().await;
    let count = samples.len();
    agent.buffer.push(samples);

    assert!(matches!(agent.flush().await, FlushOutcome::Failed(_)));
    assert_eq!(agent.buffer.len(), count, "le retard ne doit pas être jeté");

    *answer.lock().unwrap() = (200, ACK.to_string());
    assert!(matches!(agent.flush().await, FlushOutcome::Sent { .. }));
    assert!(agent.buffer.is_empty());
}

#[tokio::test]
async fn a_batch_too_large_for_a_proxy_is_split_until_it_passes() {
    // Le mandataire refuse tout corps de plus de 3 mesures.
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) = crate::testsupport::fake_http(|req| {
        let samples = req.body.matches("\"ts_ms\"").count();
        if samples > 3 {
            (413, "<html>413 Request Entity Too Large</html>".to_string())
        } else {
            (200, ACK.to_string())
        }
    })
    .await;
    let mut agent = agent_for(&url, &dir);
    let samples: Vec<Sample> = (0..10)
        .map(|i| Sample::new("cpu_usage_percent", 1.0, dumbmonit_proto::MetricKind::Gauge, i))
        .collect();
    agent.buffer.push(samples);

    let mut rounds = 0;
    while !agent.buffer.is_empty() {
        rounds += 1;
        assert!(rounds < 50, "le lot ne passe jamais");
        agent.flush().await;
    }
    // Rien n'a été perdu : les dix mesures sont arrivées, par lots de 3 au plus.
    let delivered: usize = calls
        .all()
        .iter()
        .map(|req| req.body.matches("\"ts_ms\"").count())
        .filter(|&n| n <= 3)
        .sum();
    assert_eq!(delivered, 10);
    assert!(agent.batch_limit <= 3);
}

#[tokio::test]
async fn a_single_measurement_too_large_for_any_proxy_is_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let (url, _) =
        crate::testsupport::fake_http(|_| (413, "Request Entity Too Large".to_string())).await;
    let mut agent = agent_for(&url, &dir);
    agent.buffer.push(vec![Sample::new(
        "cpu_usage_percent",
        1.0,
        dumbmonit_proto::MetricKind::Gauge,
        1,
    )]);
    let mut rounds = 0;
    while !agent.buffer.is_empty() {
        rounds += 1;
        assert!(rounds < 50, "une mesure seule ne doit pas bloquer le tampon");
        agent.flush().await;
    }
}

#[test]
fn a_batch_entirely_refused_is_reported_once_as_a_clock_problem() {
    let mut agent = Agent::new(config()).expect("agent");
    assert!(!agent.note_acceptance(10, 10));
    assert!(!agent.note_acceptance(0, 0), "un lot vide ne dit rien");
    assert!(agent.note_acceptance(10, 0), "tout refusé : on le dit");
    assert!(!agent.note_acceptance(10, 0), "une seule fois");
    assert!(!agent.note_acceptance(10, 10));
    assert!(agent.note_acceptance(10, 0), "et de nouveau après un retour à la normale");
}

#[tokio::test]
async fn run_once_reports_whether_the_server_accepted_the_batch() {
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) = crate::testsupport::fake_http(|req| {
        if req.path == "/api/ingest" { (200, ACK.to_string()) } else { (200, "[]".into()) }
    })
    .await;
    let mut agent = agent_for(&url, &dir);
    agent.config.commands = true;
    agent.commands = CommandRunner::new(
        true,
        agent.client.clone(),
        agent.identity.key(),
        std::path::Path::new("/inexistant/docker.sock"),
    );
    agent.run_once().await.expect("lot accepté");
    // Le lot part, puis l'agent demande s'il a des commandes à exécuter.
    assert_eq!(calls.count("POST", "/api/ingest"), 1);
    assert_eq!(calls.count("GET", "/api/agent/commands"), 1);

    let dir = tempfile::tempdir().unwrap();
    let (url, _) = crate::testsupport::fake_http(|_| (503, "down".to_string())).await;
    let error = agent_for(&url, &dir).run_once().await.unwrap_err().to_string();
    assert!(error.contains("503"), "{error}");
}

#[tokio::test]
async fn the_loop_collects_sends_and_flushes_before_stopping() {
    let dir = tempfile::tempdir().unwrap();
    let (url, calls) = crate::testsupport::fake_http(|_| (200, ACK.to_string())).await;
    let agent = agent_for(&url, &dir);
    let (trigger, shutdown) = crate::shutdown::channel();
    // Premier cycle : une collecte immédiate puis son envoi, et l'arrêt
    // est demandé dès que le lot est parti.
    let stopper = async {
        let started = std::time::Instant::now();
        while calls.count("POST", "/api/ingest") == 0 {
            assert!(started.elapsed() < Duration::from_secs(30), "aucun lot envoyé");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        trigger.fire();
    };
    let (result, ()) = tokio::join!(agent.run(shutdown), stopper);
    result.expect("sortie propre");
}

#[tokio::test]
async fn a_loop_that_cannot_reach_the_server_keeps_its_measurements_and_stops_anyway() {
    let dir = tempfile::tempdir().unwrap();
    let mut agent = agent_for("http://127.0.0.1:1", &dir);
    agent.config.modules = Some(Default::default());
    let (trigger, shutdown) = crate::shutdown::channel();
    let stopper = async {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        trigger.fire();
    };
    let (result, ()) = tokio::join!(agent.run(shutdown), stopper);
    result.expect("sortie propre");
}
