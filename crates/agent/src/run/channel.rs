//! Canal chiffré (enrôlement, certificat) et choix du transport et du serveur.

use super::*;

impl Agent {
    /// Vrai si rien ne doit plus partir en clair : empreinte donnée par la
    /// commande d'installation, ou agent déjà enrôlé une fois.
    pub(super) fn tls_only(&self) -> bool {
        self.config.ca_pin.is_some() || self.tls.as_ref().is_some_and(|state| state.locked)
    }

    /// Met le canal chiffré en place, ou le maintient : migration depuis
    /// l'ancien canal sur l'annonce du serveur, enrôlement, renouvellement,
    /// empreintes tenues à jour.
    pub(super) async fn ensure_tls(&mut self) {
        if self.tls.is_none() {
            // Un serveur à jour annonce son canal chiffré sur l'ancien : un
            // agent installé avant migre de lui-même, en épinglant l'autorité
            // annoncée (confiance au premier usage).
            let Some(offer) = self.client.offer() else { return };
            // L'adresse dédiée aux agents que le serveur annonce prime sur
            // l'hôte de `server_url` ; celle de la configuration, sur les deux.
            let url = self.config.tls_url.clone().or_else(|| offer.url.clone()).or_else(|| {
                dumbmonit_proto::agent_channel_url(&self.config.server_url, offer.port)
            });
            let Some(url) = url else { return };
            info!(url, "the server offers an encrypted agent channel, enrolling");
            self.client.set_pins(offer.pins.clone());
            self.tls = Some(TlsState::new(url, offer.pins));
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        if self.client.is_encrypted() {
            self.adopt_pins();
        }
        if self.tls_retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        let Some(state) = self.tls.clone() else { return };
        if state.usable(now_ms) {
            if state.needs_renewal(now_ms) {
                self.renew_certificate().await;
            }
            return;
        }
        self.enrol_tls(state).await;
    }

    /// Range les empreintes que le serveur vient d'annoncer sur le canal
    /// chiffré (rotation de l'autorité).
    pub(super) fn adopt_pins(&mut self) {
        let pins = self.client.pins();
        let Some(state) = self.tls.as_mut() else { return };
        if pins.is_empty() || pins == state.pins {
            return;
        }
        info!(pins = pins.join(","), "pinned server authorities updated");
        state.pins = pins;
        if state.locked
            && let Err(error) = state.save(&self.tls_path)
        {
            warn!(error = %format!("{error:#}"), "cannot store the encrypted channel state");
        }
    }

    /// Enrôlement : une clé neuve, le jeton (et le secret de liaison), un
    /// certificat en retour.
    pub(super) async fn enrol_tls(&mut self, mut state: TlsState) {
        let (public_key, key) = match crate::tls::new_key() {
            Ok(pair) => pair,
            Err(error) => {
                warn!(error = %format!("{error:#}"), "cannot generate the agent key");
                self.tls_retry_at = Some(Instant::now() + TLS_RETRY);
                return;
            }
        };
        let request =
            dumbmonit_proto::TlsEnrolRequest { identity: self.identity.clone(), public_key };
        // Sur le serveur en service : un secours partage l'autorité et la base.
        let url = self.failover.active_channel(&state.url);
        match self.client.enrol(&url, &request).await {
            Ok(issued) => {
                if let Some(secret) = issued.agent_secret.clone() {
                    self.adopt_binding(Some(secret), true);
                }
                self.note_approved();
                self.adopt_certificate(&mut state, issued, &key);
                info!(url = state.url, "enrolled on the encrypted agent channel (TLS 1.3, mTLS)");
            }
            // Validation attendue : rien d'anormal, l'agent réessaie à son
            // rythme sans le signaler comme une panne.
            Err(PushError::Forbidden(message)) if message == dumbmonit_proto::ENROLMENT_PENDING => {
                self.note_awaiting_approval();
                self.tls_retry_at = Some(Instant::now() + TLS_RETRY);
            }
            Err(error) => {
                if let PushError::Forbidden(message) = &error {
                    self.on_binding_refused(message);
                }
                if matches!(error, PushError::Transport(_)) {
                    self.note_unreachable(&error);
                }
                self.note_tls_failure("enrolment on the encrypted agent channel failed", &error);
            }
        }
    }

    /// Renouvellement sous le certificat en cours, avec une clé neuve.
    pub(super) async fn renew_certificate(&mut self) {
        let Some(mut state) = self.tls.clone() else { return };
        let (public_key, key) = match crate::tls::new_key() {
            Ok(pair) => pair,
            Err(error) => {
                warn!(error = %format!("{error:#}"), "cannot generate the agent key");
                self.tls_retry_at = Some(Instant::now() + TLS_RETRY);
                return;
            }
        };
        match self.client.renew(public_key).await {
            Ok(issued) => {
                self.adopt_certificate(&mut state, issued, &key);
                info!("agent certificate renewed");
            }
            Err(PushError::CertificateRefused) => self.on_certificate_refused(),
            Err(error) => self.note_tls_failure("agent certificate renewal failed", &error),
        }
    }

    /// Range un certificat reçu et bascule le client dessus.
    pub(super) fn adopt_certificate(
        &mut self,
        state: &mut TlsState,
        issued: dumbmonit_proto::TlsCertificate,
        key: &[u8],
    ) {
        use base64::Engine as _;
        state.certificate = Some(issued.certificate);
        state.key = Some(base64::engine::general_purpose::STANDARD.encode(key));
        state.not_after_ms = Some(issued.not_after_ms);
        if !issued.pins.is_empty() {
            state.pins = issued.pins;
        }
        state.locked = true;
        state.transport = Some(self.client.route());
        self.client.set_pins(state.pins.clone());
        if let Err(error) = state.save(&self.tls_path) {
            // Gardé en mémoire : l'agent marche, mais se réenrôlera au prochain
            // démarrage.
            warn!(error = %format!("{error:#}"), "cannot store the encrypted channel state");
        }
        let url = self.failover.active_channel(&state.url);
        if let Err(error) = self.client.use_tls(&url, state.identity()) {
            warn!(error = %format!("{error:#}"), "cannot switch to the encrypted agent channel");
        }
        self.tls = Some(state.clone());
        self.tls_retry_at = None;
        self.tls_failed = false;
    }

    /// Le serveur ne reconnaît plus notre certificat (machine supprimée,
    /// certificat remplacé) : on l'oublie, et on se réenrôle avec le jeton.
    pub(super) fn on_certificate_refused(&mut self) {
        let Some(state) = self.tls.as_mut() else { return };
        if state.certificate.is_none() {
            return;
        }
        warn!("the server no longer accepts this agent certificate, enrolling again");
        state.drop_certificate();
        if let Err(error) = state.save(&self.tls_path) {
            warn!(error = %format!("{error:#}"), "cannot store the encrypted channel state");
        }
        let url = self.failover.active_channel(&state.url);
        if let Err(error) = self.client.use_tls(&url, None) {
            warn!(error = %format!("{error:#}"), "cannot reset the encrypted agent channel");
        }
        self.tls_retry_at = None;
    }

    /// `transport: auto` : après des échecs de connexion au canal chiffré,
    /// essaie l'autre transport (port des agents ou WebSocket de l'interface)
    /// et l'adopte s'il répond ; le choix est retenu dans `agent-tls.json`.
    /// Vrai si l'agent vient d'en changer.
    pub(super) async fn consider_transport(&mut self, min_failures: u32) -> bool {
        let Some(route) = self.client.reconsider_route(min_failures).await else { return false };
        info!(transport = self.client.describe_transport(), "agent channel transport switched");
        if let Some(state) = self.tls.as_mut() {
            state.transport = Some(route);
            if state.locked
                && let Err(error) = state.save(&self.tls_path)
            {
                warn!(error = %format!("{error:#}"), "cannot store the encrypted channel state");
            }
        }
        // L'enrôlement peut reprendre tout de suite, par le nouveau chemin.
        self.tls_retry_at = None;
        true
    }

    pub(super) fn note_tls_failure(&mut self, what: &str, error: &PushError) {
        self.tls_retry_at = Some(Instant::now() + TLS_RETRY);
        if self.tls_failed {
            debug!(%error, "{what}");
        } else {
            self.tls_failed = true;
            warn!(%error, retry_in_s = TLS_RETRY.as_secs(), "{what}");
        }
    }

    /// Échec de transport (serveur injoignable, délai, 5xx) : compté pour la
    /// bascule, et passage à l'adresse suivante de `server_urls` s'il le faut.
    pub(super) fn note_unreachable(&mut self, error: &PushError) {
        if let Some(next) = self.failover.failed(&error.to_string()) {
            self.switch_server(next);
        }
    }

    /// Compose désormais le serveur de rang `index` (0 : la primaire). Le
    /// canal reste le même : certificat, épinglage et mandataire suivent.
    pub(super) fn switch_server(&mut self, index: usize) {
        let Some(url) = self.server_root(index) else { return };
        let Some(web_url) = self.failover.web_url(index).map(str::to_string) else { return };
        let from = self.client.endpoint();
        // Sur la WebSocket, c'est l'interface de ce serveur qui la porte.
        self.client.retarget_server(&url, &web_url);
        self.failover.switched_to(index, Instant::now());
        if index == 0 {
            info!(from, to = url, "the primary server answers again, switching back to it");
        } else {
            warn!(from, to = url, "server unreachable, switching to the next one in server_urls");
        }
    }

    /// Racine à composer pour le serveur de rang `index` : son canal chiffré
    /// quand l'agent y est, l'adresse de son interface sinon.
    pub(super) fn server_root(&self, index: usize) -> Option<String> {
        if self.client.is_encrypted() {
            let primary = self.tls.as_ref()?.url.as_str();
            self.failover.channel_url(index, primary)
        } else {
            self.failover.web_url(index).map(str::to_string)
        }
    }

    /// Sur un serveur de secours, réessaie la primaire de temps en temps
    /// (requête légère, délai court) et y revient dès qu'elle répond en
    /// serveur actif.
    pub(super) async fn check_primary(&mut self) {
        if !self.failover.primary_due(Instant::now()) {
            return;
        }
        let Some(url) = self.server_root(0) else { return };
        let outcome = if self.client.via_websocket() {
            // Par la WebSocket, le port des agents de la primaire n'est pas
            // joignable : c'est sa WebSocket qui est essayée. Un serveur en
            // attente (ou un tunnel sans serveur derrière lui) ne l'ouvre pas.
            let Some(web_url) = self.failover.web_url(0).map(str::to_string) else { return };
            let probe = self.client.websocket_answers(&web_url);
            match tokio::time::timeout(failover::PROBE_TIMEOUT, probe).await {
                Ok(answered) => answered.map(|()| 200),
                Err(_) => Err("the primary's agent WebSocket did not answer in time".to_string()),
            }
        } else {
            self.client
                .probe_http()
                .get(format!("{url}{}", failover::PROBE_PATH))
                .timeout(failover::PROBE_TIMEOUT)
                .send()
                .await
                .map(|response| response.status().as_u16())
                .map_err(|error| crate::client::describe(&error))
        };
        match failover::probe_verdict(outcome) {
            Ok(()) => self.switch_server(0),
            Err(reason) => {
                debug!(primary = url, reason, "the primary server is still unavailable");
                self.failover.primary_unreachable(&reason, Instant::now());
            }
        }
    }

    /// Journalise un échec d'envoi : chaque tentative laisse une ligne, avec sa
    /// cause (statut HTTP, erreur réseau, délai dépassé) et le délai avant la
    /// suivante. Fort au premier échec et à chaque changement de cause, en
    /// `info` ensuite — et seulement une fois sur dix passé la dixième, pour
    /// qu'une panne d'une nuit ne remplisse pas le journal.
    pub(super) fn note_failure(&mut self, error: &PushError, retry_in: Duration) {
        let attempt = self.backoff.attempts();
        let cause = error.to_string();
        let retry_in_s = retry_in.as_secs();
        let buffered = self.buffer.len();
        let Some((_, last)) = self.outage.as_mut() else {
            self.outage = Some((Instant::now(), cause.clone()));
            // Ces deux refus ont déjà leur propre message, plus parlant.
            if !matches!(error, PushError::Forbidden(_) | PushError::TooLarge(_)) {
                warn!(error = %cause, buffered, retry_in_s, "send failed, measurements kept");
            }
            return;
        };
        if *last != cause {
            *last = cause.clone();
            warn!(error = %cause, attempt, buffered, retry_in_s, "send still failing");
        } else if attempt <= 10 || attempt.is_multiple_of(10) {
            info!(error = %cause, attempt, buffered, retry_in_s, "send failed again");
        } else {
            debug!(error = %cause, attempt, retry_in_s, "send failed again");
        }
    }
}
