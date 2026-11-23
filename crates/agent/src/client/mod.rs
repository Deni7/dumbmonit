//! Envoi des lots au serveur DumbMonit.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use anyhow::{Context, Result};
use dumbmonit_proto::{
    AGENT_KEY_HEADER, AGENT_SECRET_HEADER, AGENT_TLS_HEADER, AgentCommand, BacklogAck,
    CERT_REFUSED, COMMANDS_PATH, CommandReport, INGEST_PATH, LOGS_PATH, LogBatch, ProbeOutcome,
    PushAck, PushBatch, RELAY_BACKLOG_PATH, RELAY_MUX_PROTOCOL, RELAY_PATH, RELAY_PLAN_PATH,
    RELAY_RESYNC_HEADER, RELAY_TUNNEL_PATH, RELAY_TUNNEL_PROTOCOL, RelayBacklog, RelayPlan,
    TLS_ENROL_PATH, TLS_RENEW_PATH, TlsCertificate, TlsEnrolRequest, TlsOffer, TlsRenewRequest,
};
use tokio::sync::Notify;

use crate::proxy::{ProxyConfig, ProxyMode};
use crate::tls::{self, Pins};
use crate::ws::{Route, Transport, TransportMode};

mod exchange;
mod util;

pub(crate) use self::util::describe;
use self::util::*;

/// Cible du canal chiffré : URL du port des agents, certificat et clé.
type TlsTarget = (String, Option<(Vec<u8>, Vec<u8>)>);

/// Délai de l'essai d'un autre transport (`transport: auto`).
const PROBE_TIMEOUT: Duration = Duration::from_secs(15);

/// Ce qui peut arriver à un envoi, et surtout ce qu'il faut en faire.
///
/// La distinction est celle qui décide du sort du tampon : un lot refusé pour son
/// contenu ne deviendra jamais valide et doit être jeté, alors qu'un serveur
/// injoignable finira par revenir et mérite qu'on garde les mesures.
#[derive(Debug)]
pub enum PushError {
    /// Serveur injoignable, en erreur, ou trop lent. On réessaie.
    Transport(String),
    /// Jeton refusé. On réessaie aussi — le jeton a pu être recréé côté serveur —
    /// mais en le disant clairement dans les journaux, car sans intervention
    /// humaine cette machine ne remontera plus jamais rien.
    Unauthorized,
    /// Le jeton est bon, mais le serveur ne reconnaît pas cette machine-ci :
    /// liaison inconnue, ou jeton qui ne peut plus enrôler. Réessayable, parce
    /// que quelqu'un peut ouvrir la fenêtre de réenrôlement dans l'interface
    /// pendant que l'agent patiente — mais le message doit être rendu tel quel,
    /// c'est lui qui dit quoi faire.
    Forbidden(String),
    /// Corps trop gros pour un intermédiaire (413). Le lot n'est pas fautif,
    /// sa taille si : on le garde, et on réessaie par morceaux plus petits.
    TooLarge(String),
    /// Certificat client refusé par le canal chiffré (révoqué, machine
    /// supprimée) : l'agent l'abandonne et se réenrôle avec son jeton.
    CertificateRefused,
    /// Le serveur a compris et refusé. Réessayer à l'identique est sans espoir.
    Rejected { status: u16, message: String },
}

impl PushError {
    /// Vrai si le lot mérite d'être conservé pour une nouvelle tentative.
    pub fn is_retryable(&self) -> bool {
        !matches!(self, Self::Rejected { .. })
    }
}

impl std::fmt::Display for PushError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transport(detail) => write!(f, "server unreachable: {detail}"),
            Self::Unauthorized => write!(f, "enrollment token rejected"),
            Self::Forbidden(message) => write!(f, "{message}"),
            Self::TooLarge(message) => {
                write!(f, "batch too large for the server or a proxy: {message}")
            }
            Self::Rejected { status, message } => write!(f, "batch rejected ({status}): {message}"),
            Self::CertificateRefused => write!(f, "agent certificate refused by the server"),
        }
    }
}

/// Le chemin vers le serveur : client HTTP et racine des URL.
struct Channel {
    /// Toutes les requêtes : en HTTP/2 sur le canal chiffré quand le serveur le
    /// propose, une seule connexion pour les lots, les commandes et le relais.
    http: reqwest::Client,
    /// Mises à niveau du relais (tunnels, connexion multiplexée) : HTTP/1.1,
    /// une connexion à part. Seulement sur le canal chiffré.
    upgrade: Option<reqwest::Client>,
    /// Racine du serveur, sans barre oblique finale.
    base_url: String,
    /// Vrai sur le canal chiffré (TLS 1.3, autorité épinglée).
    encrypted: bool,
}

#[derive(Clone)]
pub struct PushClient {
    /// Canal courant, partagé par toutes les copies du client : quand l'agent
    /// passe au canal chiffré ou renouvelle son certificat, la boucle des
    /// commandes et celle du relais suivent à la requête suivante.
    channel: Arc<RwLock<Arc<Channel>>>,
    timeout: Duration,
    /// Empreintes épinglées, lues par le vérificateur TLS à chaque poignée de
    /// main ; tenues à jour par l'annonce du serveur sur le canal chiffré.
    pins: Pins,
    /// Dernière annonce du canal chiffré reçue du serveur.
    offer: Arc<Mutex<Option<TlsOffer>>>,
    token: String,
    /// Secret de liaison de cette machine, s'il en a un.
    ///
    /// Partagé entre les copies du client — la boucle de collecte, celle des
    /// commandes et celle du relais en tiennent chacune une — parce que le
    /// serveur peut l'attribuer en cours de route, dans l'accusé de réception
    /// d'un lot, et que les trois doivent le présenter dès l'instant d'après.
    secret: Arc<RwLock<Option<String>>>,
    /// Vrai une fois constaté que le serveur est antérieur à l'en-tête de clé :
    /// la clé repart alors dans l'URL, comme il l'attend.
    legacy_key: Arc<AtomicBool>,
    /// Réveillé à chaque réponse favorable du serveur, quel que soit le canal :
    /// la boucle des mesures en attente de reprise repart aussitôt que le relais
    /// a retrouvé le serveur, et inversement.
    reachable: Arc<Notify>,
    /// Demande d'un lot sans attendre la période : réglage du relais changé
    /// côté serveur, passerelle ouverte ou essayée. Un seul jeton en attente
    /// (`notify_one`) : plusieurs demandes rapprochées donnent un seul lot.
    report_now: Arc<Notify>,
    /// Mandataire sortant vers le serveur, résolu (voir `proxy.rs`) : chaque
    /// client HTTP du canal le reprend, celui du canal chiffré compris — la
    /// session TLS de l'agent passe alors par `CONNECT`, de bout en bout.
    proxy: Arc<ProxyConfig>,
    /// Transport du canal chiffré : le port des agents, ou la WebSocket de
    /// l'interface par un pont local (voir `ws.rs`).
    transport: Arc<Transport>,
    /// Dernière cible du canal chiffré ([`PushClient::use_tls`]) : de quoi le
    /// remonter sur l'autre transport.
    target: Arc<Mutex<Option<TlsTarget>>>,
}

impl PushClient {
    pub fn new(
        server_url: &str,
        token: &str,
        secret: Option<String>,
        timeout: Duration,
    ) -> Result<Self> {
        let proxy = Arc::new(ProxyConfig::default());
        let channel = Channel {
            http: http_client(timeout, None, &proxy)?,
            upgrade: None,
            base_url: server_url.trim_end_matches('/').to_string(),
            encrypted: false,
        };
        Ok(Self {
            channel: Arc::new(RwLock::new(Arc::new(channel))),
            timeout,
            pins: Arc::new(RwLock::new(Vec::new())),
            offer: Arc::new(Mutex::new(None)),
            token: token.to_string(),
            secret: Arc::new(RwLock::new(secret)),
            legacy_key: Arc::new(AtomicBool::new(false)),
            reachable: Arc::new(Notify::new()),
            report_now: Arc::new(Notify::new()),
            proxy,
            transport: Arc::new(Transport::default()),
            target: Arc::new(Mutex::new(None)),
        })
    }

    /// Passe par ce mandataire (déjà résolu) pour joindre le serveur. À appeler
    /// avant [`PushClient::pin_server`] et [`PushClient::use_tls`], qui le
    /// reprennent.
    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Result<Self> {
        self.proxy = Arc::new(proxy);
        let channel = Channel {
            http: http_client(self.timeout, None, &self.proxy)?,
            upgrade: None,
            base_url: self.channel().base_url.clone(),
            encrypted: false,
        };
        if let Ok(mut held) = self.channel.write() {
            *held = Arc::new(channel);
        }
        Ok(self)
    }

    /// Canal courant.
    fn channel(&self) -> Arc<Channel> {
        self.channel
            .read()
            .map(|channel| channel.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
    }

    /// Vrai une fois sur le canal chiffré.
    pub fn is_encrypted(&self) -> bool {
        self.channel().encrypted
    }

    /// Client HTTP du canal courant, avec son certificat client et l'autorité du
    /// serveur épinglée : celui de la sentinelle, pour sonder le serveur dans
    /// les mêmes conditions que les lots de mesures.
    pub fn probe_http(&self) -> reqwest::Client {
        self.channel().http.clone()
    }

    /// Empreintes épinglées en ce moment.
    pub fn pins(&self) -> Vec<String> {
        self.pins.read().map(|pins| pins.clone()).unwrap_or_default()
    }

    /// Remplace les empreintes épinglées.
    pub fn set_pins(&self, pins: Vec<String>) {
        if let Ok(mut held) = self.pins.write() {
            *held = pins;
        }
    }

    /// Dernière annonce du canal chiffré, s'il y en a eu une.
    pub fn offer(&self) -> Option<TlsOffer> {
        self.offer.lock().ok().and_then(|offer| offer.clone())
    }

    /// Pose une annonce comme si le serveur l'avait envoyée (tests).
    #[cfg(test)]
    pub fn set_offer(&self, offer: TlsOffer) {
        if let Ok(mut held) = self.offer.lock() {
            *held = Some(offer);
        }
    }

    /// Règle le transport du canal chiffré (`transport:` de agent.yaml).
    /// `ws_url` : la WebSocket de l'interface (`None` sans adresse utilisable),
    /// ouverte sous la clé épinglée `server_pin` s'il y en a une. À appeler
    /// après [`PushClient::with_proxy`] et avant [`PushClient::use_tls`].
    pub fn set_transport(
        &self,
        mode: TransportMode,
        ws_url: Option<String>,
        server_pin: Option<&str>,
    ) -> Result<()> {
        let connector = match ws_url {
            Some(url) => Some(crate::ws::Connector::new(url, server_pin, &self.proxy)?),
            None => None,
        };
        if mode == TransportMode::WebSocket && connector.is_none() {
            tracing::warn!("transport: websocket needs an https:// server_url, using direct");
        }
        self.transport.configure(mode, connector);
        Ok(())
    }

    /// Reprend le chemin qui a marché la dernière fois (`transport: auto`).
    pub fn remember_route(&self, route: Option<Route>) {
        if let Some(route) = route {
            self.transport.remember(route);
        }
    }

    /// Chemin courant du canal chiffré.
    pub fn route(&self) -> Route {
        self.transport.route()
    }

    /// Ligne de journal du transport : `direct …` ou `websocket wss://…`.
    pub fn describe_transport(&self) -> String {
        self.transport.describe()
    }

    /// Passe au canal chiffré : `url` du port des agents, certificat client
    /// s'il y en a un. Plus aucune requête ne part en clair ensuite.
    pub fn use_tls(&self, url: &str, identity: Option<(Vec<u8>, Vec<u8>)>) -> Result<()> {
        if let Ok(mut target) = self.target.lock() {
            *target = Some((url.to_string(), identity.clone()));
        }
        self.install_tls(self.transport.route(), url, identity)
    }

    /// Monte le canal chiffré sur ce chemin.
    fn install_tls(
        &self,
        route: Route,
        url: &str,
        identity: Option<(Vec<u8>, Vec<u8>)>,
    ) -> Result<()> {
        let (base_url, proxy) = self.dial(route, url)?;
        let config = tls::client_config(self.pins.clone(), identity.clone())?;
        let upgrade = tls::upgrade_config(self.pins.clone(), identity)?;
        let channel = Channel {
            http: http_client(self.timeout, Some(config), &proxy)?,
            upgrade: Some(upgrade_client(self.timeout, upgrade, &proxy)?),
            base_url,
            encrypted: true,
        };
        if let Ok(mut held) = self.channel.write() {
            *held = Arc::new(channel);
        }
        Ok(())
    }

    /// Racine et mandataire du canal chiffré par ce chemin : le port des agents
    /// à travers le mandataire sortant, ou le pont local de la WebSocket, joint
    /// sans mandataire (c'est la WebSocket qui le traverse).
    fn dial(&self, route: Route, url: &str) -> Result<(String, Arc<ProxyConfig>)> {
        match route {
            Route::Direct => Ok((url.trim_end_matches('/').to_string(), self.proxy.clone())),
            Route::WebSocket => {
                let bridge = self.transport.bridge()?;
                let local = ProxyConfig { mode: ProxyMode::Direct, no_proxy: Vec::new() };
                Ok((bridge.base_url(), Arc::new(local)))
            }
        }
    }

    /// `transport: auto` : après `min_failures` échecs de connexion d'affilée
    /// sur le canal chiffré, essaie l'autre chemin (port des agents ou
    /// WebSocket de l'interface) et l'adopte s'il répond. Rend le chemin
    /// adopté ; à l'appelant de le retenir.
    pub async fn reconsider_route(&self, min_failures: u32) -> Option<Route> {
        if !self.transport.can_switch()
            || !self.is_encrypted()
            || self.transport.unreachable() < min_failures.max(1)
        {
            return None;
        }
        let (url, identity) = self.target.lock().ok()?.clone()?;
        let other = self.transport.route().other();
        // L'essai suivant attendra autant d'échecs : pas de rafale d'essais
        // quand le serveur est tout simplement arrêté.
        self.transport.note_reachable();
        if !self.reaches(other, &url).await {
            tracing::debug!(?other, "agent channel: the other transport does not answer either");
            return None;
        }
        if let Err(error) = self.install_tls(other, &url, identity) {
            tracing::warn!(error = %format!("{error:#}"), "agent channel transport not switched");
            return None;
        }
        self.transport.set_route(other);
        Some(other)
    }

    /// Le canal répond-il par ce chemin ? Une poignée de main sans certificat
    /// (l'autorité du serveur toujours vérifiée) et une requête quelconque :
    /// toute réponse HTTP prouve le chemin entier.
    async fn reaches(&self, route: Route, url: &str) -> bool {
        let Ok((base, proxy)) = self.dial(route, url) else { return false };
        let Ok(config) = tls::client_config(self.pins.clone(), None) else { return false };
        let Ok(http) = http_client(PROBE_TIMEOUT, Some(config), &proxy) else { return false };
        http.head(format!("{base}/")).send().await.is_ok()
    }

    /// Ancien canal (port de l'interface) vers un serveur au certificat
    /// autosigné : seule la clé publique épinglée (`server_pin`, posée par la
    /// commande d'installation) est acceptée — ni une autre, ni le clair.
    pub fn pin_server(&self, pin: &str) -> Result<()> {
        let config = tls::pinned_key_config(pin)?;
        let channel = Channel {
            http: http_client(self.timeout, Some(config), &self.proxy)?,
            upgrade: None,
            base_url: self.channel().base_url.clone(),
            encrypted: false,
        };
        if let Ok(mut held) = self.channel.write() {
            *held = Arc::new(channel);
        }
        Ok(())
    }

    /// Lit l'annonce du canal chiffré d'une réponse. Sur le canal chiffré, le
    /// serveur est authentifié : ses empreintes (rotation de l'autorité)
    /// remplacent celles qu'on épinglait. En clair, l'annonce est seulement
    /// gardée, pour la migration.
    fn observe(&self, response: &reqwest::Response, encrypted: bool) {
        let Some(offer) = response
            .headers()
            .get(AGENT_TLS_HEADER)
            .and_then(|value| value.to_str().ok())
            .and_then(TlsOffer::parse)
        else {
            return;
        };
        if encrypted {
            self.set_pins(offer.pins.clone());
        }
        if let Ok(mut held) = self.offer.lock() {
            *held = Some(offer);
        }
    }

    /// Enrôlement sur le canal chiffré : la clé publique de cette machine
    /// contre un certificat client. Le serveur est vérifié par `pins` ; le
    /// jeton (et le secret de liaison s'il y en a un) ne part qu'à lui.
    pub async fn enrol(
        &self,
        url: &str,
        request: &TlsEnrolRequest,
    ) -> Result<TlsCertificate, PushError> {
        let (base, proxy) = self
            .dial(self.transport.route(), url)
            .map_err(|error| PushError::Transport(format!("{error:#}")))?;
        let config = tls::client_config(self.pins.clone(), None)
            .map_err(|error| PushError::Transport(format!("{error:#}")))?;
        let http = http_client(self.timeout, Some(config), &proxy)
            .map_err(|error| PushError::Transport(format!("{error:#}")))?;
        let target = format!("{base}{TLS_ENROL_PATH}");
        let response = self
            .identified(http.post(target).bearer_auth(&self.token))
            .json(request)
            .send()
            .await
            .map_err(|error| self.transport(&error))?;
        self.observe(&response, true);
        self.read_json(response).await
    }

    /// Renouvellement sous le certificat en cours, avec une nouvelle clé.
    pub async fn renew(&self, public_key: String) -> Result<TlsCertificate, PushError> {
        let channel = self.channel();
        let target = format!("{}{TLS_RENEW_PATH}", channel.base_url);
        let response = self
            .identified(channel.http.post(target).bearer_auth(&self.token))
            .json(&TlsRenewRequest { public_key })
            .send()
            .await
            .map_err(|error| self.transport(&error))?;
        self.observe(&response, channel.encrypted);
        self.read_json(response).await
    }

    /// Signal partagé « le serveur répond » : à attendre pendant une temporisation.
    pub fn reachable(&self) -> Arc<Notify> {
        self.reachable.clone()
    }

    /// Note une réponse favorable et réveille qui attendait le serveur.
    fn mark_reachable(&self) {
        self.transport.note_reachable();
        self.reachable.notify_waiters();
    }

    /// Demande à la boucle des mesures d'envoyer un lot sans attendre sa
    /// période.
    pub fn request_report(&self) {
        self.report_now.notify_one();
    }

    /// Signal « un lot est demandé », à attendre dans la boucle des mesures.
    pub fn report_requested(&self) -> Arc<Notify> {
        self.report_now.clone()
    }
}

#[cfg(test)]
mod http_tests;

#[cfg(test)]
mod tests;
