//! Passerelle d'un relais vue du serveur : sur quel port les machines du site
//! la composent, et pourquoi elle ne les accepte pas (encore).
//!
//! L'écran d'ajout n'affiche la commande « à travers ce relais » qu'une fois
//! la passerelle à l'écoute et essayée par le relais lui-même. Sans ce module,
//! une passerelle fermée, impossible à ouvrir (port pris) ou limitée à la
//! boucle locale le faisait attendre en silence ; chaque cause a ici un nom,
//! que l'interface traduit en explication et en correctif.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Context, Result};
use dumbmonit_proto::{DEFAULT_RELAY_GATEWAY_PORT, TargetId, Version};
use serde::Serialize;
use sqlx::SqlitePool;

use super::store::{self, HostInfo};

/// Première version de l'agent dont le relais ouvre une passerelle.
pub const MIN_GATEWAY_AGENT: &str = "0.14.2";

/// Ce qui empêche la passerelle d'un relais d'accepter les machines du site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayProblem {
    /// Fermée depuis l'interface (panneau Relais de la machine).
    Closed,
    /// Binaire antérieur à la passerelle ([`MIN_GATEWAY_AGENT`]).
    TooOld,
    /// Le relais ne rapporte plus rien : il ne peut ni ouvrir ni porter quoi
    /// que ce soit.
    Offline,
    /// Le relais n'a pas pu ouvrir le port (pris, droits, adresse absente).
    BindFailed,
    /// Il n'écoute que sur la boucle locale (`gateway_listen: 127.0.0.1:…`) :
    /// aucune machine du site ne le joint.
    Loopback,
    /// Il écoute, mais ne parle pas au serveur sur le canal chiffré : pas de
    /// tunnel, la passerelle ne mène nulle part.
    NoTunnel,
}

/// Port que les machines du site composent : celui que l'interface demande,
/// sauf quand l'agent.yaml du relais en impose un (`gateway_listen`), qui
/// prime — c'est alors celui sur lequel il écoute vraiment.
pub fn dialed_port(info: &HostInfo) -> u16 {
    let local = info.relay_gateway_state.as_ref().filter(|state| state.local);
    let wanted = info.relay_gateway_wanted;
    let real = info.relay_gateway_port;
    let port = match local {
        Some(state) => {
            real.or_else(|| state.wanted.parse::<SocketAddr>().ok().map(|addr| addr.port()))
        }
        None => wanted.or(real),
    };
    port.filter(|port| *port > 0).unwrap_or(DEFAULT_RELAY_GATEWAY_PORT)
}

/// Vrai si `version` est antérieure à la passerelle. Une version illisible
/// (binaire de développement) n'est pas déclarée trop ancienne.
fn too_old(version: &str) -> bool {
    match (Version::parse(version), Version::parse(MIN_GATEWAY_AGENT)) {
        (Some(version), Some(minimum)) => version < minimum,
        _ => false,
    }
}

fn is_loopback(address: &str) -> bool {
    address.parse::<SocketAddr>().is_ok_and(|addr| addr.ip().is_loopback())
}

/// Ce qui bloque la passerelle, du plus fondamental au plus fin ; `None` :
/// rien de connu, elle s'ouvre ou s'essaie (voir `listening`, `reachable`).
/// `offline` : le relais est muet (voir [`offline`]).
pub fn problem(info: &HostInfo, offline: bool) -> Option<GatewayProblem> {
    let state = info.relay_gateway_state.as_ref();
    if info.relay_gateway_port.is_none() && state.is_none() {
        // Rien d'ouvert ni de tenté : il ne sait pas, ou on ne le lui demande pas.
        if too_old(&info.agent_version) {
            return Some(GatewayProblem::TooOld);
        }
        if info.relay_gateway_wanted.is_none() {
            return Some(GatewayProblem::Closed);
        }
    }
    if offline {
        return Some(GatewayProblem::Offline);
    }
    let state = state?;
    match state.listen.as_deref() {
        None if state.error.is_some() => Some(GatewayProblem::BindFailed),
        Some(listen) if is_loopback(listen) => Some(GatewayProblem::Loopback),
        Some(_) if !state.encrypted => Some(GatewayProblem::NoTunnel),
        _ => None,
    }
}

/// Délai de silence au-delà duquel un relais est tenu pour hors ligne : le
/// même que celui qui le déclare injoignable (trois périodes, 90 s au moins).
fn silence_limit(interval_secs: i64) -> Duration {
    let interval = Duration::from_secs(u64::try_from(interval_secs).unwrap_or(0));
    super::staleness_deadline(interval)
}

/// Vrai si le relais n'a rien envoyé depuis plus que sa tolérance, ou jamais.
pub fn silent(last_seen_ms: Option<i64>, now_ms: i64, interval_secs: i64) -> bool {
    let limit = silence_limit(interval_secs).as_millis() as i64;
    last_seen_ms.is_none_or(|seen| now_ms.saturating_sub(seen) > limit)
}

/// Le relais `target` est-il muet en ce moment ?
pub async fn offline(pool: &SqlitePool, target: TargetId) -> Result<bool> {
    let interval: Option<i64> =
        sqlx::query_scalar("SELECT interval_secs FROM targets WHERE id = ?")
            .bind(target)
            .fetch_optional(pool)
            .await
            .context("reading the relay's period")?;
    let seen = store::last_seen_ms(pool, target).await?;
    Ok(silent(seen, chrono::Utc::now().timestamp_millis(), interval.unwrap_or(60)))
}

#[cfg(test)]
mod tests {
    use dumbmonit_proto::GatewayState;

    use super::*;

    fn relay() -> HostInfo {
        HostInfo {
            hostname: "relais".into(),
            os: "linux".into(),
            os_version: None,
            arch: None,
            agent_version: "0.20.0".into(),
            commands_enabled: None,
            relay: true,
            site: Some("Lyon".into()),
            relay_wanted: true,
            relay_wanted_site: None,
            relay_control: true,
            relay_local: false,
            relay_gateway_wanted: Some(8443),
            relay_gateway_port: None,
            relay_gateway_host: None,
            relay_gateway_check: None,
            relay_gateway_state: None,
            server_link: None,
            last_seen_at: None,
            bound: true,
            bound_at: None,
            binding_supported: true,
            rebind_until: None,
        }
    }

    fn state(listen: Option<&str>, error: Option<&str>) -> Option<GatewayState> {
        Some(GatewayState {
            wanted: "0.0.0.0:8443".into(),
            listen: listen.map(str::to_string),
            error: error.map(str::to_string),
            local: false,
            encrypted: true,
        })
    }

    #[test]
    fn a_gateway_that_listens_everywhere_through_the_tunnel_has_no_problem() {
        let mut info = relay();
        info.relay_gateway_port = Some(8443);
        info.relay_gateway_state = state(Some("0.0.0.0:8443"), None);
        assert_eq!(problem(&info, false), None);
        // Un relais antérieur au rapport d'état : seul le port compte.
        info.relay_gateway_state = None;
        assert_eq!(problem(&info, false), None);
        // Il s'ouvre : rien de bloquant, l'interface attend.
        let opening = relay();
        assert_eq!(problem(&opening, false), None);
    }

    #[test]
    fn a_gateway_closed_from_the_interface_says_so() {
        let mut info = relay();
        info.relay_gateway_wanted = None;
        assert_eq!(problem(&info, false), Some(GatewayProblem::Closed));
        // … sauf si l'agent.yaml l'ouvre quand même.
        info.relay_gateway_port = Some(9443);
        assert_eq!(problem(&info, false), None);
    }

    #[test]
    fn a_relay_older_than_the_gateway_is_told_to_update() {
        let mut info = relay();
        info.agent_version = "0.13.6".into();
        assert_eq!(problem(&info, false), Some(GatewayProblem::TooOld));
        info.agent_version = "0.14.2".into();
        assert_eq!(problem(&info, false), None);
        info.agent_version = "dev".into();
        assert_eq!(problem(&info, false), None, "illisible : pas de faux reproche");
    }

    #[test]
    fn a_bind_failure_a_loopback_listener_or_no_tunnel_is_named() {
        let mut info = relay();
        info.relay_gateway_state = state(None, Some("the port is already used"));
        assert_eq!(problem(&info, false), Some(GatewayProblem::BindFailed));

        info.relay_gateway_port = Some(8443);
        info.relay_gateway_state = state(Some("127.0.0.1:8443"), None);
        assert_eq!(problem(&info, false), Some(GatewayProblem::Loopback));
        info.relay_gateway_state = state(Some("[::1]:8443"), None);
        assert_eq!(problem(&info, false), Some(GatewayProblem::Loopback));

        info.relay_gateway_state = state(Some("0.0.0.0:8443"), None);
        info.relay_gateway_state.as_mut().unwrap().encrypted = false;
        assert_eq!(problem(&info, false), Some(GatewayProblem::NoTunnel));
    }

    #[test]
    fn a_silent_relay_is_offline_whatever_it_last_said() {
        let mut info = relay();
        info.relay_gateway_port = Some(8443);
        info.relay_gateway_state = state(Some("0.0.0.0:8443"), None);
        assert_eq!(problem(&info, true), Some(GatewayProblem::Offline));
        // Une passerelle fermée reste le premier correctif à faire.
        let mut closed = relay();
        closed.relay_gateway_wanted = None;
        assert_eq!(problem(&closed, true), Some(GatewayProblem::Closed));
    }

    #[test]
    fn the_site_dials_the_port_the_relay_really_listens_on_when_its_file_imposes_one() {
        let mut info = relay();
        assert_eq!(dialed_port(&info), 8443);
        info.relay_gateway_wanted = Some(9443);
        info.relay_gateway_port = Some(8443);
        assert_eq!(dialed_port(&info), 9443, "réglé dans l'interface : il s'y déplace");

        let mut local = state(Some("0.0.0.0:7443"), None).unwrap();
        local.local = true;
        info.relay_gateway_state = Some(local.clone());
        info.relay_gateway_port = Some(7443);
        assert_eq!(dialed_port(&info), 7443, "gateway_listen prime");

        local.wanted = "0.0.0.0:6443".into();
        local.listen = None;
        info.relay_gateway_state = Some(local);
        info.relay_gateway_port = None;
        assert_eq!(dialed_port(&info), 6443, "même quand l'ouverture échoue");

        let mut closed = relay();
        closed.relay_gateway_wanted = None;
        assert_eq!(dialed_port(&closed), DEFAULT_RELAY_GATEWAY_PORT);
    }

    #[test]
    fn a_relay_is_offline_after_three_periods_and_at_least_ninety_seconds() {
        let now = 1_700_000_000_000;
        assert!(silent(None, now, 30), "jamais vu");
        assert!(!silent(Some(now - 60_000), now, 30));
        assert!(silent(Some(now - 91_000), now, 30));
        assert!(!silent(Some(now - 600_000), now, 3600), "longue période : longue patience");
    }
}
