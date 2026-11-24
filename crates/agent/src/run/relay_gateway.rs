//! Relais et passerelle de l'agent : réglages du serveur, démarrage, état.

use super::startup::{disk_limit, spool_dir};
use super::*;

impl Agent {
    /// Vrai si l'agent doit relayer : `relay: true` en local, qui prime, ou le
    /// réglage de l'interface.
    pub(super) fn relay_wanted(&self) -> bool {
        self.config.relay || self.server_relay.as_ref().is_some_and(|r| r.enabled)
    }

    /// Site annoncé : celui de la configuration locale s'il y en a un, sinon
    /// celui réglé dans l'interface.
    pub(super) fn relay_site(&self) -> Option<String> {
        self.config.site.clone().or_else(|| {
            self.server_relay
                .as_ref()
                .and_then(|r| r.site.as_deref())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
    }

    /// Adopte le mode relais que le serveur vient de renvoyer, et le reporte
    /// dans l'identité : le lot suivant dit au serveur ce qui est en vigueur.
    pub(super) fn adopt_relay(&mut self, setting: Option<RelaySetting>) {
        // Un serveur antérieur ne dit rien : on garde ce qu'on avait.
        let Some(setting) = setting else { return };
        if self.server_relay.as_ref() != Some(&setting) {
            if self.config.relay && !setting.enabled {
                debug!("relay switched off from the server, but relay: true in agent.yaml wins");
            }
            self.server_relay = Some(setting);
        }
        self.identity.relay = self.relay_wanted();
        self.identity.site = self.relay_site();
    }

    /// Lance ou arrête la boucle du relais selon le réglage en vigueur, à chaud.
    pub(super) fn sync_relay(&mut self, shutdown: &Shutdown) {
        // Une boucle qui a rendu la main (arrêt) ne compte plus.
        if self.relay.as_ref().is_some_and(|handle| handle.is_finished()) {
            self.relay = None;
        }
        match (self.relay_wanted(), self.relay.is_some()) {
            (true, false) => {
                if !self.config.relay {
                    info!(site = ?self.relay_site(), "relay mode switched on from the server");
                }
                self.start_relay(shutdown);
            }
            (false, true) => {
                // L'attente longue est simplement abandonnée ; une sonde déjà
                // confiée finit dans sa propre tâche et rend compte quand même.
                if let Some(handle) = self.relay.take() {
                    handle.abort();
                }
                // Plus relais : ni planning (il porte les identifiants des
                // équipements), ni résultats en attente ne restent sur le disque.
                self.plan_store().forget();
                if let Some(backlog) = self.relay_backlog.take() {
                    backlog.clear();
                }
                self.buffer.set_disk_limit(disk_limit(self.config.buffer_max_mb));
                info!("relay mode switched off from the server");
            }
            _ => {}
        }
    }

    /// Lance la boucle du relais, avec de quoi tenir une coupure du lien
    /// quand le tampon disque est actif.
    pub(super) fn start_relay(&mut self, shutdown: &Shutdown) {
        let mut runner =
            RelayRunner::new(self.client.clone(), self.identity.key(), RELAY_REQUEST_TIMEOUT);
        if let Some(backlog) = self.open_relay_backlog() {
            runner = runner.with_offline(Offline { store: self.plan_store(), backlog });
        }
        self.relay = Some(tokio::spawn(runner.run(shutdown.clone())));
    }

    /// Planning du relais, à côté du secret de liaison.
    pub(super) fn plan_store(&self) -> PlanStore {
        PlanStore::beside(&self.config.secret_path)
    }

    /// Tampon des résultats du relais : les trois quarts de la borne du
    /// disque, le reste pour les mesures de la machine elle-même.
    pub(super) fn open_relay_backlog(&mut self) -> Option<Backlog> {
        if self.config.buffer_max_mb == 0 {
            return None;
        }
        if self.relay_backlog.is_none() {
            let total = disk_limit(self.config.buffer_max_mb);
            let dir = spool_dir(&self.config.secret_path).join("relay");
            match Backlog::open(&dir, total / 4 * 3) {
                Ok(backlog) => self.relay_backlog = Some(backlog),
                Err(error) => {
                    warn!(
                        path = %dir.display(),
                        %error,
                        "cannot open the relay buffer: results probed during an outage will be lost"
                    );
                    return None;
                }
            }
            self.buffer.set_disk_limit(total / 4);
        }
        self.relay_backlog.clone()
    }

    /// Adresse où la passerelle doit écouter : `gateway_listen` en local, qui
    /// prime, sinon le port réglé dans l'interface — et seulement en mode relais.
    pub(super) fn gateway_wanted(&self) -> Option<std::net::SocketAddr> {
        if !self.relay_wanted() {
            return None;
        }
        self.config.gateway.listen.or_else(|| {
            let port = self.server_relay.as_ref()?.gateway_port?;
            Some(std::net::SocketAddr::from(([0, 0, 0, 0], port)))
        })
    }

    /// Ouvre, déplace ou ferme la passerelle selon le réglage en vigueur, à
    /// chaud, et reporte dans l'identité le port sur lequel elle écoute.
    pub(super) async fn sync_gateway(&mut self, shutdown: &Shutdown) {
        if self.gateway.as_ref().is_some_and(|gateway| gateway.handle.is_finished()) {
            self.gateway = None;
        }
        let wanted = self.gateway_wanted();
        if self.gateway.as_ref().map(|gateway| gateway.addr) != wanted {
            if let Some(gateway) = self.gateway.take() {
                gateway.handle.abort();
                info!(listen = %gateway.addr, "relay gateway closed");
            }
            if let Some(addr) = wanted {
                match crate::gateway::bind(addr).await {
                    Ok(listener) => {
                        // L'adresse vraiment liée, pas celle demandée : c'est elle
                        // que le serveur montre, et qui trahit une écoute limitée
                        // à la boucle locale.
                        let bound = listener.local_addr().unwrap_or(addr);
                        if bound.ip().is_loopback() {
                            warn!(
                                listen = %bound,
                                "relay gateway listens on the loopback only: the site's machines \
                                 cannot reach it (gateway_listen)"
                            );
                        }
                        let upstream = std::sync::Arc::new(crate::gateway::ServerUpstream::new(
                            self.client.clone(),
                        ));
                        let handle = tokio::spawn(crate::gateway::serve(
                            listener,
                            upstream,
                            self.config.gateway.clone(),
                            shutdown.clone(),
                        ));
                        self.gateway = Some(GatewayTask { addr, bound, handle });
                        self.gateway_failed = None;
                    }
                    Err(error) => {
                        let cause = crate::gateway::describe_bind_error(&error);
                        let again = self.gateway_failed.as_ref().is_some_and(|(at, _)| *at == addr);
                        if again {
                            debug!(listen = %addr, %error, "relay gateway still cannot listen");
                        } else {
                            warn!(listen = %addr, %error, "relay gateway cannot listen");
                        }
                        self.gateway_failed = Some((addr, cause));
                    }
                }
            }
        }
        let port = self.gateway.as_ref().map(|gateway| gateway.bound.port());
        let state = self.gateway_state(wanted);
        if port != self.identity.relay_gateway || state != self.identity.relay_gateway_state {
            // Passerelle ouverte, déplacée, fermée ou impossible à ouvrir : le
            // serveur (et l'écran d'ajout qui attend qu'elle écoute) l'apprend
            // par un lot immédiat, pas à la période suivante.
            self.client.request_report();
        }
        self.identity.relay_gateway = port;
        self.identity.relay_gateway_state = state;
        self.sync_gateway_check();
    }

    /// Ce que la passerelle est vraiment, pour le serveur : l'adresse liée, ou
    /// la cause de l'échec à l'adresse demandée. `None` sans passerelle demandée.
    pub(super) fn gateway_state(
        &self,
        wanted: Option<std::net::SocketAddr>,
    ) -> Option<dumbmonit_proto::GatewayState> {
        let failed = self
            .gateway_failed
            .as_ref()
            .filter(|(addr, _)| Some(*addr) == wanted)
            .map(|(_, cause)| cause.as_str());
        crate::gateway::state(
            wanted,
            self.config.gateway.listen.is_some(),
            self.gateway.as_ref().map(|gateway| gateway.bound),
            failed,
            self.client.is_encrypted(),
        )
    }

    /// Essaie la passerelle à l'adresse que le site compose, comme le ferait
    /// une de ses machines : quand elle s'ouvre, quand l'adresse change, puis
    /// toutes les heures. Le résultat part avec un lot envoyé aussitôt.
    pub(super) fn sync_gateway_check(&mut self) {
        if self.gateway_check_task.as_ref().is_some_and(|task| task.is_finished()) {
            self.gateway_check_task = None;
        }
        let Some(port) = self.gateway.as_ref().map(|gateway| gateway.bound.port()) else {
            if let Some(task) = self.gateway_check_task.take() {
                task.abort();
            }
            self.gateway_checked = None;
            if let Ok(mut held) = self.gateway_check.lock() {
                *held = None;
            }
            self.identity.relay_gateway_check = None;
            return;
        };
        let host = self.server_relay.as_ref().and_then(|relay| relay.gateway_host.as_deref());
        let address = crate::gateway::check_address(host, port);
        // Un essai raté se refait chaque minute : l'écran d'ajout attend de voir
        // la correction (port publié, adresse) sans patienter une heure.
        let failed = self
            .gateway_check
            .lock()
            .ok()
            .and_then(|held| held.as_ref().map(|check| !check.ok))
            .unwrap_or(false);
        let period = if failed { GATEWAY_RETRY } else { GATEWAY_RECHECK };
        let due = match &self.gateway_checked {
            Some((last, at)) => *last != address || at.elapsed() >= period,
            None => true,
        };
        // Sans canal chiffré, pas de tunnel : rien à essayer encore.
        let pins = self.client.pins();
        if !due
            || self.gateway_check_task.is_some()
            || !self.client.is_encrypted()
            || pins.is_empty()
        {
            return;
        }
        self.gateway_checked = Some((address.clone(), Instant::now()));
        let slot = self.gateway_check.clone();
        let client = self.client.clone();
        self.gateway_check_task = Some(tokio::spawn(async move {
            let check = crate::gateway::self_check(&address, pins).await;
            if let Ok(mut held) = slot.lock() {
                *held = Some(check);
            }
            client.request_report();
        }));
    }
}
