//! Fonctions utilitaires du client : chemins, erreurs, construction de `reqwest`.

use super::*;

/// Chemin des sondes déléguées, ou du compte rendu de l'une d'elles.
pub(super) fn relay_path(id: Option<i64>) -> String {
    match id {
        Some(id) => format!("{RELAY_PATH}/{id}"),
        None => RELAY_PATH.to_string(),
    }
}

/// Vrai si un refus 400 vient d'un serveur qui attend la clé dans l'URL.
pub(super) fn wants_legacy_key(body: &str) -> bool {
    server_error_message(body).is_some_and(|message| message.contains("'key' query parameter"))
}

/// Qualifie une réponse d'erreur du serveur.
pub(super) async fn rejection(response: reqwest::Response) -> PushError {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    rejection_of(status, &body)
}

/// Qualifie un statut d'erreur et son corps.
pub(super) fn rejection_of(status: reqwest::StatusCode, body: &str) -> PushError {
    let from_server = server_error_message(body).is_some();
    let message = extract_error_message(body);

    match status.as_u16() {
        // Sur le canal chiffré, un 401 peut viser le certificat plutôt que le
        // jeton : le serveur le dit par un message convenu.
        401 if message == CERT_REFUSED => PushError::CertificateRefused,
        401 => PushError::Unauthorized,
        // 403 : le jeton est bon, mais pas cette machine-là. Le message du
        // serveur dit quoi faire — il est rendu tel quel dans les journaux.
        403 => PushError::Forbidden(message),
        // 408, 429 et toute la famille 5xx traduisent un serveur momentanément
        // incapable de répondre : les mesures restent en tampon.
        408 | 429 | 500..=599 => PushError::Transport(format!("HTTP {status}: {message}")),
        // 413 ne vient jamais du serveur avec son enveloppe : c'est un plafond de
        // taille, le sien ou celui d'un mandataire. Le lot sera recoupé.
        413 => PushError::TooLarge(message),
        // Sans l'enveloppe `{"error": …}` du serveur, un 400 ou un 404 vient d'un
        // intermédiaire — chemin mal routé, maintenance, filtrage. Le lot n'y
        // est pour rien : jeter tout le retard sur cette foi serait une perte
        // sèche. Les mesures restent en tampon, l'agent réessaie.
        400 | 404 if !from_server => PushError::Transport(format!("HTTP {status}: {message}")),
        other => PushError::Rejected { status: other, message },
    }
}

/// Client HTTP d'un canal : en clair ou derrière l'ancien mandataire tel que
/// configuré, ou, avec `tls`, canal chiffré strict — HTTPS seulement, sans
/// redirection. `proxy` : le mandataire sortant, traversé par `CONNECT`.
pub(super) fn http_client(
    timeout: Duration,
    tls: Option<rustls::ClientConfig>,
    proxy: &ProxyConfig,
) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .timeout(timeout)
        // Le socket est réutilisé d'un envoi au suivant : sur un lien distant,
        // rétablir TLS toutes les trente secondes coûterait plus cher que la
        // collecte elle-même.
        .pool_idle_timeout(Duration::from_secs(300))
        .user_agent(concat!("dumbmonit-agent/", env!("CARGO_PKG_VERSION")));
    if let Some(config) = tls {
        builder = builder
            .tls_backend_preconfigured(config)
            .https_only(true)
            .redirect(reqwest::redirect::Policy::none())
            // En HTTP/2, toutes les requêtes passent par une seule connexion :
            // une connexion morte (NAT, coupure) est détectée par un ping
            // sans réponse plutôt qu'au délai de chaque requête.
            .http2_keep_alive_interval(HTTP2_PING_INTERVAL)
            .http2_keep_alive_timeout(HTTP2_PING_TIMEOUT);
    }
    proxy.apply(builder)?.build().context("building the HTTP client")
}

/// Intervalle des pings HTTP/2 sur le canal chiffré.
pub(super) const HTTP2_PING_INTERVAL: Duration = Duration::from_secs(30);

/// Délai de réponse à un ping HTTP/2 : au-delà, la connexion est abandonnée.
pub(super) const HTTP2_PING_TIMEOUT: Duration = Duration::from_secs(20);

/// Client des mises à niveau du relais : HTTP/1.1 seul (une mise à niveau
/// n'existe pas en HTTP/2), sans connexion gardée en réserve, et des sondes
/// TCP pour qu'une connexion multiplexée muette (NAT) finisse par tomber.
pub(super) fn upgrade_client(
    timeout: Duration,
    tls: rustls::ClientConfig,
    proxy: &ProxyConfig,
) -> Result<reqwest::Client> {
    let builder = reqwest::Client::builder()
        .timeout(timeout)
        .http1_only()
        .pool_max_idle_per_host(0)
        .tcp_keepalive(Duration::from_secs(60))
        .user_agent(concat!("dumbmonit-agent/", env!("CARGO_PKG_VERSION")))
        .tls_backend_preconfigured(tls)
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none());
    proxy.apply(builder)?.build().context("building the HTTP client")
}

/// Construit l'URL d'ingestion à partir de la racine du serveur.
pub(super) fn ingest_url(server_url: &str) -> String {
    format!("{}{INGEST_PATH}", server_url.trim_end_matches('/'))
}

/// Chemin de la file de commandes, ou du compte rendu d'une commande donnée.
///
/// La clé d'identité accompagne chaque requête (en-tête [`AGENT_KEY_HEADER`]) :
/// le jeton est partagé par tout un parc et ne dit pas de quelle machine il
/// s'agit.
pub(super) fn commands_path(id: Option<i64>) -> String {
    match id {
        Some(id) => format!("{COMMANDS_PATH}/{id}"),
        None => COMMANDS_PATH.to_string(),
    }
}

/// Décrit une erreur de transport : la nature (délai, connexion…), puis la
/// chaîne des causes en commençant par la plus profonde — « Connection refused »
/// avant « error sending request » —, puis l'URL **sans ses paramètres**.
///
/// Le message brut de `reqwest` commence par l'URL complète : tronqué par
/// `systemctl status`, il ne montrait jamais la cause, et recopiait la clé de
/// la machine quand elle voyageait encore dans l'URL.
pub(crate) fn describe(error: &reqwest::Error) -> String {
    use std::error::Error as _;
    let nature = if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "cannot connect"
    } else if error.is_decode() {
        "unreadable response"
    } else if error.is_body() {
        "response interrupted"
    } else {
        "request failed"
    };
    let mut causes: Vec<String> = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !text.is_empty() && !causes.contains(&text) {
            causes.push(text);
        }
        source = cause.source();
    }
    causes.reverse();
    let mut message = nature.to_string();
    for cause in causes.iter().take(3) {
        message.push_str(": ");
        message.push_str(cause);
    }
    if let Some(url) = error.url() {
        let mut url = url.clone();
        url.set_query(None);
        url.set_fragment(None);
        let _ = url.set_password(None);
        let _ = url.set_username("");
        message.push_str(&format!(" ({url})"));
    }
    message
}

/// Encodage d'un paramètre de requête. Sans le `query` de reqwest — une
/// dépendance de plus pour un seul paramètre — les caractères non réservés
/// passent tels quels, tout le reste est échappé.
pub(super) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Message du corps d'erreur `{"error": "..."}` renvoyé par l'API, si c'en est un.
///
/// Sa présence est ce qui distingue un refus du serveur DumbMonit d'une page
/// d'erreur produite par un intermédiaire.
pub(super) fn server_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string))
}

/// Message d'erreur à journaliser : celui du serveur, ou le début du corps brut.
pub(super) fn extract_error_message(body: &str) -> String {
    server_error_message(body).unwrap_or_else(|| body.trim().chars().take(200).collect())
}

/// Retire le jeton d'un message avant qu'il n'atteigne les journaux.
///
/// Les messages de `reqwest` contiennent l'URL, et une erreur de configuration
/// pourrait un jour y glisser le jeton. Le filet est peu coûteux, et l'oubli
/// inverse ne se rattrape pas une fois les journaux partis en centralisation.
pub(super) fn sanitise(message: &str, token: &str) -> String {
    if token.is_empty() {
        return message.to_string();
    }
    message.replace(token, "<redacted>")
}
