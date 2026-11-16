//! Port unique : l'interface, l'API et le canal des agents sur une seule
//! écoute (`DUMBMONIT_SINGLE_PORT`, typiquement 443).
//!
//! Facultatif : sans ce réglage, rien ne change (interface sur 8080, agents sur
//! 8443). Avec lui, les deux autres ports restent ouverts — un agent antérieur
//! continue par le port des agents —, et cette écoute s'y ajoute. Chaque
//! connexion y est aiguillée avant toute application :
//!
//! - **premier octet** : une requête en clair reçoit la même redirection `308`
//!   vers HTTPS que le port de l'interface, sans rien de l'application ;
//! - **ClientHello** lu sans engager de configuration : s'il propose le
//!   protocole ALPN [`dumbmonit_proto::AGENT_ALPN`] (un agent à jour le fait
//!   toujours), la poignée de main se poursuit sous la configuration des agents
//!   — autorité interne, certificat client, épinglage — et la connexion est
//!   servie par les routes du port des agents, avec les mêmes contrôles ;
//! - **tout le reste** (navigateur, `curl`, PowerShell, y compris en TLS 1.2)
//!   poursuit sous le certificat de l'interface et voit l'interface et l'API,
//!   exactement comme sur le port de l'interface. Les commandes d'installation
//!   y épinglent donc la clé de ce certificat-là.
//!
//! Le relais (sa connexion multiplexée, ses tunnels) passe par le côté agents,
//! comme un agent. Une connexion n'est jamais servie par les deux côtés : le
//! choix est fait une fois, sur le ClientHello, et la poignée de main de
//! l'autre configuration n'a pas lieu.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;
use tokio_rustls::LazyConfigAcceptor;

use crate::agent_tls::{self, AgentIo, AgentPeer, Transport};
use crate::state::AppState;
use crate::web_tls::{
    FIRST_BYTE_TIMEOUT, HANDSHAKE_TIMEOUT, TLS_HANDSHAKE_RECORD, WebFeed, WebIo, WebListener,
    WebPeer, redirect_plain,
};

/// Connexions en cours de tri au plus (premier octet, ClientHello, poignée de
/// main) : la somme des deux autres ports.
const MAX_PENDING: usize = 1024 + 256;

/// Une connexion triée.
enum Routed {
    /// Un agent : servi par les routes du port des agents.
    Agent(AgentIo, AgentPeer),
    /// Tout le reste : servi par l'interface.
    Web(WebIo, WebPeer),
}

/// Ouvre l'écoute unique. Les connexions établies rejoignent celles du port
/// des agents (déjà ouvert) et celles de `web`.
pub async fn listen(state: AppState, web: &WebListener) -> Result<SocketAddr> {
    let bind = state.config.single_listen.context("DUMBMONIT_SINGLE_PORT is not set")?;
    let agents = state
        .agent_tls()
        .ready()
        .context("the single port needs the agent channel, which is not listening")?;
    let tcp = TcpListener::bind(bind)
        .await
        .with_context(|| format!("cannot listen on {bind} (DUMBMONIT_SINGLE_PORT)"))?;
    let local = tcp.local_addr().context("single port address")?;
    tokio::spawn(accept_loop(state, tcp, agents, web.feed()));
    Ok(local)
}

async fn accept_loop(state: AppState, tcp: TcpListener, agents: agent_tls::Ready, web: WebFeed) {
    let pending = Arc::new(Semaphore::new(MAX_PENDING));
    loop {
        if agents.is_closed() && web.is_closed() {
            return;
        }
        let (stream, addr) = match tcp.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::debug!(%error, "single port: accept failed");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let Ok(permit) = pending.clone().try_acquire_owned() else {
            tracing::debug!(%addr, "single port: too many connections being opened");
            continue;
        };
        let state = state.clone();
        let agents = agents.clone();
        let web = web.clone();
        tokio::spawn(async move {
            let routed = classify(&state, stream, addr).await;
            drop(permit);
            match routed {
                Some(Routed::Agent(io, peer)) => {
                    let _ = agents.send((io, peer)).await;
                }
                Some(Routed::Web(io, peer)) => {
                    let _ = web.send((io, peer)).await;
                }
                None => {}
            }
        });
    }
}

/// Trie une connexion : en clair, redirigée puis fermée ; en TLS, menée à son
/// terme sous la configuration que son ClientHello désigne.
async fn classify(state: &AppState, stream: TcpStream, addr: SocketAddr) -> Option<Routed> {
    let mut first = [0u8; 1];
    match tokio::time::timeout(FIRST_BYTE_TIMEOUT, stream.peek(&mut first)).await {
        Ok(Ok(read)) if read > 0 => {}
        _ => return None,
    }
    if first[0] != TLS_HANDSHAKE_RECORD {
        redirect_plain(stream, addr).await;
        return None;
    }
    let transport: Box<dyn Transport> = Box::new(stream);
    let routed = tokio::time::timeout(HANDSHAKE_TIMEOUT, async {
        let acceptor = rustls::server::Acceptor::default();
        let start = LazyConfigAcceptor::new(acceptor, transport).await.ok()?;
        if agent_tls::offers_agent_protocol(&start.client_hello()) {
            let config = state.agent_tls().configs()?.agent;
            let tls = start.into_stream(config).await.ok()?;
            let peer = agent_tls::peer_of(&tls, addr, None);
            Some(Routed::Agent(tls, peer))
        } else {
            let config = state.web_tls().server_config()?;
            let tls = start.into_stream(config).await.ok()?;
            Some(Routed::Web(tls, WebPeer { addr }))
        }
    })
    .await;
    match routed {
        Ok(Some(routed)) => Some(routed),
        Ok(None) => {
            tracing::debug!(%addr, "single port: TLS handshake refused");
            None
        }
        Err(_) => None,
    }
}
