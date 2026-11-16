//! Tunnels des agents relais.
//!
//! Sur un site derrière un pare-feu, seul l'agent relais sort vers le serveur.
//! Les autres agents du site se connectent à sa passerelle, qui ouvre pour
//! chacun un tunnel ici : une mise à niveau HTTP sur le port des agents, sous
//! le certificat client du relais. Le flux mis à niveau est ensuite traité
//! comme une connexion neuve à ce port — poignée de main TLS 1.3, certificat
//! client de l'agent du site, mêmes routes et mêmes contrôles —, avec en plus
//! le relais qui le porte ([`AgentPeer::via_relay`]).
//!
//! **Connexion multiplexée** (`Upgrade:` [`RELAY_MUX_PROTOCOL`]) : un relais à
//! jour n'ouvre plus un tunnel par connexion, mais une seule connexion mise à
//! niveau, sur laquelle il ouvre un flux [yamux] par connexion d'agent du site.
//! Chaque flux est traité exactement comme un tunnel (même poignée de main de
//! bout en bout, même plafond, mêmes contrôles) ; seul le transport change :
//! une poignée de main TLS et un socket de moins par agent, côté relais comme
//! côté serveur.
//!
//! Le relais ne fait que transporter du chiffré : il ne lit rien, ne peut rien
//! modifier, et ne peut pas se faire passer pour l'agent. Ce qu'il pourrait
//! tenter est borné ici : un nombre de tunnels par relais
//! (`DUMBMONIT_RELAY_MAX_TUNNELS`, flux multiplexés compris), la même limite
//! de poignées de main que le port lui-même, et les contrôles de
//! [`super::routes`] (un agent d'un autre client ne passe pas par lui).
//!
//! [yamux]: https://github.com/hashicorp/yamux/blob/master/spec.md
//! [`RELAY_MUX_PROTOCOL`]: dumbmonit_proto::RELAY_MUX_PROTOCOL

use std::collections::HashMap;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use dumbmonit_proto::TargetId;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

use super::Configs;
use super::listener::{self, Transport};
use crate::state::AppState;

/// Tunnels simultanés par relais, par défaut (`DUMBMONIT_RELAY_MAX_TUNNELS`) :
/// de quoi servir un gros site (une connexion gardée ouverte par agent à jour,
/// qui parle HTTP/2), pas de quoi épuiser le serveur.
pub const DEFAULT_MAX_TUNNELS_PER_RELAY: usize = 1024;

/// Place de tunnel réservée pour un relais ; rendue à la fermeture.
pub struct Slot {
    open: Arc<Mutex<HashMap<TargetId, usize>>>,
    relay: TargetId,
}

impl Slot {
    pub fn reserve(
        open: &Arc<Mutex<HashMap<TargetId, usize>>>,
        relay: TargetId,
        max: usize,
    ) -> Option<Self> {
        let mut counts = open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = counts.entry(relay).or_insert(0);
        if *count >= max {
            return None;
        }
        *count += 1;
        Some(Self { open: open.clone(), relay })
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut counts = self.open.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(count) = counts.get_mut(&self.relay) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                counts.remove(&self.relay);
            }
        }
    }
}

/// Transport qui garde sa place de tunnel tant qu'il vit.
struct Guarded<T> {
    inner: T,
    _slot: Slot,
}

impl<T: AsyncRead + Unpin> AsyncRead for Guarded<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Guarded<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Pourquoi un tunnel n'a pas été accordé.
#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Le port des agents n'écoute pas (ou plus).
    Closed,
    /// Ce relais a atteint son plafond de tunnels.
    Busy,
}

/// Tunnel accordé : sa place est réservée, il ne reste qu'à le brancher.
pub struct Grant {
    slot: Slot,
    ready: listener::Ready,
    configs: Configs,
}

/// Réserve un tunnel pour `relay`, avant de répondre `101` au relais.
pub fn grant(state: &AppState, relay: TargetId) -> Result<Grant, Refusal> {
    let tls = state.agent_tls();
    let ready = tls.ready().ok_or(Refusal::Closed)?;
    let configs = tls.configs().ok_or(Refusal::Closed)?;
    let slot = tls.reserve_tunnel(relay).ok_or(Refusal::Busy)?;
    Ok(Grant { slot, ready, configs })
}

impl Grant {
    /// Branche le flux mis à niveau sur le port des agents : poignée de main
    /// TLS de l'agent du site, puis service ordinaire.
    pub async fn attach<T>(self, upgraded: T, relay: TargetId, addr: SocketAddr)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let transport: Box<dyn Transport> = Box::new(Guarded { inner: upgraded, _slot: self.slot });
        if let Some(connection) =
            listener::handshake(self.configs, transport, addr, Some(relay)).await
        {
            let _ = self.ready.send(connection).await;
        }
    }

    /// Sert la connexion multiplexée d'un relais : chaque flux qu'il y ouvre
    /// devient une connexion au port des agents, comme un tunnel, jusqu'à la
    /// fin de la connexion. La place réservée par [`grant`] est celle de la
    /// connexion elle-même ; chaque flux en réserve une de plus, rendue à sa
    /// fermeture.
    pub async fn multiplex<T>(self, state: AppState, upgraded: T, relay: TargetId, addr: SocketAddr)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let Grant { slot: _session, ready, configs } = self;
        let config = mux_config();
        let socket = TokioAsyncReadCompatExt::compat(upgraded);
        let mut connection = yamux::Connection::new(socket, config, yamux::Mode::Server);
        tracing::debug!(relay, %addr, "agent relay: multiplexed connection open");
        loop {
            let stream = match std::future::poll_fn(|cx| connection.poll_next_inbound(cx)).await {
                Some(Ok(stream)) => stream,
                Some(Err(error)) => {
                    tracing::debug!(relay, %error, "agent relay: multiplexed connection failed");
                    break;
                }
                None => break,
            };
            let Some(slot) = state.agent_tls().reserve_tunnel(relay) else {
                // Le flux est fermé aussitôt ; le relais (et l'agent du site)
                // réessaient plus tard, comme pour un tunnel refusé.
                tracing::warn!(relay, "agent relay: tunnel ceiling reached");
                continue;
            };
            // Certificat du jour : la connexion peut durer plus qu'un
            // renouvellement.
            let configs = state.agent_tls().configs().unwrap_or_else(|| configs.clone());
            let ready = ready.clone();
            let inner = FuturesAsyncReadCompatExt::compat(stream);
            let transport: Box<dyn Transport> = Box::new(Guarded { inner, _slot: slot });
            tokio::spawn(async move {
                if let Some(connection) =
                    listener::handshake(configs, transport, addr, Some(relay)).await
                {
                    let _ = ready.send(connection).await;
                }
            });
        }
        tracing::debug!(relay, "agent relay: multiplexed connection closed");
    }
}

/// Réglages yamux d'une connexion multiplexée.
///
/// yamux coupe la connexion *entière* quand l'autre bout ouvre un flux de trop :
/// sa limite est donc le plafond absolu ([`crate::config::MAX_RELAY_TUNNELS`],
/// ce que sa fenêtre de réception par défaut, un gibioctet, permet en flux de
/// 256 Kio), et le plafond réglé d'un relais est tenu à part, par les places
/// de tunnel : un flux de trop est seulement refusé, les autres continuent. Le
/// relais, lui, n'ouvre jamais plus de flux que sa passerelle n'accepte de
/// connexions (2048 au plus).
pub fn mux_config() -> yamux::Config {
    let mut config = yamux::Config::default();
    config.set_max_num_streams(crate::config::MAX_RELAY_TUNNELS);
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relay_cannot_hold_more_tunnels_than_its_ceiling_and_closing_frees_a_place() {
        let open = Arc::new(Mutex::new(HashMap::new()));
        let first = Slot::reserve(&open, 7, 2).expect("première place");
        let second = Slot::reserve(&open, 7, 2).expect("deuxième place");
        assert!(Slot::reserve(&open, 7, 2).is_none(), "plafond atteint");
        // Un autre relais a ses propres places.
        let other = Slot::reserve(&open, 8, 2).expect("autre relais");
        drop(first);
        let third = Slot::reserve(&open, 7, 2).expect("une place s'est libérée");
        drop((second, third, other));
        assert!(open.lock().unwrap().is_empty(), "plus rien d'ouvert");
    }

    #[test]
    fn the_multiplexed_connection_settings_are_accepted_by_yamux() {
        // Au-delà de ce que la fenêtre de réception autorise,
        // `set_max_num_streams` paniquerait.
        let _ = mux_config();
    }

    /// Un relais factice ouvre deux flux sur une connexion multiplexée : les
    /// octets passent dans les deux sens, chacun dans son flux.
    #[tokio::test]
    async fn two_streams_share_one_multiplexed_connection() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (near, far) = tokio::io::duplex(256 * 1024);
        // Côté serveur : un écho par flux.
        tokio::spawn(async move {
            let socket = TokioAsyncReadCompatExt::compat(far);
            let mut server = yamux::Connection::new(socket, mux_config(), yamux::Mode::Server);
            while let Some(Ok(stream)) =
                std::future::poll_fn(|cx| server.poll_next_inbound(cx)).await
            {
                tokio::spawn(async move {
                    let mut stream = FuturesAsyncReadCompatExt::compat(stream);
                    let mut buffer = [0u8; 64];
                    while let Ok(read) = stream.read(&mut buffer).await {
                        if read == 0 || stream.write_all(&buffer[..read]).await.is_err() {
                            break;
                        }
                    }
                    let _ = stream.shutdown().await;
                });
            }
        });
        // Côté relais : la connexion est menée par sa propre tâche.
        let socket = TokioAsyncReadCompatExt::compat(near);
        let mut client = yamux::Connection::new(socket, mux_config(), yamux::Mode::Client);
        let first = std::future::poll_fn(|cx| client.poll_new_outbound(cx)).await.unwrap();
        let second = std::future::poll_fn(|cx| client.poll_new_outbound(cx)).await.unwrap();
        tokio::spawn(async move {
            while let Some(Ok(_)) = std::future::poll_fn(|cx| client.poll_next_inbound(cx)).await {}
        });
        for (stream, message) in [(first, b"premier".as_slice()), (second, b"second".as_slice())] {
            let mut stream = FuturesAsyncReadCompatExt::compat(stream);
            stream.write_all(message).await.unwrap();
            let mut back = vec![0u8; message.len()];
            tokio::time::timeout(std::time::Duration::from_secs(5), stream.read_exact(&mut back))
                .await
                .expect("délai")
                .expect("écho");
            assert_eq!(back, message);
        }
    }
}
