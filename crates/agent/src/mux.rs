//! Connexion multiplexée d'un relais vers le serveur.
//!
//! Au lieu d'ouvrir un tunnel (connexion TCP, poignée de main TLS, mise à
//! niveau HTTP) par connexion d'un agent du site, le relais garde une seule
//! connexion mise à niveau ([`dumbmonit_proto::RELAY_MUX_PROTOCOL`]) et y ouvre
//! un flux [yamux] par connexion. Le flux porte, comme un tunnel, la session
//! TLS de l'agent du site de bout en bout : le relais n'y voit que du chiffré.
//!
//! yamux demande qu'une seule tâche mène la connexion : [`spawn`] la lance, et
//! [`MuxHandle::open`] lui demande un flux. Quand la connexion tombe (serveur
//! redémarré, réseau coupé), `open` rend `None` : la passerelle en rouvre une.
//!
//! [yamux]: https://github.com/hashicorp/yamux/blob/master/spec.md

use std::collections::VecDeque;
use std::task::Poll;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{mpsc, oneshot};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};
use tracing::debug;

/// Flux d'une connexion multiplexée, vu comme une connexion tokio.
pub type MuxStream = Compat<yamux::Stream>;

/// Flux ouverts au plus sur une connexion : la limite du serveur. yamux coupe
/// la connexion entière au-delà, d'où la borne de la passerelle
/// ([`crate::gateway::MAX_CONNECTIONS`]), bien en dessous.
pub const MAX_STREAMS: usize = 4096;

/// Demande d'un flux à la tâche qui mène la connexion.
type Request = oneshot::Sender<yamux::Stream>;

/// Poignée vers une connexion multiplexée menée par sa propre tâche.
#[derive(Clone)]
pub struct MuxHandle {
    requests: mpsc::Sender<Request>,
}

impl MuxHandle {
    /// Ouvre un flux ; `None` si la connexion est tombée.
    pub async fn open(&self) -> Option<MuxStream> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(reply).await.ok()?;
        let stream = answer.await.ok()?;
        Some(FuturesAsyncReadCompatExt::compat(stream))
    }

    /// Vrai tant que la connexion est menée.
    pub fn is_open(&self) -> bool {
        !self.requests.is_closed()
    }
}

/// Réglages yamux du relais.
fn config() -> yamux::Config {
    let mut config = yamux::Config::default();
    config.set_max_num_streams(MAX_STREAMS);
    config
}

/// Mène une connexion multiplexée (côté relais) sur `socket`, déjà mis à
/// niveau, jusqu'à sa fin ou la disparition de toutes les poignées.
pub fn spawn<T>(socket: T) -> MuxHandle
where
    T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let (requests, receiver) = mpsc::channel(64);
    let socket = TokioAsyncReadCompatExt::compat(socket);
    let connection = yamux::Connection::new(socket, config(), yamux::Mode::Client);
    tokio::spawn(drive(connection, receiver));
    MuxHandle { requests }
}

/// La tâche de la connexion : ouvre les flux demandés et fait avancer les
/// lectures, les écritures et les fenêtres de tous.
async fn drive<T>(
    mut connection: yamux::Connection<Compat<T>>,
    mut requests: mpsc::Receiver<Request>,
) where
    T: AsyncRead + AsyncWrite + Unpin,
{
    let mut waiting: VecDeque<Request> = VecDeque::new();
    let mut handles_gone = false;
    let ended = std::future::poll_fn(|cx| {
        // Nouvelles demandes.
        while !handles_gone {
            match requests.poll_recv(cx) {
                Poll::Ready(Some(reply)) => waiting.push_back(reply),
                Poll::Ready(None) => handles_gone = true,
                Poll::Pending => break,
            }
        }
        // Flux demandés : prêts aussitôt, sauf si trop attendent encore
        // l'accord du serveur (yamux réveille alors la tâche).
        while !waiting.is_empty() {
            match connection.poll_new_outbound(cx) {
                Poll::Ready(Ok(stream)) => {
                    if let Some(reply) = waiting.pop_front() {
                        // Demande abandonnée entre-temps : le flux est refermé.
                        let _ = reply.send(stream);
                    }
                }
                Poll::Ready(Err(error)) => return Poll::Ready(Some(error)),
                Poll::Pending => break,
            }
        }
        if handles_gone && waiting.is_empty() {
            return Poll::Ready(None);
        }
        // La connexion elle-même. Le serveur n'ouvre jamais de flux vers le
        // relais : s'il en venait un, il serait refermé.
        loop {
            match connection.poll_next_inbound(cx) {
                Poll::Ready(Some(Ok(stream))) => drop(stream),
                Poll::Ready(Some(Err(error))) => return Poll::Ready(Some(error)),
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    })
    .await;
    match ended {
        Some(error) => debug!(%error, "relay: multiplexed connection to the server failed"),
        None => debug!("relay: multiplexed connection to the server closed"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    /// Serveur factice : accepte les flux et renvoie chacun en écho.
    fn echo_server<T>(socket: T)
    where
        T: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        tokio::spawn(async move {
            let socket = TokioAsyncReadCompatExt::compat(socket);
            let mut server = yamux::Connection::new(socket, config(), yamux::Mode::Server);
            while let Some(Ok(stream)) =
                std::future::poll_fn(|cx| server.poll_next_inbound(cx)).await
            {
                tokio::spawn(async move {
                    let mut stream = FuturesAsyncReadCompatExt::compat(stream);
                    let mut buffer = [0u8; 256];
                    while let Ok(read) = stream.read(&mut buffer).await {
                        if read == 0 || stream.write_all(&buffer[..read]).await.is_err() {
                            break;
                        }
                    }
                    let _ = stream.shutdown().await;
                });
            }
        });
    }

    #[tokio::test]
    async fn many_streams_share_one_connection_each_with_its_own_bytes() {
        let (near, far) = tokio::io::duplex(1024 * 1024);
        echo_server(far);
        let handle = spawn(near);
        let mut streams = Vec::new();
        for index in 0..20u8 {
            let stream = handle.open().await.expect("flux");
            streams.push((index, stream));
        }
        for (index, stream) in &mut streams {
            let message = [*index; 100];
            stream.write_all(&message).await.unwrap();
            let mut back = [0u8; 100];
            tokio::time::timeout(Duration::from_secs(5), stream.read_exact(&mut back))
                .await
                .expect("délai")
                .expect("écho");
            assert_eq!(back, message, "chaque flux reçoit ses propres octets");
        }
        assert!(handle.is_open());
    }

    #[tokio::test]
    async fn a_closed_connection_says_so_instead_of_hanging() {
        let (near, far) = tokio::io::duplex(64 * 1024);
        let handle = spawn(near);
        // Le serveur disparaît : la connexion tombe, `open` rend `None`.
        drop(far);
        let opened = tokio::time::timeout(Duration::from_secs(5), async {
            // Le premier flux peut encore être rendu avant que la fin ne soit
            // lue ; les suivants, non.
            for _ in 0..3 {
                if handle.open().await.is_none() {
                    return true;
                }
                tokio::task::yield_now().await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            false
        })
        .await
        .expect("délai");
        assert!(opened, "la fin de la connexion est vue");
        assert!(!handle.is_open());
    }
}
