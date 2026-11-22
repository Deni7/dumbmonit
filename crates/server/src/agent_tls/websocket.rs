//! Canal des agents par WebSocket, à l'adresse de l'interface.
//!
//! Derrière Cloudflare Tunnel, un mandataire inverse qui termine TLS ou un
//! mandataire d'entreprise qui ne laisse sortir que HTTP(S), le port des
//! agents est injoignable : seule l'adresse de l'interface l'est. Un agent y
//! ouvre alors une WebSocket ([`AGENT_WS_PATH`]) et fait passer *dedans* sa
//! session TLS habituelle : la même poignée de main TLS 1.3 sous l'autorité
//! interne épinglée, avec son certificat client. Le flux est remis à la file
//! du port des agents ([`super::listener`]) et servi par les mêmes routes,
//! avec les mêmes contrôles ; la connexion multiplexée d'un relais y passe
//! comme le reste.
//!
//! L'intermédiaire ne voit qu'une requête `GET` et des messages binaires
//! opaques. Le TLS extérieur (celui de Cloudflare, du mandataire ou de
//! l'interface) ne prouve rien de l'agent : son identité reste celle de son
//! certificat interne, vérifiée de bout en bout. Aucun en-tête n'entre en
//! compte pour l'identité ; `X-Forwarded-For` ne sert qu'aux compteurs, et
//! seulement venu d'un mandataire déclaré (`DUMBMONIT_TRUSTED_PROXIES`, voir
//! [`crate::auth::client_ip`]).
//!
//! **Pas un contournement** :
//!
//! - sans poignée de main TLS menée à son terme sous l'autorité des agents
//!   (10 s au plus, poignées simultanées bornées), rien n'atteint une route ;
//! - sans certificat client, seul l'enrôlement répond (jeton, tentatives
//!   comptées par adresse) — ni fichiers d'installation ni rien d'autre — et
//!   la session est coupée au bout d'une minute ;
//! - la route n'ouvre aucune session, ne lit aucun cookie, et refuse toute
//!   requête qui porte une origine (`Origin`, qu'un navigateur envoie
//!   toujours) ou le cookie de session : une page web ne peut pas s'en servir ;
//! - bornes : connexions par adresse (`DUMBMONIT_AGENT_WS_MAX_PER_IP`), taille
//!   des messages ([`AGENT_WS_MAX_MESSAGE`]), débit entrant de chaque connexion
//!   (`DUMBMONIT_AGENT_WS_RATE_KIB`), deux minutes de silence au plus (l'agent
//!   envoie un ping toutes les 20 s).

use std::collections::HashMap;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, ready};
use std::time::Duration;

use axum::Json;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Version, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use dumbmonit_proto::{AGENT_WS_CHUNK, AGENT_WS_MAX_MESSAGE, AGENT_WS_PATH, AGENT_WS_PROTOCOL};
use futures::{Sink, Stream};
use serde_json::json;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Semaphore;
use tokio::time::{Instant, Sleep};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::{Role, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Bytes, Error as WsError, Message};

use super::listener::{self, Transport};
use crate::auth::client_ip::ClientIp;
use crate::state::AppState;

/// Connexions simultanées par adresse, par défaut
/// (`DUMBMONIT_AGENT_WS_MAX_PER_IP`) : un bureau derrière un seul NAT, ou un
/// mandataire non déclaré, compte pour une adresse.
pub const DEFAULT_MAX_PER_IP: usize = 256;

/// Poignées de main TLS simultanées à travers une WebSocket, au plus : comme
/// le port des agents.
const MAX_HANDSHAKES: usize = 256;

/// Durée de vie d'une session sans certificat client : de quoi s'enrôler,
/// pas de quoi garder une connexion ouverte.
pub const UNAUTHENTICATED_LIFETIME: Duration = Duration::from_secs(60);

/// Silence au-delà duquel une connexion est coupée. L'agent envoie un ping
/// toutes les 20 s : seule une connexion morte (NAT, coupure) se tait aussi
/// longtemps.
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Bornes partagées des connexions WebSocket d'agents.
pub struct Limits {
    /// Connexions ouvertes, par adresse.
    open: Arc<Mutex<HashMap<IpAddr, usize>>>,
    /// Poignées de main TLS en cours.
    handshakes: Arc<Semaphore>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            open: Arc::new(Mutex::new(HashMap::new())),
            handshakes: Arc::new(Semaphore::new(MAX_HANDSHAKES)),
        }
    }
}

impl Limits {
    /// Réserve une place pour cette adresse, ou `None` au plafond. Elle se
    /// libère avec la connexion.
    pub fn reserve(&self, ip: IpAddr, max: usize) -> Option<IpSlot> {
        let mut counts = self.open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = counts.entry(ip).or_insert(0);
        if *count >= max {
            return None;
        }
        *count += 1;
        Some(IpSlot { open: self.open.clone(), ip })
    }

    /// Connexions ouvertes en ce moment depuis cette adresse.
    pub fn open_from(&self, ip: IpAddr) -> usize {
        let counts = self.open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        counts.get(&ip).copied().unwrap_or(0)
    }
}

/// Place réservée pour une adresse ; rendue quand la connexion se ferme.
pub struct IpSlot {
    open: Arc<Mutex<HashMap<IpAddr, usize>>>,
    ip: IpAddr,
}

impl Drop for IpSlot {
    fn drop(&mut self) {
        let mut counts = self.open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = counts.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&self.ip);
            }
        }
    }
}

/// Réglages WebSocket du canal, des deux côtés : messages bornés, écriture
/// immédiate (la session TLS au-dessus fait ses propres tampons), tampon de
/// lecture modeste — un serveur en tient un par agent.
pub fn config() -> WebSocketConfig {
    WebSocketConfig::default()
        .read_buffer_size(32 * 1024)
        .write_buffer_size(0)
        .max_message_size(Some(AGENT_WS_MAX_MESSAGE))
        .max_frame_size(Some(AGENT_WS_MAX_MESSAGE))
}

/// Une WebSocket vue comme un flux d'octets : chaque message binaire porte un
/// morceau de la session TLS de l'agent, dans l'ordre.
///
/// Les deux côtés s'en servent : le serveur sous sa poignée de main TLS, les
/// tests (et la vérification du canal) comme le ferait un agent.
pub struct WsIo<S> {
    ws: WebSocketStream<S>,
    /// Octets reçus, pas encore lus.
    pending: Bytes,
    /// Fin du flux : message `Close` reçu, ou flux terminé.
    ended: bool,
    /// Silence toléré, et son échéance en cours.
    idle: Option<(Duration, Pin<Box<Sleep>>)>,
    /// Débit entrant au plus.
    pace: Option<Pace>,
    /// Coupure d'une session restée sans certificat client : l'échéance, et le
    /// drapeau levé une fois le certificat présenté.
    deadline: Option<(Pin<Box<Sleep>>, Arc<AtomicBool>)>,
    /// Place de l'adresse, rendue avec la connexion.
    _slot: Option<IpSlot>,
}

/// Débit entrant, par fenêtre d'une seconde.
struct Pace {
    bytes_per_sec: u64,
    window: Instant,
    received: u64,
    /// Attente en cours avant de lire le message suivant.
    wait: Option<Pin<Box<Sleep>>>,
}

impl<S> WsIo<S> {
    pub fn new(ws: WebSocketStream<S>) -> Self {
        Self {
            ws,
            pending: Bytes::new(),
            ended: false,
            idle: None,
            pace: None,
            deadline: None,
            _slot: None,
        }
    }

    /// Coupe la connexion après `limit` sans aucun message reçu.
    pub fn idle_timeout(mut self, limit: Duration) -> Self {
        self.idle = Some((limit, Box::pin(tokio::time::sleep(limit))));
        self
    }

    /// Ne lit pas plus de `bytes_per_sec` octets par seconde.
    pub fn inbound_rate(mut self, bytes_per_sec: u64) -> Self {
        self.pace = Some(Pace {
            bytes_per_sec: bytes_per_sec.max(1),
            window: Instant::now(),
            received: 0,
            wait: None,
        });
        self
    }

    /// Coupe la connexion au bout de `limit`, sauf si `cleared` est levé d'ici
    /// là (certificat client présenté).
    pub fn unauthenticated_for(mut self, limit: Duration, cleared: Arc<AtomicBool>) -> Self {
        self.deadline = Some((Box::pin(tokio::time::sleep(limit)), cleared));
        self
    }

    fn holding(mut self, slot: IpSlot) -> Self {
        self._slot = Some(slot);
        self
    }

    /// Erreur si la session est restée sans certificat au-delà de son délai.
    fn check_deadline(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        let expired = match &mut self.deadline {
            None => return Ok(()),
            Some((_, cleared)) if cleared.load(Ordering::Relaxed) => None,
            Some((sleep, _)) => Some(sleep.as_mut().poll(cx).is_ready()),
        };
        match expired {
            None => {
                self.deadline = None;
                Ok(())
            }
            Some(true) => Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "agent websocket: no client certificate, session closed",
            )),
            Some(false) => Ok(()),
        }
    }

    /// Un message vient d'arriver : le silence repart de zéro.
    fn touch(&mut self) {
        if let Some((limit, sleep)) = &mut self.idle {
            sleep.as_mut().reset(Instant::now() + *limit);
        }
    }

    /// Compte `n` octets reçus ; au-delà du débit, la lecture suivante attend
    /// la fenêtre d'après.
    fn pace_after(&mut self, n: usize) {
        let Some(pace) = &mut self.pace else { return };
        let now = Instant::now();
        let second = Duration::from_secs(1);
        if now.duration_since(pace.window) >= second {
            pace.window = now;
            pace.received = 0;
        }
        pace.received += n as u64;
        if pace.received > pace.bytes_per_sec {
            let next = pace.window + second;
            pace.window = next;
            pace.received = 0;
            pace.wait = Some(Box::pin(tokio::time::sleep_until(next)));
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for WsIo<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        loop {
            if !this.pending.is_empty() {
                let n = this.pending.len().min(buf.remaining());
                buf.put_slice(&this.pending.split_to(n));
                return Poll::Ready(Ok(()));
            }
            if this.ended {
                return Poll::Ready(Ok(()));
            }
            if let Some(pace) = &mut this.pace
                && let Some(wait) = &mut pace.wait
            {
                ready!(wait.as_mut().poll(cx));
                pace.wait = None;
            }
            if let Some((_, idle)) = &mut this.idle
                && idle.as_mut().poll(cx).is_ready()
            {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "agent websocket: silent for too long",
                )));
            }
            match ready!(Pin::new(&mut this.ws).poll_next(cx)) {
                Some(Ok(message)) => {
                    this.touch();
                    match message {
                        Message::Binary(data) => {
                            this.pace_after(data.len());
                            this.pending = data;
                        }
                        // Les pongs partent d'eux-mêmes à la lecture suivante.
                        Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => {}
                        Message::Close(_) => this.ended = true,
                        Message::Text(_) => {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "agent websocket: text message on a binary channel",
                            )));
                        }
                    }
                }
                Some(Err(error)) => return Poll::Ready(Err(into_io(error))),
                None => this.ended = true,
            }
        }
    }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for WsIo<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        this.check_deadline(cx)?;
        ready!(Pin::new(&mut this.ws).poll_ready(cx)).map_err(into_io)?;
        let n = buf.len().min(AGENT_WS_CHUNK);
        let message = Message::Binary(Bytes::copy_from_slice(&buf[..n]));
        Pin::new(&mut this.ws).start_send(message).map_err(into_io)?;
        Poll::Ready(Ok(n))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        Pin::new(&mut this.ws).poll_flush(cx).map_err(into_io)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        match ready!(Pin::new(&mut this.ws).poll_close(cx)) {
            Ok(()) | Err(WsError::ConnectionClosed | WsError::AlreadyClosed) => Poll::Ready(Ok(())),
            Err(error) => Poll::Ready(Err(into_io(error))),
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

/// Refus d'une requête venue d'un navigateur.
const FROM_A_BROWSER: &str = "Browsers cannot open the agent channel.";

/// Refus d'une requête qui porte la session de l'interface.
const WITH_A_SESSION: &str = "The agent channel does not take a session cookie.";

/// Ce qui trahit un navigateur, ou une session : l'origine (toujours envoyée
/// par un navigateur pour une WebSocket), le cookie de session.
fn browser_refusal(headers: &HeaderMap) -> Option<&'static str> {
    if headers.contains_key(header::ORIGIN) {
        return Some(FROM_A_BROWSER);
    }
    let session = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.split_once('='))
        .any(|(name, _)| name.trim() == crate::auth::cookie::NAME);
    session.then_some(WITH_A_SESSION)
}

/// Vrai si l'en-tête (liste à virgules) contient ce jeton, à la casse près.
fn lists(headers: &HeaderMap, name: header::HeaderName, token: &str) -> bool {
    headers
        .get_all(name)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|item| item.trim().eq_ignore_ascii_case(token))
}

/// Clé `Sec-WebSocket-Key` d'une demande de mise à niveau conforme, ou ce
/// qui manque.
fn upgrade_key(headers: &HeaderMap) -> Result<String, &'static str> {
    if !lists(headers, header::UPGRADE, "websocket")
        || !lists(headers, header::CONNECTION, "upgrade")
    {
        return Err("This route only opens the agent WebSocket (Upgrade: websocket).");
    }
    let version = headers.get(header::SEC_WEBSOCKET_VERSION).and_then(|v| v.to_str().ok());
    if version.map(str::trim) != Some("13") {
        return Err("WebSocket version 13 is expected.");
    }
    if !lists(headers, header::SEC_WEBSOCKET_PROTOCOL, AGENT_WS_PROTOCOL) {
        return Err("The agent WebSocket subprotocol (dumbmonit-agent.v1) is expected.");
    }
    let key = headers
        .get(header::SEC_WEBSOCKET_KEY)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .unwrap_or_default();
    // Seize octets aléatoires en base64 (RFC 6455 §4.1) : rien d'autre.
    match BASE64.decode(key) {
        Ok(raw) if raw.len() == 16 => Ok(key.to_string()),
        _ => Err("Invalid Sec-WebSocket-Key."),
    }
}

/// `GET /api/agent/ws` (mise à niveau WebSocket) : le canal des agents à
/// l'adresse de l'interface. Rien n'y est authentifié : c'est la session TLS
/// de l'agent, à l'intérieur, qui l'est.
pub async fn upgrade(
    State(state): State<AppState>,
    ClientIp(client): ClientIp,
    mut request: Request,
) -> Response {
    if !state.config.agent_websocket {
        return error(StatusCode::NOT_FOUND, "Not found.");
    }
    if let Some(refusal) = browser_refusal(request.headers()) {
        return error(StatusCode::FORBIDDEN, refusal);
    }
    // Une mise à niveau n'existe qu'en HTTP/1.1 ; un mandataire la demande
    // ainsi à DumbMonit, quoi qu'il parle avec l'agent.
    if request.version() != Version::HTTP_11 {
        return error(
            StatusCode::HTTP_VERSION_NOT_SUPPORTED,
            "The agent WebSocket is opened over HTTP/1.1.",
        );
    }
    let key = match upgrade_key(request.headers()) {
        Ok(key) => key,
        Err(message) => {
            let mut response = error(StatusCode::UPGRADE_REQUIRED, message);
            let headers = response.headers_mut();
            headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
            headers.insert(header::SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));
            return response;
        }
    };
    let tls = state.agent_tls();
    let (Some(ready), Some(configs)) = (tls.ready(), tls.configs()) else {
        return error(StatusCode::SERVICE_UNAVAILABLE, "The agent channel is not ready.");
    };
    // L'adresse du client sert aux compteurs (places, tentatives d'enrôlement),
    // jamais à l'identité.
    let socket = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|info| info.0);
    let ip = client.or(socket.map(|addr| addr.ip())).unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    let addr = SocketAddr::new(ip, socket.map_or(0, |addr| addr.port()));
    let Some(slot) = tls.ws_limits().reserve(ip, state.config.agent_ws_max_per_ip) else {
        tracing::warn!(%ip, "agent websocket: connection ceiling reached for this address");
        return error(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many agent connections from this address.",
        );
    };
    let Ok(permit) = tls.ws_limits().handshakes.clone().try_acquire_owned() else {
        tracing::debug!(%ip, "agent websocket: too many handshakes in flight");
        return error(StatusCode::SERVICE_UNAVAILABLE, "Too many agent connections being opened.");
    };
    let rate = u64::from(state.config.agent_ws_rate_kib) * 1024;
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let upgraded = match upgrade.await {
            Ok(upgraded) => hyper_util::rt::TokioIo::new(upgraded),
            Err(cause) => {
                tracing::debug!(%addr, %cause, "agent websocket: upgrade failed");
                return;
            }
        };
        let ws = WebSocketStream::from_raw_socket(upgraded, Role::Server, Some(config())).await;
        let certified = Arc::new(AtomicBool::new(false));
        let io = WsIo::new(ws)
            .idle_timeout(IDLE_TIMEOUT)
            .inbound_rate(rate)
            .unauthenticated_for(UNAUTHENTICATED_LIFETIME, certified.clone())
            .holding(slot);
        let transport: Box<dyn Transport> = Box::new(io);
        let handshake = listener::handshake(configs, transport, addr, None).await;
        drop(permit);
        let Some((stream, mut peer)) = handshake else { return };
        peer.via_websocket = true;
        if peer.certificate.is_some() {
            certified.store(true, Ordering::Relaxed);
        }
        tracing::debug!(%addr, certified = peer.certificate.is_some(), "agent websocket open");
        let _ = ready.send((stream, peer)).await;
    });
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    let headers = response.headers_mut();
    headers.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
    headers.insert(header::UPGRADE, HeaderValue::from_static("websocket"));
    headers.insert(header::SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(AGENT_WS_PROTOCOL));
    if let Ok(accept) = HeaderValue::from_str(&derive_accept_key(key.as_bytes())) {
        headers.insert(header::SEC_WEBSOCKET_ACCEPT, accept);
    }
    response
}

/// Adresse de la WebSocket des agents pour une URL de l'interface
/// (`https://hôte[:port]`), telle qu'un agent l'ouvre.
pub fn url_for(base_url: &str) -> String {
    format!("{}{AGENT_WS_PATH}", base_url.trim_end_matches('/'))
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn upgrade_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::UPGRADE, HeaderValue::from_static("WebSocket"));
        headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive, Upgrade"));
        headers.insert(header::SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));
        headers.insert(
            header::SEC_WEBSOCKET_KEY,
            HeaderValue::from_static("dGhlIHNhbXBsZSBub25jZQ=="),
        );
        headers.insert(header::SEC_WEBSOCKET_PROTOCOL, HeaderValue::from_static(AGENT_WS_PROTOCOL));
        headers
    }

    #[test]
    fn a_conforming_upgrade_gives_its_key_and_anything_else_is_refused() {
        assert_eq!(upgrade_key(&upgrade_headers()).as_deref(), Ok("dGhlIHNhbXBsZSBub25jZQ=="));
        for (name, value) in [
            (header::UPGRADE, "h2c"),
            (header::CONNECTION, "close"),
            (header::SEC_WEBSOCKET_VERSION, "8"),
            (header::SEC_WEBSOCKET_KEY, "trop-court"),
            (header::SEC_WEBSOCKET_PROTOCOL, "chat"),
        ] {
            let mut headers = upgrade_headers();
            headers.insert(name.clone(), HeaderValue::from_static(value));
            assert!(upgrade_key(&headers).is_err(), "{name}: {value}");
        }
    }

    #[test]
    fn a_browser_or_a_session_never_opens_the_agent_channel() {
        let mut headers = upgrade_headers();
        assert_eq!(browser_refusal(&headers), None);
        headers.insert(header::COOKIE, HeaderValue::from_static("theme=dark"));
        assert_eq!(browser_refusal(&headers), None, "un cookie quelconque ne dit rien");
        let session = format!("theme=dark; {}=abc.def", crate::auth::cookie::NAME);
        headers.insert(header::COOKIE, HeaderValue::from_str(&session).unwrap());
        assert_eq!(browser_refusal(&headers), Some(WITH_A_SESSION));
        let mut headers = upgrade_headers();
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert_eq!(browser_refusal(&headers), Some(FROM_A_BROWSER));
    }

    #[test]
    fn an_address_cannot_hold_more_connections_than_its_ceiling() {
        let limits = Limits::default();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let first = limits.reserve(ip, 2).expect("première");
        let second = limits.reserve(ip, 2).expect("deuxième");
        assert!(limits.reserve(ip, 2).is_none(), "plafond atteint");
        assert!(limits.reserve("192.0.2.8".parse().unwrap(), 2).is_some(), "autre adresse");
        drop(first);
        assert_eq!(limits.open_from(ip), 1);
        drop(second);
        assert_eq!(limits.open_from(ip), 0);
    }

    #[test]
    fn the_agent_websocket_url_sits_under_the_interface_address() {
        assert_eq!(url_for("https://monit.example.com/"), "https://monit.example.com/api/agent/ws");
    }

    /// Deux WsIo face à face : les octets passent dans l'ordre, découpés en
    /// messages, et la fermeture d'un côté est la fin du flux de l'autre.
    #[tokio::test]
    async fn bytes_cross_a_websocket_in_order_and_close_ends_the_stream() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (near, far) = tokio::io::duplex(1024 * 1024);
        let (client, server) = tokio::join!(
            WebSocketStream::from_raw_socket(near, Role::Client, Some(config())),
            WebSocketStream::from_raw_socket(far, Role::Server, Some(config())),
        );
        let mut client = WsIo::new(client);
        let mut server = WsIo::new(server).idle_timeout(Duration::from_secs(30));
        // Plus qu'un message : le flux est découpé, puis recousu.
        let sent: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let writer = async {
            client.write_all(&sent).await.unwrap();
            client.flush().await.unwrap();
            client.shutdown().await.unwrap();
        };
        let mut received = Vec::new();
        let reader = async {
            server.read_to_end(&mut received).await.unwrap();
        };
        tokio::time::timeout(Duration::from_secs(10), async { tokio::join!(writer, reader) })
            .await
            .expect("délai");
        assert_eq!(received, sent);
    }

    #[tokio::test]
    async fn a_text_message_breaks_the_channel() {
        use futures::SinkExt;
        use tokio::io::AsyncReadExt;

        let (near, far) = tokio::io::duplex(64 * 1024);
        let (mut client, server) = tokio::join!(
            WebSocketStream::from_raw_socket(near, Role::Client, Some(config())),
            WebSocketStream::from_raw_socket(far, Role::Server, Some(config())),
        );
        client.send(Message::text("GET / HTTP/1.1")).await.unwrap();
        let mut server = WsIo::new(server);
        let mut buffer = [0u8; 16];
        let read = server.read(&mut buffer).await;
        assert_eq!(read.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test(start_paused = true)]
    async fn a_session_without_certificate_is_cut_unless_cleared() {
        use tokio::io::AsyncReadExt;

        for cleared in [false, true] {
            let (near, far) = tokio::io::duplex(64 * 1024);
            let (_client, server) = tokio::join!(
                WebSocketStream::from_raw_socket(near, Role::Client, Some(config())),
                WebSocketStream::from_raw_socket(far, Role::Server, Some(config())),
            );
            let flag = Arc::new(AtomicBool::new(cleared));
            let mut server =
                WsIo::new(server).unauthenticated_for(Duration::from_secs(60), flag.clone());
            let mut buffer = [0u8; 16];
            let read =
                tokio::time::timeout(Duration::from_secs(120), server.read(&mut buffer)).await;
            if cleared {
                assert!(read.is_err(), "rien reçu, rien coupé : l'attente continue");
            } else {
                let error = read.expect("coupée avant le délai du test").unwrap_err();
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            }
        }
    }
}
