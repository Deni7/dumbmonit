//! Tests du client de l'agent : chemins, erreurs et nettoyage des messages.

use super::*;

#[test]
fn the_ingest_url_is_built_without_a_double_slash() {
    assert_eq!(ingest_url("http://serveur:8080"), "http://serveur:8080/api/ingest");
    assert_eq!(ingest_url("http://serveur:8080/"), "http://serveur:8080/api/ingest");
    assert_eq!(
        ingest_url("https://mon.domaine/dumbmonit/"),
        "https://mon.domaine/dumbmonit/api/ingest"
    );
}

#[test]
fn the_agent_channel_paths_follow_the_shared_contract() {
    assert_eq!(commands_path(None), "/api/agent/commands");
    assert_eq!(commands_path(Some(42)), "/api/agent/commands/42");
    assert_eq!(relay_path(None), "/api/agent/relay");
    assert_eq!(relay_path(Some(3)), "/api/agent/relay/3");
}

#[test]
fn only_the_old_server_refusal_switches_the_key_back_to_the_url() {
    assert!(wants_legacy_key(r#"{"error":"the 'key' query parameter is required"}"#));
    assert!(!wants_legacy_key(r#"{"error":"the machine key is required (header x)"}"#));
    assert!(!wants_legacy_key("the 'key' query parameter is required"), "hors enveloppe");
}

#[test]
fn a_rejected_batch_is_not_retried_but_an_outage_is() {
    assert!(PushError::Transport("connexion refusée".into()).is_retryable());
    assert!(PushError::Unauthorized.is_retryable());
    // Quelqu'un peut ouvrir la fenêtre de réenrôlement pendant qu'on patiente.
    assert!(PushError::Forbidden("rebind me".into()).is_retryable());
    assert!(!PushError::Rejected { status: 400, message: "lot vide".into() }.is_retryable());
}

#[tokio::test]
async fn a_requested_report_waits_for_the_loop_and_close_requests_make_one_batch() {
    let client = PushClient::new("http://serveur:8080", "dmon_x", None, Duration::from_secs(1))
        .expect("client");
    let signal = client.report_requested();
    // Demandé avant que la boucle n'attende (fin de l'attente longue du
    // relais pendant un envoi) : la demande est gardée.
    client.clone().request_report();
    client.request_report();
    // La demande est déjà là : l'attente rend la main aussitôt, le plafond
    // ne sert qu'à ne pas pendre si elle s'était perdue.
    tokio::time::timeout(Duration::from_secs(5), signal.notified()).await.expect("demande gardée");
    let again = tokio::time::timeout(Duration::from_millis(50), signal.notified()).await;
    assert!(again.is_err(), "deux demandes rapprochées, un seul lot");
}

#[test]
fn the_binding_secret_travels_in_its_own_header_only_once_it_exists() {
    let client = PushClient::new("http://serveur:8080", "dmon_x", None, Duration::from_secs(1))
        .expect("client");
    assert!(client.secret.read().unwrap().is_none(), "pas encore lié : aucun en-tête");

    client.set_secret(Some("dmab_abc".into()));
    assert_eq!(client.secret.read().unwrap().as_deref(), Some("dmab_abc"));
    // Le secret est partagé : la boucle des commandes tient une copie du
    // client et doit le voir apparaître sans être reconstruite.
    let twin = client.clone();
    assert_eq!(twin.secret.read().unwrap().as_deref(), Some("dmab_abc"));
    twin.set_secret(None);
    assert!(client.secret.read().unwrap().is_none());
}

#[test]
fn a_refusal_of_this_machine_is_told_apart_from_a_refused_token() {
    // 401 : le jeton. 403 : cette machine-ci. Les deux ne se réparent pas de
    // la même façon, et le message du serveur est ce qui le dit.
    assert!(matches!(PushError::Unauthorized, PushError::Unauthorized));
    let forbidden = PushError::Forbidden("Allow re-enrolment in the interface.".into());
    assert_eq!(forbidden.to_string(), "Allow re-enrolment in the interface.");
}

#[test]
fn the_api_error_message_is_extracted_from_its_envelope() {
    assert_eq!(extract_error_message(r#"{"error":"jeton révoqué"}"#), "jeton révoqué");
}

#[test]
fn a_non_json_error_body_is_kept_as_is_but_truncated() {
    let long = "x".repeat(500);
    assert_eq!(extract_error_message(&long).len(), 200);
    assert_eq!(extract_error_message("  502 Bad Gateway  "), "502 Bad Gateway");
}

#[test]
fn the_token_is_scrubbed_from_transport_errors() {
    let message = "error sending request for url (http://s/api/ingest?t=dmon_secret)";
    let cleaned = sanitise(message, "dmon_secret");
    assert!(!cleaned.contains("dmon_secret"), "le jeton a fuité : {cleaned}");
}

#[test]
fn error_messages_are_written_in_english() {
    assert_eq!(PushError::Unauthorized.to_string(), "enrollment token rejected");
    assert!(
        PushError::Transport("connexion refusée".into())
            .to_string()
            .starts_with("server unreachable")
    );
}
