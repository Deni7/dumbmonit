//! Bascule entre serveurs : la primaire, puis les secours de `server_urls`.
//!
//! Un serveur DumbMonit de secours (voir `docs/install/high-availability.md`)
//! partage la base et l'autorité des agents de la primaire : le même
//! certificat client y est reconnu, sans réenrôlement. L'agent n'a donc qu'à
//! changer d'adresse.
//!
//! - Après [`SWITCH_AFTER`] échecs de transport consécutifs (serveur
//!   injoignable, délai dépassé, 5xx — un serveur en attente répond 503),
//!   l'agent passe à l'adresse suivante de la liste, en boucle. La
//!   temporisation habituelle s'applique entre deux tentatives.
//! - Sur un secours, la primaire est réessayée toutes les
//!   [`PRIMARY_RECHECK`] par une requête légère ; dès qu'elle répond (autre
//!   chose qu'un 502, 503 ou 504), l'agent y revient.
//! - L'adresse en service, et depuis quand, part avec chaque lot
//!   ([`ServerLink`]) : la page de la machine le montre.
//!
//! Sans `server_urls`, la liste n'a qu'une adresse et rien de tout cela ne
//! joue.

use std::time::Duration;

use dumbmonit_proto::{MAX_SERVER_LINK_TEXT, ServerLink};
use tokio::time::Instant;

use crate::config::{Config, ServerEndpoint};

/// Échecs de transport consécutifs sur une adresse avant de passer à la
/// suivante. Trois : un redémarrage du serveur (quelques secondes) ne fait
/// pas basculer le parc, une panne franche si, en moins d'une minute.
pub const SWITCH_AFTER: u32 = 3;

/// Période de l'essai de la primaire, tant que l'agent est sur un secours.
pub const PRIMARY_RECHECK: Duration = Duration::from_secs(120);

/// Délai de l'essai de la primaire : court, il retarde le lot qui suit.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// Chemin de l'essai. Sur le port de l'interface, un serveur en attente y
/// répond 503 et un serveur actif 200 ; sur le canal des agents, un serveur
/// actif répond 404 (route inconnue), un serveur en attente n'écoute pas.
pub const PROBE_PATH: &str = "/api/ha/status";

/// État de la bascule.
#[derive(Debug)]
pub struct Failover {
    /// Serveurs dans l'ordre : la primaire d'abord.
    servers: Vec<ServerEndpoint>,
    /// Rang de l'adresse en service.
    active: usize,
    /// Échecs de transport consécutifs sur l'adresse en service.
    failures: u32,
    /// Passage sur un secours, en millisecondes depuis l'époque Unix.
    since_ms: Option<i64>,
    /// Dernière erreur constatée sur la primaire.
    primary_error: Option<String>,
    /// Prochain essai de la primaire, sur un secours.
    next_primary_check: Option<Instant>,
    /// Vrai juste après une bascule : la boucle tente aussitôt la nouvelle
    /// adresse au lieu d'attendre sa temporisation.
    switched: bool,
}

impl Failover {
    pub fn new(config: &Config) -> Self {
        let primary =
            ServerEndpoint { url: config.server_url.clone(), tls_url: config.tls_url.clone() };
        let mut servers = vec![primary];
        servers.extend(config.fallback_servers.iter().cloned());
        Self {
            servers,
            active: 0,
            failures: 0,
            since_ms: None,
            primary_error: None,
            next_primary_check: None,
            switched: false,
        }
    }

    /// Vrai si la configuration donne au moins un secours.
    pub fn enabled(&self) -> bool {
        self.servers.len() > 1
    }

    /// Rang de l'adresse en service (0 : la primaire).
    pub fn active(&self) -> usize {
        self.active
    }

    /// Adresse de l'interface du serveur de rang `index`.
    pub fn web_url(&self, index: usize) -> Option<&str> {
        self.servers.get(index).map(|server| server.url.as_str())
    }

    /// Canal chiffré du serveur de rang `index`. La primaire garde le sien
    /// (`primary_channel`, celui de l'état rangé) ; un secours a le sien s'il
    /// est donné, sinon le même port que la primaire, sur son propre hôte.
    pub fn channel_url(&self, index: usize, primary_channel: &str) -> Option<String> {
        if index == 0 {
            return Some(primary_channel.to_string());
        }
        let server = self.servers.get(index)?;
        if let Some(url) = &server.tls_url {
            return Some(url.clone());
        }
        let port = port_of(primary_channel).unwrap_or(dumbmonit_proto::DEFAULT_AGENT_TLS_PORT);
        dumbmonit_proto::agent_channel_url(&server.url, port)
    }

    /// Canal chiffré de l'adresse en service ; `primary_channel` à défaut.
    pub fn active_channel(&self, primary_channel: &str) -> String {
        self.channel_url(self.active, primary_channel)
            .unwrap_or_else(|| primary_channel.to_string())
    }

    /// Réponse favorable de l'adresse en service.
    pub fn succeeded(&mut self) {
        self.failures = 0;
        self.switched = false;
    }

    /// Échec de transport sur l'adresse en service. Rend le rang de
    /// l'adresse suivante quand il est temps d'y passer ; l'appelant y
    /// bascule puis appelle [`Failover::switched_to`].
    pub fn failed(&mut self, error: &str) -> Option<usize> {
        if !self.enabled() {
            return None;
        }
        if self.active == 0 {
            self.primary_error = Some(clip(error));
        }
        self.failures = self.failures.saturating_add(1);
        if self.failures < SWITCH_AFTER {
            return None;
        }
        Some((self.active + 1) % self.servers.len())
    }

    /// L'agent compose désormais l'adresse de rang `index`.
    pub fn switched_to(&mut self, index: usize, now: Instant) {
        self.failures = 0;
        self.switched = true;
        self.active = index;
        if index == 0 {
            self.back_to_primary();
            return;
        }
        if self.since_ms.is_none() {
            self.since_ms = Some(chrono::Utc::now().timestamp_millis());
        }
        self.next_primary_check = Some(now + PRIMARY_RECHECK);
    }

    /// Vrai une fois, juste après une bascule.
    pub fn take_switched(&mut self) -> bool {
        std::mem::take(&mut self.switched)
    }

    /// Faut-il réessayer la primaire ?
    pub fn primary_due(&self, now: Instant) -> bool {
        self.active != 0 && self.next_primary_check.is_some_and(|at| now >= at)
    }

    /// La primaire ne répond toujours pas : prochain essai plus tard.
    pub fn primary_unreachable(&mut self, error: &str, now: Instant) {
        self.primary_error = Some(clip(error));
        self.next_primary_check = Some(now + PRIMARY_RECHECK);
    }

    /// Retour sur la primaire.
    pub fn back_to_primary(&mut self) {
        self.active = 0;
        self.failures = 0;
        self.since_ms = None;
        self.primary_error = None;
        self.next_primary_check = None;
    }

    /// Essai de la primaire dès le prochain lot (tests).
    #[cfg(test)]
    pub fn recheck_now(&mut self) {
        self.next_primary_check = Some(Instant::now());
    }

    /// Ce que le lot dit de l'adresse en service ; `None` sans secours.
    pub fn link(&self, url: &str) -> Option<ServerLink> {
        if !self.enabled() {
            return None;
        }
        let secondary = self.active != 0;
        Some(ServerLink {
            active: u32::try_from(self.active).unwrap_or(u32::MAX),
            servers: u32::try_from(self.servers.len()).unwrap_or(u32::MAX),
            url: clip(url),
            since_ms: self.since_ms.filter(|_| secondary),
            primary_error: self.primary_error.clone().filter(|_| secondary),
        })
    }
}

/// Verdict de l'essai de la primaire : `Ok` si elle répond en serveur
/// actif. Une réponse 502, 503 ou 504 vient d'un serveur en attente ou d'un
/// mandataire sans serveur derrière lui : la primaire n'est pas revenue.
pub fn probe_verdict(outcome: Result<u16, String>) -> Result<(), String> {
    match outcome {
        Ok(status @ (502..=504)) => Err(format!("HTTP {status}: not serving")),
        Ok(_) => Ok(()),
        Err(error) => Err(error),
    }
}

/// Port d'une URL `https://hôte[:port]` ; 443 sans port explicite.
fn port_of(url: &str) -> Option<u16> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split('/').next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    // `[::1]:8443` : le port suit le crochet fermant.
    let after_host = match authority.rfind(']') {
        Some(end) => &authority[end + 1..],
        None => authority,
    };
    match after_host.rsplit_once(':') {
        Some((_, port)) => port.parse::<u16>().ok(),
        None => Some(443),
    }
}

/// Texte borné, tel qu'il part dans le lot.
fn clip(text: &str) -> String {
    text.chars().take(MAX_SERVER_LINK_TEXT).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failover(fallbacks: &[&str]) -> Failover {
        Failover {
            servers: std::iter::once("https://a:8080")
                .chain(fallbacks.iter().copied())
                .map(|url| ServerEndpoint { url: url.into(), tls_url: None })
                .collect(),
            active: 0,
            failures: 0,
            since_ms: None,
            primary_error: None,
            next_primary_check: None,
            switched: false,
        }
    }

    #[test]
    fn a_single_server_never_switches_and_reports_nothing() {
        let mut single = failover(&[]);
        for _ in 0..10 {
            assert_eq!(single.failed("cannot connect"), None);
        }
        assert!(single.link("https://a:8443").is_none());
        assert!(!single.primary_due(Instant::now() + Duration::from_secs(3600)));
    }

    #[test]
    fn the_next_server_is_chosen_after_consecutive_failures_only() {
        let mut state = failover(&["https://b:8080", "https://c:8080"]);
        assert_eq!(state.failed("cannot connect"), None);
        // Une réponse entre deux échecs remet le compte à zéro.
        state.succeeded();
        assert_eq!(state.failed("cannot connect"), None);
        assert_eq!(state.failed("cannot connect"), None);
        assert_eq!(state.failed("cannot connect"), Some(1));
        let now = Instant::now();
        state.switched_to(1, now);
        assert!(state.take_switched());
        assert!(!state.take_switched(), "une seule fois");
        assert_eq!(state.active(), 1);
        // Les secours se suivent, puis la liste reprend à la primaire.
        for _ in 1..SWITCH_AFTER {
            assert_eq!(state.failed("timed out"), None);
        }
        assert_eq!(state.failed("timed out"), Some(2));
        state.switched_to(2, now);
        for _ in 1..SWITCH_AFTER {
            state.failed("timed out");
        }
        assert_eq!(state.failed("timed out"), Some(0));
        state.switched_to(0, now);
        assert_eq!(state.active(), 0);
        assert!(state.link("https://a:8443").unwrap().since_ms.is_none());
    }

    #[test]
    fn on_a_backup_the_primary_is_tried_again_periodically() {
        let mut state = failover(&["https://b:8080"]);
        let now = Instant::now();
        for _ in 0..SWITCH_AFTER {
            state.failed("cannot connect: Connection refused");
        }
        state.switched_to(1, now);
        assert!(!state.primary_due(now));
        assert!(state.primary_due(now + PRIMARY_RECHECK));
        state.primary_unreachable("HTTP 503: not serving", now + PRIMARY_RECHECK);
        assert!(!state.primary_due(now + PRIMARY_RECHECK));
        assert!(state.primary_due(now + PRIMARY_RECHECK * 2));

        let link = state.link("https://b:8443").expect("lien");
        assert_eq!((link.active, link.servers), (1, 2));
        assert_eq!(link.url, "https://b:8443");
        assert!(link.since_ms.is_some());
        assert_eq!(link.primary_error.as_deref(), Some("HTTP 503: not serving"));

        state.back_to_primary();
        let link = state.link("https://a:8443").expect("lien");
        assert_eq!(link.active, 0);
        assert_eq!(link.primary_error, None);
        assert!(!state.primary_due(now + PRIMARY_RECHECK * 10));
    }

    #[test]
    fn a_backup_channel_follows_the_port_of_the_primary_channel() {
        let mut state = failover(&["https://b:8080", "https://[fd00::2]:8080"]);
        state.servers.push(ServerEndpoint {
            url: "https://c:8080".into(),
            tls_url: Some("https://agents-c:9443".into()),
        });
        assert_eq!(state.channel_url(0, "https://a:8443").as_deref(), Some("https://a:8443"));
        assert_eq!(state.channel_url(1, "https://a:9443").as_deref(), Some("https://b:9443"));
        assert_eq!(
            state.channel_url(2, "https://agents.example.com").as_deref(),
            Some("https://[fd00::2]:443")
        );
        let explicit = state.channel_url(3, "https://a:8443");
        assert_eq!(explicit.as_deref(), Some("https://agents-c:9443"));
        assert_eq!(state.channel_url(4, "https://a:8443"), None);
        assert_eq!(state.web_url(1), Some("https://b:8080"));
    }

    #[test]
    fn only_an_active_server_brings_the_agent_back() {
        assert_eq!(probe_verdict(Ok(200)), Ok(()));
        // Le canal des agents d'un serveur actif ne connaît pas la route.
        assert_eq!(probe_verdict(Ok(404)), Ok(()));
        assert!(probe_verdict(Ok(503)).is_err(), "serveur en attente");
        assert!(probe_verdict(Ok(502)).is_err(), "mandataire sans serveur");
        assert!(probe_verdict(Err("cannot connect".into())).is_err());
    }

    #[test]
    fn ports_are_read_from_channel_urls() {
        assert_eq!(port_of("https://a:8443"), Some(8443));
        assert_eq!(port_of("https://a"), Some(443));
        assert_eq!(port_of("https://[::1]:9443"), Some(9443));
        assert_eq!(port_of("https://[::1]"), Some(443));
        assert_eq!(port_of("https://a:pas-un-port"), None);
    }
}
