//! Échanges du client avec le serveur : requêtes, envoi des lots, commandes, relais.

use super::*;

impl PushClient {
    /// Requête vers un canal d'agent (commandes, relais) : jeton, secret de
    /// liaison et clé d'identité, celle-ci dans son en-tête — jamais dans l'URL,
    /// sauf pour un serveur trop ancien pour lire l'en-tête.
    pub(super) fn keyed(
        &self,
        method: reqwest::Method,
        path: &str,
        key: &str,
        query: &str,
    ) -> reqwest::RequestBuilder {
        let channel = self.channel();
        let mut url = format!("{}{path}", channel.base_url);
        let mut params: Vec<String> = Vec::new();
        if self.legacy_key.load(Ordering::Relaxed) {
            params.push(format!("key={}", percent_encode(key)));
        }
        if !query.is_empty() {
            params.push(query.to_string());
        }
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        self.identified(
            channel
                .http
                .request(method, url)
                .bearer_auth(&self.token)
                .header(AGENT_KEY_HEADER, key),
        )
    }

    /// Envoie une requête de canal d'agent, et la rejoue une fois avec la clé dans
    /// l'URL si le serveur, antérieur à l'en-tête, la réclame là.
    pub(super) async fn exchange(
        &self,
        build: impl Fn() -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, PushError> {
        let encrypted = self.is_encrypted();
        let response = build().send().await.map_err(|error| self.transport(&error))?;
        self.observe(&response, encrypted);
        if response.status() != reqwest::StatusCode::BAD_REQUEST
            || self.legacy_key.load(Ordering::Relaxed)
        {
            return Ok(response);
        }
        let body = response.text().await.unwrap_or_default();
        if !wants_legacy_key(&body) {
            return Err(rejection_of(reqwest::StatusCode::BAD_REQUEST, &body));
        }
        tracing::info!("older server: the machine key goes in the URL again");
        self.legacy_key.store(true, Ordering::Relaxed);
        build().send().await.map_err(|error| self.transport(&error))
    }

    /// Erreur de transport lisible : la cause d'abord, l'URL sans paramètres.
    /// Un échec de connexion est compté (`transport: auto`) ; par la WebSocket,
    /// c'est la cause de son ouverture qui est dite, pas la poignée de main
    /// interrompue qu'en voit le client.
    pub(super) fn transport(&self, error: &reqwest::Error) -> PushError {
        let unreachable = error.is_connect() || error.is_timeout();
        if unreachable {
            self.transport.note_unreachable();
        }
        let message = self
            .transport
            .websocket_failure()
            .filter(|_| unreachable)
            .unwrap_or_else(|| describe(error));
        PushError::Transport(sanitise(&message, &self.token))
    }

    /// Adopte le secret que le serveur vient d'attribuer.
    pub fn set_secret(&self, secret: Option<String>) {
        if let Ok(mut held) = self.secret.write() {
            *held = secret;
        }
    }

    /// Ajoute le secret de liaison à une requête, s'il y en a un.
    ///
    /// Un agent pas encore lié n'envoie pas l'en-tête du tout : c'est ce qui
    /// demande implicitement au serveur de lui en attribuer un.
    pub(super) fn identified(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let secret = self.secret.read().ok().and_then(|held| held.clone());
        match secret {
            Some(secret) => request.header(AGENT_SECRET_HEADER, secret),
            None => request,
        }
    }

    /// Garde le canal (client HTTP, certificat client, épinglage) et change
    /// seulement le serveur visé : bascule vers un serveur de secours, ou
    /// retour à la primaire (voir `failover.rs`). Les deux partagent
    /// l'autorité des agents : le même certificat y est reconnu.
    pub fn retarget(&self, base_url: &str) {
        let current = self.channel();
        let channel = Channel {
            http: current.http.clone(),
            upgrade: current.upgrade.clone(),
            base_url: base_url.trim_end_matches('/').to_string(),
            encrypted: current.encrypted,
        };
        if let Ok(mut held) = self.channel.write() {
            *held = Arc::new(channel);
        }
    }

    /// Change de serveur (`server_urls`) : `root`, son canal chiffré (ou son
    /// interface sur l'ancien canal), `web_url` la racine de son interface,
    /// où vit sa WebSocket. Sur le canal chiffré, le canal est remonté sur le
    /// chemin en cours : par la WebSocket, le pont vise désormais ce serveur
    /// et aucune connexion ne reste ouverte vers l'ancien.
    pub fn retarget_server(&self, root: &str, web_url: &str) {
        self.transport.retarget(crate::ws::websocket_url(web_url));
        let identity = match self.target.lock() {
            Ok(mut target) => target.as_mut().map(|(url, identity)| {
                *url = root.to_string();
                identity.clone()
            }),
            Err(_) => None,
        };
        if let Some(identity) = identity.filter(|_| self.is_encrypted()) {
            match self.install_tls(self.transport.route(), root, identity) {
                Ok(()) => return,
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "agent channel not moved");
                }
            }
        }
        self.retarget(root);
    }

    /// Le serveur de cette interface répond-il par sa WebSocket ? Vrai si
    /// DumbMonit y accepte la mise à niveau des agents (un serveur en attente
    /// ne l'accepte pas). L'essai de la primaire quand l'agent passe par la
    /// WebSocket : le port des agents, lui, n'est pas joignable.
    pub async fn websocket_answers(&self, web_url: &str) -> Result<(), String> {
        let url = crate::ws::websocket_url(web_url)
            .ok_or_else(|| format!("no agent WebSocket address for {web_url}"))?;
        let connector = self
            .transport
            .connector_for(url)
            .ok_or_else(|| "the WebSocket transport is not configured".to_string())?;
        connector.answers().await
    }

    /// Vrai si le canal chiffré passe en ce moment par la WebSocket.
    pub fn via_websocket(&self) -> bool {
        self.is_encrypted() && self.transport.route() == Route::WebSocket
    }

    /// Adresse du serveur telle qu'elle se dit (journaux, lot) : la WebSocket
    /// (`wss://…`) quand c'est elle qui sert, jamais le pont local.
    pub fn endpoint(&self) -> String {
        if self.is_encrypted()
            && let Some(address) = self.transport.websocket_address()
        {
            return address;
        }
        self.base_url()
    }

    /// Racine du serveur visé en ce moment, sans chemin.
    pub fn base_url(&self) -> String {
        self.channel().base_url.clone()
    }

    /// URL effectivement appelée, pour la journalisation de démarrage.
    pub fn url(&self) -> String {
        ingest_url(&self.channel().base_url)
    }

    pub async fn send(&self, batch: &PushBatch) -> Result<PushAck, PushError> {
        let channel = self.channel();
        let response = self
            .identified(channel.http.post(ingest_url(&channel.base_url)).bearer_auth(&self.token))
            .json(batch)
            .send()
            .await
            .map_err(|error| self.transport(&error))?;
        self.observe(&response, channel.encrypted);

        let status = response.status();
        if status.is_success() {
            let ack = response
                .json::<PushAck>()
                .await
                .map_err(|error| PushError::Transport(format!("unreadable response: {error}")))?;
            self.mark_reachable();
            return Ok(ack);
        }

        Err(rejection(response).await)
    }

    /// Commandes en attente pour cette machine.
    ///
    /// La clé d'identité accompagne le jeton : celui-ci est partagé par tout un
    /// parc, il ne dit pas de quelle machine il s'agit.
    pub async fn fetch_commands(&self, key: &str) -> Result<Vec<AgentCommand>, PushError> {
        let path = commands_path(None);
        let response = self.exchange(|| self.keyed(reqwest::Method::GET, &path, key, "")).await?;
        self.read_json(response).await
    }

    /// Corps JSON d'une réponse favorable, ou qualification du refus.
    pub(super) async fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, PushError> {
        if !response.status().is_success() {
            return Err(rejection(response).await);
        }
        let value = response
            .json::<T>()
            .await
            .map_err(|error| PushError::Transport(format!("unreadable response: {error}")))?;
        self.mark_reachable();
        Ok(value)
    }

    /// Réponse favorable sans corps attendu, ou qualification du refus.
    pub(super) async fn read_empty(&self, response: reqwest::Response) -> Result<(), PushError> {
        if !response.status().is_success() {
            return Err(rejection(response).await);
        }
        self.mark_reachable();
        Ok(())
    }

    /// Lot de lignes de journal, sur le même canal que les commandes.
    pub async fn send_logs(&self, key: &str, batch: &LogBatch) -> Result<(), PushError> {
        let response = self
            .exchange(|| self.keyed(reqwest::Method::POST, LOGS_PATH, key, "").json(batch))
            .await?;
        self.read_empty(response).await
    }

    /// Compte rendu d'exécution d'une commande.
    pub async fn report_command(
        &self,
        key: &str,
        id: i64,
        report: &CommandReport,
    ) -> Result<(), PushError> {
        let path = commands_path(Some(id));
        let response = self
            .exchange(|| self.keyed(reqwest::Method::POST, &path, key, "").json(report))
            .await?;
        self.read_empty(response).await
    }
}

impl PushClient {
    /// Sondes que le serveur nous délègue (agent relais).
    ///
    /// Le serveur retient la réponse jusqu'à `wait` secondes s'il n'a rien à
    /// donner : `timeout` doit dépasser cette attente, sans quoi chaque tour
    /// vide finirait en erreur de transport.
    pub async fn fetch_probes(
        &self,
        key: &str,
        wait: Duration,
        timeout: Duration,
    ) -> Result<Vec<AgentCommand>, PushError> {
        let path = relay_path(None);
        let query = format!("wait={}", wait.as_secs());
        let response = self
            .exchange(|| self.keyed(reqwest::Method::GET, &path, key, &query).timeout(timeout))
            .await?;
        if response.headers().contains_key(RELAY_RESYNC_HEADER) {
            // Le réglage de ce relais vient de changer : le prochain accusé de
            // réception le porte, autant l'avoir tout de suite.
            self.request_report();
        }
        self.read_json(response).await
    }

    /// Mesures et verdict d'une sonde déléguée.
    pub async fn report_probe(
        &self,
        key: &str,
        id: i64,
        outcome: &ProbeOutcome,
    ) -> Result<(), PushError> {
        let path = relay_path(Some(id));
        let response = self
            .exchange(|| self.keyed(reqwest::Method::POST, &path, key, "").json(outcome))
            .await?;
        self.read_empty(response).await
    }

    /// Compte rendu progressif d'un balayage de découverte confié à cet agent.
    /// Rend `false` quand le serveur a répondu `410 Gone` : le balayage a été
    /// annulé, l'agent doit s'arrêter.
    pub async fn report_discovery(
        &self,
        key: &str,
        id: i64,
        report: &dumbmonit_proto::DiscoveryReport,
    ) -> Result<bool, PushError> {
        let path = format!("{}/{id}", dumbmonit_proto::DISCOVERY_REPORT_PATH);
        let response = self
            .exchange(|| self.keyed(reqwest::Method::POST, &path, key, "").json(report))
            .await?;
        if response.status() == reqwest::StatusCode::GONE {
            return Ok(false);
        }
        self.read_empty(response).await.map(|()| true)
    }

    /// Planning des sondes de ce relais, gardé pour les coupures du lien.
    pub async fn fetch_plan(&self, key: &str) -> Result<RelayPlan, PushError> {
        let response =
            self.exchange(|| self.keyed(reqwest::Method::GET, RELAY_PLAN_PATH, key, "")).await?;
        self.read_json(response).await
    }

    /// Lot de résultats obtenus pendant une coupure.
    pub async fn send_backlog(
        &self,
        key: &str,
        backlog: &RelayBacklog,
    ) -> Result<BacklogAck, PushError> {
        let post = || self.keyed(reqwest::Method::POST, RELAY_BACKLOG_PATH, key, "").json(backlog);
        let response = self.exchange(post).await?;
        self.read_json(response).await
    }
}

impl PushClient {
    /// Binaire proposé par le serveur pour la mise à jour de l'agent, par le
    /// canal courant (chiffré quand l'agent l'a) : au plus `max` octets.
    pub async fn download_update(
        &self,
        key: &str,
        file: &str,
        max: u64,
        timeout: Duration,
    ) -> Result<Vec<u8>, PushError> {
        let path = format!("{}/{file}", dumbmonit_proto::UPDATE_DOWNLOAD_PATH);
        let response = self
            .exchange(|| self.keyed(reqwest::Method::GET, &path, key, "").timeout(timeout))
            .await?;
        if !response.status().is_success() {
            return Err(rejection(response).await);
        }
        if response.content_length().is_some_and(|length| length > max) {
            return Err(PushError::TooLarge(format!("the agent binary exceeds {max} bytes")));
        }
        let bytes = response.bytes().await.map_err(|error| self.transport(&error))?;
        if bytes.len() as u64 > max {
            return Err(PushError::TooLarge(format!("the agent binary exceeds {max} bytes")));
        }
        self.mark_reachable();
        Ok(bytes.to_vec())
    }

    /// Ouvre, pour la passerelle du relais, un tunnel vers le port des agents
    /// du serveur : une mise à niveau HTTP sous le certificat client du relais.
    /// Le flux rendu est ensuite une connexion brute au port des agents — c'est
    /// l'agent du site qui y fait sa propre poignée de main TLS.
    ///
    /// Jamais par l'ancien canal en clair : seul un relais authentifié par son
    /// certificat peut ouvrir un tunnel.
    pub async fn open_tunnel(&self) -> Result<reqwest::Upgraded, PushError> {
        self.upgrade_to(RELAY_TUNNEL_PROTOCOL).await
    }

    /// Ouvre la connexion multiplexée du relais (voir `mux.rs`) : la même mise à
    /// niveau, pour une connexion qui portera un flux par connexion d'agent du
    /// site. Un serveur antérieur répond `426` (`PushError::Rejected`).
    pub async fn open_mux(&self) -> Result<reqwest::Upgraded, PushError> {
        self.upgrade_to(RELAY_MUX_PROTOCOL).await
    }

    pub(super) async fn upgrade_to(&self, protocol: &'static str) -> Result<reqwest::Upgraded, PushError> {
        let channel = self.channel();
        let Some(http) = channel.upgrade.as_ref().filter(|_| channel.encrypted) else {
            return Err(PushError::Transport(
                "the relay is not on the encrypted agent channel yet".to_string(),
            ));
        };
        let target = format!("{}{RELAY_TUNNEL_PATH}", channel.base_url);
        let response = self
            .identified(http.get(target).bearer_auth(&self.token))
            .header(reqwest::header::CONNECTION, "upgrade")
            .header(reqwest::header::UPGRADE, protocol)
            .send()
            .await
            .map_err(|error| self.transport(&error))?;
        if response.status() != reqwest::StatusCode::SWITCHING_PROTOCOLS {
            return Err(rejection(response).await);
        }
        self.mark_reachable();
        response.upgrade().await.map_err(|error| self.transport(&error))
    }
}
