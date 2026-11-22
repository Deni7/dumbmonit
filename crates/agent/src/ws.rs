//! Transport WebSocket du canal chiffré : l'agent passe par l'adresse de
//! l'interface quand le port des agents est injoignable.
//!
//! Derrière Cloudflare Tunnel, un mandataire inverse qui termine TLS ou un
//! mandataire d'entreprise qui ne laisse sortir que HTTP(S), seule l'adresse de
//! l'interface (`server_url`) répond. L'agent y ouvre une WebSocket
//! ([`AGENT_WS_PATH`]) et fait passer *dedans* sa session TLS habituelle : TLS
//! 1.3, autorité épinglée, certificat client, échange post-quantique.
//! L'intermédiaire ne voit que des messages binaires opaques ; le serveur
//! traite le flux comme une connexion à son port des agents.
//!
//! Mécanique : un **pont local** écoute sur la boucle locale (`127.0.0.1`, port
//! choisi par le système). Le client HTTP du canal s'y connecte comme au port
//! des agents ; chaque connexion acceptée devient une WebSocket vers le
//! serveur, recopiée octet pour octet. Tout le reste — HTTP/2, enrôlement,
//! renouvellement, connexion multiplexée d'un relais — passe sans changer.
//! La mise à niveau HTTP part par le même client que l'interface : autorités
//! du système (ou clé épinglée `server_pin`) et mandataire sortant.
//!
//! Le pont n'est pas une porte : il ne mène qu'au serveur de l'agent, et ce
//! qu'il transporte est une session TLS que seul ce serveur termine. Un autre
//! programme de la machine qui s'y connecterait n'obtiendrait rien de plus
//! qu'en ouvrant lui-même l'adresse de l'interface.
//!
//! Choix (`transport:` de agent.yaml, `DUMBMONIT_AGENT_TRANSPORT`) :
//! `direct` (le port des agents, toujours), `websocket` (l'adresse de
//! l'interface, toujours), `auto` (défaut) : le port des agents d'abord ; après
//! des échecs de connexion répétés, la WebSocket est essayée, et adoptée si
//! elle répond — puis retenue (`agent-tls.json`) jusqu'à ce qu'elle échoue à
//! son tour.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::{Context, Result};
use dumbmonit_proto::{AGENT_WS_CHUNK, AGENT_WS_MAX_MESSAGE, AGENT_WS_PATH, AGENT_WS_PROTOCOL};
use futures_util::{SinkExt, StreamExt};
use reqwest::header;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, MissedTickBehavior};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::client::generate_key;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Bytes, Error as WsError, Message};
use tracing::{debug, info, warn};

use crate::proxy::ProxyConfig;

/// Intervalle des pings : sous les délais d'inactivité habituels des
/// mandataires (60 s pour nginx, 100 s pour Cloudflare).
const PING_INTERVAL: Duration = Duration::from_secs(20);

/// Silence du serveur au-delà duquel la WebSocket est tenue pour morte : il
/// répond à chaque ping.
const IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Délai de connexion vers l'adresse de l'interface (ou le mandataire).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Délai de la mise à niveau elle-même.
const UPGRADE_TIMEOUT: Duration = Duration::from_secs(30);

/// Connexions simultanées au plus à travers le pont : l'agent n'en garde
/// qu'une (HTTP/2), plus celle de la connexion multiplexée d'un relais.
const MAX_CONNECTIONS: usize = 64;

/// Ce que la configuration demande (`transport:`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransportMode {
    /// Le port des agents d'abord, la WebSocket s'il est injoignable.
    #[default]
    Auto,
    /// Le port des agents, toujours.
    Direct,
    /// L'adresse de l'interface, toujours.
    WebSocket,
}

impl TransportMode {
    /// Lit `auto`, `direct` ou `websocket` (`ws` accepté) ; `None` sinon.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "direct" => Some(Self::Direct),
            "websocket" | "ws" | "wss" => Some(Self::WebSocket),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Direct => "direct",
            Self::WebSocket => "websocket",
        }
    }
}

/// Chemin effectivement emprunté par le canal chiffré, retenu dans
/// `agent-tls.json` en mode `auto`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    /// Le port des agents (en direct, ou par `CONNECT` à travers le mandataire).
    #[default]
    Direct,
    /// La WebSocket de l'interface.
    WebSocket,
}

impl Route {
    pub fn other(self) -> Self {
        match self {
            Self::Direct => Self::WebSocket,
            Self::WebSocket => Self::Direct,
        }
    }
}

/// Adresse de la WebSocket des agents pour une URL de serveur : la même
/// racine (chemin de préfixe compris), le chemin des agents. `None` pour ce
/// qui n'est pas une URL HTTP(S).
pub fn websocket_url(server_url: &str) -> Option<String> {
    let base = server_url.trim().trim_end_matches('/');
    let rest = base.strip_prefix("https://").or_else(|| base.strip_prefix("http://"))?;
    if rest.is_empty() || rest.starts_with('/') {
        return None;
    }
    Some(format!("{base}{AGENT_WS_PATH}"))
}

/// Forme affichée d'une adresse de WebSocket : `wss://…` (`ws://…` en clair).
pub fn display(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = url.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        url.to_string()
    }
}

/// Réglages WebSocket : ceux du serveur (messages bornés, écriture immédiate).
fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(32 * 1024)
        .write_buffer_size(0)
        .max_message_size(Some(AGENT_WS_MAX_MESSAGE))
        .max_frame_size(Some(AGENT_WS_MAX_MESSAGE))
}

/// Ouvre les WebSockets vers l'adresse de l'interface.
pub struct Connector {
    /// Forme HTTP de l'adresse (`https://hôte[:port][/préfixe]/api/agent/ws`).
    url: String,
    /// Client de la mise à niveau : HTTP/1.1, TLS de l'interface, mandataire.
    http: reqwest::Client,
    /// Dernière erreur d'ouverture : une panne du canal dit ainsi sa vraie
    /// cause, au lieu d'une poignée de main TLS interrompue.
    last_error: Mutex<Option<String>>,
}

impl Connector {
    /// `server_pin` : la clé épinglée du certificat autosigné de l'interface,
    /// si la commande d'installation en a donné une ; sinon, les autorités du
    /// système (celles d'un mandataire d'entreprise qui inspecte TLS
    /// comprises — la session de l'agent, à l'intérieur, reste épinglée).
    pub fn new(url: String, server_pin: Option<&str>, proxy: &ProxyConfig) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .http1_only()
            .pool_max_idle_per_host(0)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(UPGRADE_TIMEOUT)
            .tcp_keepalive(Duration::from_secs(60))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(concat!("dumbmonit-agent/", env!("CARGO_PKG_VERSION")));
        if let Some(pin) = server_pin {
            builder = builder.tls_backend_preconfigured(crate::tls::pinned_key_config(pin)?);
        }
        let http = proxy.apply(builder)?.build().context("building the WebSocket client")?;
        Ok(Self { url, http, last_error: Mutex::new(None) })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Le même client (TLS de l'interface, mandataire) vers une autre adresse :
    /// celle d'un serveur de secours (`server_urls`).
    pub fn with_url(&self, url: String) -> Self {
        Self { url, http: self.http.clone(), last_error: Mutex::new(None) }
    }

    /// Ouvre une WebSocket à cette adresse puis la referme, sans rien noter :
    /// vrai si DumbMonit y répond (mise à niveau acceptée sous le bon
    /// sous-protocole). L'essai de la primaire, depuis un secours.
    pub async fn answers(&self) -> Result<(), String> {
        let mut ws = self.try_open().await?;
        let _ = ws.close(None).await;
        Ok(())
    }

    /// Dernière erreur d'ouverture, s'il y en a eu une depuis le dernier succès.
    pub fn last_error(&self) -> Option<String> {
        self.last_error.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    /// Ouvre une WebSocket et note l'issue : une erreur est journalisée quand
    /// elle change, pas à chaque essai.
    pub async fn open(&self) -> Result<WebSocketStream<reqwest::Upgraded>, String> {
        let opened = self.try_open().await;
        let mut last = self.last_error.lock().unwrap_or_else(PoisonError::into_inner);
        match &opened {
            Ok(_) => {
                if last.take().is_some() {
                    info!(url = %display(&self.url), "agent websocket open again");
                }
            }
            Err(error) => {
                if last.as_deref() != Some(error.as_str()) {
                    warn!(url = %display(&self.url), %error, "cannot open the agent websocket");
                } else {
                    debug!(url = %display(&self.url), %error, "cannot open the agent websocket");
                }
                *last = Some(error.clone());
            }
        }
        opened
    }

    async fn try_open(&self) -> Result<WebSocketStream<reqwest::Upgraded>, String> {
        let key = generate_key();
        let response = self
            .http
            .get(&self.url)
            .header(header::CONNECTION, "Upgrade")
            .header(header::UPGRADE, "websocket")
            .header(header::SEC_WEBSOCKET_VERSION, "13")
            .header(header::SEC_WEBSOCKET_KEY, key.as_str())
            .header(header::SEC_WEBSOCKET_PROTOCOL, AGENT_WS_PROTOCOL)
            .send()
            .await
            .map_err(|error| crate::client::describe(&error))?;
        let status = response.status();
        if status != reqwest::StatusCode::SWITCHING_PROTOCOLS {
            let body = response.text().await.unwrap_or_default();
            return Err(refusal(status, &body));
        }
        let accept = response
            .headers()
            .get(header::SEC_WEBSOCKET_ACCEPT)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if accept != derive_accept_key(key.as_bytes()) {
            return Err("the WebSocket upgrade was not answered by DumbMonit \
                        (wrong Sec-WebSocket-Accept)"
                .to_string());
        }
        let protocol = response
            .headers()
            .get(header::SEC_WEBSOCKET_PROTOCOL)
            .and_then(|value| value.to_str().ok())
            .map(str::trim);
        if protocol != Some(AGENT_WS_PROTOCOL) {
            return Err("a WebSocket answers at this address, but not the DumbMonit agent \
                        channel"
                .to_string());
        }
        let upgraded = response.upgrade().await.map_err(|error| crate::client::describe(&error))?;
        Ok(WebSocketStream::from_raw_socket(upgraded, Role::Client, Some(config())).await)
    }
}

/// Ce que dit un refus de la mise à niveau, et quoi y faire.
fn refusal(status: reqwest::StatusCode, body: &str) -> String {
    let from_server = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get("error")?.as_str().map(str::to_string));
    let hint = match status.as_u16() {
        404 | 405 => {
            "the server predates the agent WebSocket, or the proxy does not forward \
                      /api/agent/ws"
        }
        400 | 426 => {
            "the proxy did not pass the WebSocket upgrade through \
                      (Upgrade and Connection headers)"
        }
        429 => "too many agent connections from this address",
        502..=504 => "the proxy or tunnel cannot reach DumbMonit",
        _ => "no WebSocket at this address",
    };
    match from_server {
        Some(message) => format!("HTTP {status} at the agent WebSocket: {message}"),
        None => format!("HTTP {status} at the agent WebSocket: {hint}"),
    }
}

/// Pont local : écoute sur la boucle locale ; chaque connexion du client HTTP
/// de l'agent devient une WebSocket vers le serveur. Arrêté avec lui.
pub struct Bridge {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl Bridge {
    /// Ouvre le pont (port choisi par le système). À appeler depuis le runtime.
    pub fn start(connector: Arc<Connector>) -> Result<Self> {
        tokio::runtime::Handle::try_current()
            .context("the WebSocket transport needs the async runtime")?;
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .context("opening the local WebSocket bridge")?;
        listener.set_nonblocking(true).context("opening the local WebSocket bridge")?;
        let listener = TcpListener::from_std(listener).context("opening the local bridge")?;
        let addr = listener.local_addr().context("opening the local bridge")?;
        debug!(%addr, url = %display(connector.url()), "local WebSocket bridge open");
        let task = tokio::spawn(serve(listener, connector));
        Ok(Self { addr, task })
    }

    /// Racine du canal chiffré à travers le pont.
    pub fn base_url(&self) -> String {
        format!("https://{}", self.addr)
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        // Les connexions en cours tombent avec la tâche (`JoinSet`).
        self.task.abort();
    }
}

async fn serve(listener: TcpListener, connector: Arc<Connector>) {
    let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut connections = JoinSet::new();
    loop {
        let accepted = tokio::select! {
            // Ramasse les connexions terminées, pour que l'ensemble ne grossisse pas.
            Some(_) = connections.join_next(), if !connections.is_empty() => continue,
            accepted = listener.accept() => accepted,
        };
        let (socket, _) = match accepted {
            Ok(accepted) => accepted,
            Err(error) => {
                debug!(%error, "local WebSocket bridge: accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let Ok(slot) = slots.clone().try_acquire_owned() else {
            warn!("local WebSocket bridge: connection limit reached, connection refused");
            continue;
        };
        let connector = connector.clone();
        connections.spawn(async move {
            let _slot = slot;
            let _ = socket.set_nodelay(true);
            // L'échec est journalisé par `open` ; la connexion locale se ferme,
            // et le client HTTP le voit comme un canal injoignable.
            if let Ok(ws) = connector.open().await
                && let Err(error) = splice(socket, ws).await
            {
                debug!(%error, "agent websocket closed");
            }
        });
    }
}

/// Recopie la connexion locale dans la WebSocket et inversement, avec un ping
/// toutes les 20 s, jusqu'à la fin des deux côtés ou un silence trop long.
pub(crate) async fn splice<L, S>(local: L, mut ws: WebSocketStream<S>) -> io::Result<()>
where
    L: AsyncRead + AsyncWrite + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut from_agent, mut to_agent) = tokio::io::split(local);
    let mut buffer = vec![0u8; AGENT_WS_CHUNK];
    let mut ping = tokio::time::interval_at(Instant::now() + PING_INTERVAL, PING_INTERVAL);
    ping.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut heard = Instant::now();
    let mut agent_open = true;
    loop {
        tokio::select! {
            read = from_agent.read(&mut buffer), if agent_open => match read? {
                0 => {
                    // Le client local a tout envoyé : fin du flux côté serveur,
                    // dont on attend encore la réponse.
                    agent_open = false;
                    ws.close(None).await.map_err(into_io)?;
                }
                n => {
                    let message = Message::Binary(Bytes::copy_from_slice(&buffer[..n]));
                    ws.send(message).await.map_err(into_io)?;
                }
            },
            message = ws.next() => match message {
                Some(Ok(Message::Binary(data))) => {
                    heard = Instant::now();
                    to_agent.write_all(&data).await?;
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Frame(_))) => {
                    heard = Instant::now();
                }
                Some(Ok(Message::Text(_))) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "text message on the agent websocket",
                    ));
                }
                Some(Ok(Message::Close(_))) | None => {
                    let _ = to_agent.shutdown().await;
                    return Ok(());
                }
                Some(Err(error)) => return Err(into_io(error)),
            },
            _ = ping.tick() => {
                if heard.elapsed() > IDLE_TIMEOUT {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the agent websocket went silent",
                    ));
                }
                ws.send(Message::Ping(Bytes::new())).await.map_err(into_io)?;
            }
        }
    }
}

fn into_io(error: WsError) -> io::Error {
    match error {
        WsError::Io(error) => error,
        WsError::ConnectionClosed | WsError::AlreadyClosed => {
            io::Error::from(io::ErrorKind::BrokenPipe)
        }
        other => io::Error::other(other.to_string()),
    }
}

/// Choix du transport, partagé par toutes les copies du client.
#[derive(Default)]
pub struct Transport {
    state: Mutex<State>,
    /// Échecs de connexion d'affilée depuis la dernière réponse du serveur.
    unreachable: AtomicU32,
}

#[derive(Default)]
struct State {
    mode: TransportMode,
    route: Route,
    /// `None` : pas d'adresse de WebSocket utilisable (`server_url` en clair).
    connector: Option<Arc<Connector>>,
    /// Ouvert au premier besoin, puis gardé.
    bridge: Option<Arc<Bridge>>,
}

impl Transport {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Pose le mode et l'adresse de la WebSocket. `websocket` sans adresse
    /// utilisable revient au port des agents.
    pub fn configure(&self, mode: TransportMode, connector: Option<Connector>) {
        let mut state = self.lock();
        state.route = match (mode, &connector) {
            (TransportMode::WebSocket, Some(_)) => Route::WebSocket,
            _ => Route::Direct,
        };
        state.mode = mode;
        state.connector = connector.map(Arc::new);
        state.bridge = None;
    }

    /// Autre serveur (`server_urls`) : la WebSocket vise désormais son
    /// adresse. Le pont est refermé, ses connexions avec lui — aucune ne reste
    /// ouverte vers l'ancien serveur — et rouvert au premier besoin.
    /// `None` : pas d'adresse utilisable, le port des agents.
    pub fn retarget(&self, ws_url: Option<String>) {
        let mut state = self.lock();
        let connector = match (&state.connector, ws_url) {
            (Some(current), Some(url)) => Some(Arc::new(current.with_url(url))),
            _ => None,
        };
        if connector.is_none() {
            state.route = Route::Direct;
        }
        state.connector = connector;
        state.bridge = None;
    }

    /// Ouvreur de la WebSocket vers cette adresse, avec les réglages en cours
    /// (`None` sans WebSocket configurée).
    pub fn connector_for(&self, ws_url: String) -> Option<Connector> {
        self.lock().connector.as_ref().map(|current| current.with_url(ws_url))
    }

    /// Adresse affichée de la WebSocket quand c'est elle qui sert.
    pub fn websocket_address(&self) -> Option<String> {
        let state = self.lock();
        let connector = state.connector.as_ref().filter(|_| state.route == Route::WebSocket)?;
        Some(display(connector.url()))
    }

    /// Reprend le chemin retenu la dernière fois, en mode `auto` seulement.
    pub fn remember(&self, route: Route) {
        let mut state = self.lock();
        let usable = route == Route::Direct || state.connector.is_some();
        if state.mode == TransportMode::Auto && usable {
            state.route = route;
        }
    }

    pub fn route(&self) -> Route {
        self.lock().route
    }

    pub(crate) fn set_route(&self, route: Route) {
        self.lock().route = route;
    }

    /// Vrai si le mode permet de changer de chemin, et qu'il y en a deux.
    pub fn can_switch(&self) -> bool {
        let state = self.lock();
        state.mode == TransportMode::Auto && state.connector.is_some()
    }

    /// Le pont local, ouvert au premier besoin.
    pub fn bridge(&self) -> Result<Arc<Bridge>> {
        let mut state = self.lock();
        if let Some(bridge) = &state.bridge {
            return Ok(bridge.clone());
        }
        let connector = state
            .connector
            .clone()
            .context("no agent WebSocket address: the server URL is not https://")?;
        let bridge = Arc::new(Bridge::start(connector)?);
        state.bridge = Some(bridge.clone());
        Ok(bridge)
    }

    /// Un échec de connexion de plus.
    pub fn note_unreachable(&self) {
        self.unreachable.fetch_add(1, Ordering::Relaxed);
    }

    /// Le serveur a répondu : le compte repart de zéro.
    pub fn note_reachable(&self) {
        self.unreachable.store(0, Ordering::Relaxed);
    }

    pub fn unreachable(&self) -> u32 {
        self.unreachable.load(Ordering::Relaxed)
    }

    /// Cause d'une panne de la WebSocket, quand c'est elle qui sert.
    pub fn websocket_failure(&self) -> Option<String> {
        let state = self.lock();
        let connector = state.connector.as_ref().filter(|_| state.route == Route::WebSocket)?;
        let cause = connector.last_error()?;
        Some(format!("agent websocket {}: {cause}", display(connector.url())))
    }

    /// Ligne de journal : `direct`, ou `websocket wss://…`.
    pub fn describe(&self) -> String {
        let state = self.lock();
        match (state.route, &state.connector) {
            (Route::WebSocket, Some(connector)) => {
                format!("websocket {}", display(connector.url()))
            }
            _ => format!("direct (transport: {})", state.mode.as_str()),
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio::net::TcpStream;

    use super::*;
    use crate::testsupport::answering;

    #[test]
    fn the_transport_is_read_from_a_plain_word() {
        assert_eq!(TransportMode::parse("auto"), Some(TransportMode::Auto));
        assert_eq!(TransportMode::parse(" Direct "), Some(TransportMode::Direct));
        assert_eq!(TransportMode::parse("websocket"), Some(TransportMode::WebSocket));
        assert_eq!(TransportMode::parse("wss"), Some(TransportMode::WebSocket));
        assert_eq!(TransportMode::parse("tcp"), None);
        assert_eq!(TransportMode::default(), TransportMode::Auto);
        assert_eq!(Route::Direct.other(), Route::WebSocket);
        // Le chemin retenu s'écrit en clair dans agent-tls.json.
        assert_eq!(serde_json::to_string(&Route::WebSocket).unwrap(), r#""websocket""#);
        assert_eq!(serde_json::from_str::<Route>(r#""direct""#).unwrap(), Route::Direct);
    }

    #[test]
    fn the_websocket_sits_under_the_interface_address_prefix_included() {
        assert_eq!(
            websocket_url("https://monit.example.com/").as_deref(),
            Some("https://monit.example.com/api/agent/ws")
        );
        assert_eq!(
            websocket_url("https://example.com/dumbmonit").as_deref(),
            Some("https://example.com/dumbmonit/api/agent/ws")
        );
        assert_eq!(
            websocket_url("http://127.0.0.1:8080").as_deref(),
            Some("http://127.0.0.1:8080/api/agent/ws")
        );
        assert_eq!(websocket_url("ftp://x"), None);
        assert_eq!(websocket_url("https://"), None);
        assert_eq!(
            display("https://monit.example.com/api/agent/ws"),
            "wss://monit.example.com/api/agent/ws"
        );
        assert_eq!(display("http://127.0.0.1:1/api/agent/ws"), "ws://127.0.0.1:1/api/agent/ws");
    }

    #[test]
    fn a_refused_upgrade_says_what_to_fix() {
        let not_found = refusal(reqwest::StatusCode::NOT_FOUND, "<html>nginx</html>");
        assert!(not_found.contains("/api/agent/ws"), "{not_found}");
        let stripped = refusal(reqwest::StatusCode::UPGRADE_REQUIRED, "");
        assert!(stripped.contains("Upgrade and Connection"), "{stripped}");
        // Le message du serveur, quand c'est lui qui refuse.
        let server = refusal(reqwest::StatusCode::FORBIDDEN, r#"{"error":"Browsers cannot."}"#);
        assert!(server.ends_with("Browsers cannot."), "{server}");
    }

    #[test]
    fn only_auto_remembers_and_switches_and_only_with_a_websocket_address() {
        let proxy = ProxyConfig::default();
        let connector = || Connector::new("https://s/api/agent/ws".into(), None, &proxy).unwrap();

        let transport = Transport::default();
        transport.configure(TransportMode::Auto, Some(connector()));
        assert_eq!(transport.route(), Route::Direct, "le port des agents d'abord");
        assert!(transport.can_switch());
        transport.remember(Route::WebSocket);
        assert_eq!(transport.route(), Route::WebSocket, "le chemin retenu");

        transport.configure(TransportMode::Direct, Some(connector()));
        transport.remember(Route::WebSocket);
        assert_eq!(transport.route(), Route::Direct, "imposé : rien ne bouge");
        assert!(!transport.can_switch());

        transport.configure(TransportMode::WebSocket, Some(connector()));
        assert_eq!(transport.route(), Route::WebSocket);
        assert!(transport.describe().starts_with("websocket wss://s/"), "{}", transport.describe());

        // Pas d'adresse utilisable : le port des agents, quoi qu'on demande.
        transport.configure(TransportMode::WebSocket, None);
        assert_eq!(transport.route(), Route::Direct);
        transport.configure(TransportMode::Auto, None);
        transport.remember(Route::WebSocket);
        assert_eq!(transport.route(), Route::Direct);
        assert!(!transport.can_switch());

        transport.note_unreachable();
        transport.note_unreachable();
        assert_eq!(transport.unreachable(), 2);
        transport.note_reachable();
        assert_eq!(transport.unreachable(), 0);
    }

    #[test]
    fn another_server_moves_the_websocket_and_drops_it_without_an_address() {
        let proxy = ProxyConfig::default();
        let connector = Connector::new("https://a/api/agent/ws".into(), None, &proxy).unwrap();
        let transport = Transport::default();
        transport.configure(TransportMode::WebSocket, Some(connector));
        assert_eq!(transport.websocket_address().as_deref(), Some("wss://a/api/agent/ws"));

        // Un secours de `server_urls` : la WebSocket suit son adresse.
        transport.retarget(Some("https://b/api/agent/ws".into()));
        assert_eq!(transport.route(), Route::WebSocket);
        assert_eq!(transport.websocket_address().as_deref(), Some("wss://b/api/agent/ws"));
        let probe = transport.connector_for("https://c/api/agent/ws".into()).expect("ouvreur");
        assert_eq!(probe.url(), "https://c/api/agent/ws");

        // Sans adresse utilisable : le port des agents, plus de WebSocket.
        transport.retarget(None);
        assert_eq!(transport.route(), Route::Direct);
        assert_eq!(transport.websocket_address(), None);
        assert!(transport.connector_for("https://c/api/agent/ws".into()).is_none());
    }

    /// Serveur WebSocket factice : renvoie en écho chaque message binaire.
    async fn echo_server(protocol: &'static str) -> SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let accepted = tokio_tungstenite::accept_hdr_async(socket, answering(protocol));
                    let Ok(mut ws) = accepted.await else { return };
                    while let Some(Ok(message)) = ws.next().await {
                        if message.is_binary() && ws.send(message).await.is_err() {
                            break;
                        }
                    }
                });
            }
        });
        addr
    }

    #[tokio::test]
    async fn the_bridge_carries_bytes_through_a_websocket_both_ways() {
        let server = echo_server(AGENT_WS_PROTOCOL).await;
        let url = format!("http://{server}{AGENT_WS_PATH}");
        let connector = Arc::new(Connector::new(url, None, &ProxyConfig::default()).unwrap());
        let bridge = Bridge::start(connector.clone()).unwrap();
        let local = bridge.base_url().replace("https://", "");
        let mut socket = TcpStream::connect(local).await.unwrap();
        // Plus qu'un message : découpé à l'aller, recousu au retour.
        let sent: Vec<u8> = (0..150_000u32).map(|i| (i % 253) as u8).collect();
        socket.write_all(&sent).await.unwrap();
        let mut back = vec![0u8; sent.len()];
        tokio::time::timeout(Duration::from_secs(10), socket.read_exact(&mut back))
            .await
            .expect("délai")
            .expect("écho");
        assert_eq!(back, sent);
        assert_eq!(connector.last_error(), None);
    }

    #[tokio::test]
    async fn an_address_without_the_agent_websocket_closes_the_local_connection_and_says_why() {
        // Rien n'écoute : la connexion locale est fermée, la cause gardée.
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let url = format!("http://{closed}{AGENT_WS_PATH}");
        let connector = Arc::new(Connector::new(url, None, &ProxyConfig::default()).unwrap());
        let bridge = Bridge::start(connector.clone()).unwrap();
        let local = bridge.base_url().replace("https://", "");
        let mut socket = TcpStream::connect(local).await.unwrap();
        let _ = socket.write_all(&[0x16, 0x03, 0x01]).await;
        let mut buffer = [0u8; 8];
        let read = tokio::time::timeout(Duration::from_secs(15), socket.read(&mut buffer))
            .await
            .expect("fermée, pas pendante");
        assert!(matches!(read, Ok(0) | Err(_)), "{read:?}");
        let error = connector.last_error().expect("cause gardée");
        assert!(error.contains("cannot connect"), "{error}");

        // Une WebSocket quelconque, d'un autre protocole : refusée, et dit.
        let foreign = echo_server("chat").await;
        let url = format!("http://{foreign}{AGENT_WS_PATH}");
        let connector = Connector::new(url, None, &ProxyConfig::default()).unwrap();
        let error = connector.open().await.err().expect("refusée");
        assert!(error.contains("not the DumbMonit agent channel"), "{error}");
    }
}
