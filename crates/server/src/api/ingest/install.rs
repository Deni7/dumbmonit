//! Paramètres d'installation des agents : commandes, passerelles de relais et révocation de jetons.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use dumbmonit_proto::TargetId;
use serde::{Deserialize, Serialize};

use crate::api::pin_probe;
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::clients;
use crate::auth::middleware::{AdminIdentity, Authenticated};
use crate::collectors::agent;
use crate::state::AppState;

/// Tout ce qu'une commande d'installation porte, hormis le jeton.
///
/// Source unique des commandes affichées par l'interface (création d'un jeton)
/// et de tout générateur de déploiement (script GPO, Ansible…) : ceux-ci lisent
/// `GET /api/agent/install` et remplacent `{TOKEN}` dans les gabarits, ou
/// appellent [`install_parameters_for`] côté serveur.
#[derive(Debug, Clone, Serialize)]
pub struct InstallParams {
    /// URL `https://` d'où viennent scripts d'installation et binaires : le
    /// port de l'interface, le port des agents (`agent_port_only`), ou la
    /// passerelle d'un relais.
    pub base_url: String,
    /// Vrai si tout passe par le port des agents : scripts, binaires et canal
    /// chiffré, sur un seul port (site distant). Le serveur y est reconnu à la
    /// clé de son certificat (`https_pin`), comme à travers un relais.
    pub agent_port_only: bool,
    /// URL du canal chiffré des agents (`https://hôte:8443`, ou l'adresse
    /// dédiée `DUMBMONIT_AGENT_PUBLIC_URL`), s'il est ouvert.
    pub agent_url: Option<String>,
    /// Empreinte SHA-256 de l'autorité du canal, que l'agent épingle.
    pub ca_pin: Option<String>,
    /// Empreintes SHA-256 des binaires livrés, par plateforme.
    pub checksums: Vec<InstallChecksum>,
    /// Commande Unix, jeton à remplacer : `{TOKEN}`.
    pub linux_template: String,
    /// Commande PowerShell, jeton à remplacer : `{TOKEN}`.
    pub windows_template: String,
    /// Relais par lequel passent ces commandes, s'il y en a un : `base_url` et
    /// `agent_url` sont alors ceux de sa passerelle.
    pub via_relay: Option<RelayRoute>,
    /// La machine installée sera un relais : l'installateur ouvre le port de sa
    /// passerelle sur son pare-feu (firewalld, ufw, pare-feu Windows), au seul
    /// sous-réseau local (`--open-gateway`, `-OpenGateway`).
    pub open_gateway: bool,
    /// Clé publique épinglée (`sha256//…`) du certificat que présente
    /// `base_url`, quand il n'est pas d'une autorité de confiance : celle du
    /// certificat autosigné de l'interface, ou celle du canal des agents à
    /// travers un relais. Les deux commandes vérifient le serveur par elle
    /// (`curl --pinnedpubkey`, rappel de validation en PowerShell) avant de
    /// lire le moindre octet, puis l'agent la garde (`server_pin`). `None` :
    /// certificat fourni, vérifié par les autorités du système.
    pub https_pin: Option<String>,
    /// D'où vient `https_pin`, quand le serveur a sondé l'adresse ouverte par le
    /// navigateur : `own` (certificat de DumbMonit), `proxy` (un mandataire
    /// inverse en présente un autre : c'est le sien qui est épinglé) ou
    /// `trusted` (certificat de confiance : aucune empreinte). Absent : pas de
    /// sondage (relais, adresse saisie à la main, échec).
    pub pin_source: Option<&'static str>,
    /// `websocket` : l'agent passe par l'adresse de l'interface
    /// (`--transport=websocket`, `-Transport websocket`), sa session TLS dans
    /// une WebSocket — derrière Cloudflare Tunnel ou un mandataire inverse qui
    /// termine TLS. Absent : rien d'imposé (l'agent essaie le port des agents,
    /// puis la WebSocket s'il est injoignable).
    pub transport: Option<&'static str>,
}

/// Passage par un agent relais : sa passerelle, telle que les machines du site
/// la joignent.
#[derive(Debug, Clone, Serialize)]
pub struct RelayRoute {
    pub relay_id: TargetId,
    pub relay_name: String,
    pub site: Option<String>,
    /// Adresse du relais vue du site (IP ou nom local).
    pub host: String,
    pub port: u16,
    /// Vrai si le relais déclare déjà écouter sur ce port.
    pub listening: bool,
    /// Vrai si le relais rapporte des essais de sa passerelle (binaire à
    /// jour) : l'interface attend alors le résultat de l'essai avant d'afficher
    /// la commande.
    pub self_check: bool,
    /// Issue du dernier essai du relais à cette adresse (`hôte:port`) :
    /// `Some(true)`, le serveur a répondu à travers la passerelle ;
    /// `Some(false)`, voir `check_error` ; `None`, pas encore essayée là.
    pub reachable: Option<bool>,
    pub check_error: Option<String>,
    /// Instant de cet essai, en millisecondes depuis l'époque Unix.
    pub checked_at_ms: Option<i64>,
    /// Ce qui empêche la passerelle d'accepter le site, quand le serveur le
    /// sait : fermée, relais trop ancien ou muet, port impossible à ouvrir,
    /// écoute sur la boucle locale, pas de canal chiffré.
    pub problem: Option<agent::gateway::GatewayProblem>,
    /// Adresse que le relais a vraiment liée (`0.0.0.0:8443`).
    pub listen: Option<String>,
    /// Pourquoi le relais n'a pas pu ouvrir le port, tel qu'il le rapporte.
    pub bind_error: Option<String>,
    /// Version de l'agent du relais.
    pub agent_version: String,
}

/// Résout le passage par `relay` : un agent en mode relais, du client donné
/// (`None` : sans contrôle, lecture seule), avec l'adresse de sa passerelle.
pub async fn relay_route(
    state: &AppState,
    relay: TargetId,
    host: Option<&str>,
    client: Option<i64>,
) -> ApiResult<RelayRoute> {
    let not_relay = || ApiError::BadRequest(format!("Monitor {relay} is not a relay agent."));
    let info = agent::host(&state.pool, relay).await?.ok_or_else(not_relay)?;
    if !(info.relay || info.relay_wanted) {
        return Err(ApiError::BadRequest(
            "This agent is not a relay: switch relay mode on for it first.".into(),
        ));
    }
    if let Some(client) = client
        && clients::target_client(&state.pool, relay).await? != Some(client)
    {
        return Err(ApiError::BadRequest(
            "This relay belongs to another client: pick a relay of the same client.".into(),
        ));
    }
    let typed = agent::clean_gateway_host(host).map_err(|e| ApiError::BadRequest(e.into()))?;
    let host = match typed {
        Some(host) => host,
        None => match info.relay_gateway_host.clone() {
            Some(host) => host,
            None => {
                agent::clean_gateway_host(Some(&info.hostname)).ok().flatten().ok_or_else(|| {
                    ApiError::BadRequest("Give the address the site reaches this relay at.".into())
                })?
            }
        },
    };
    let port = agent::gateway::dialed_port(&info);
    let offline = agent::gateway::offline(&state.pool, relay).await?;
    let problem = agent::gateway::problem(&info, offline);
    let relay_name: String = sqlx::query_scalar("SELECT name FROM targets WHERE id = ?")
        .bind(relay)
        .fetch_one(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;
    let address = format!("{host}:{port}");
    let check = info
        .relay_gateway_check
        .as_ref()
        .filter(|check| check.address.eq_ignore_ascii_case(&address));
    Ok(RelayRoute {
        relay_id: relay,
        relay_name,
        site: info.site.clone().or(info.relay_wanted_site.clone()),
        listening: info.relay_gateway_port == Some(port),
        self_check: info.relay_gateway_check.is_some(),
        reachable: check.map(|check| check.ok),
        check_error: check.and_then(|check| check.error.clone()),
        checked_at_ms: check.map(|check| check.checked_at_ms),
        problem,
        listen: info.relay_gateway_state.as_ref().and_then(|state| state.listen.clone()),
        bind_error: info.relay_gateway_state.as_ref().and_then(|state| state.error.clone()),
        agent_version: info.agent_version.clone(),
        host,
        port,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct InstallChecksum {
    pub platform: String,
    pub sha256: String,
}

/// Marque du jeton dans les gabarits de [`InstallParams`].
pub const TOKEN_PLACEHOLDER: &str = "{TOKEN}";

impl InstallParams {
    fn pairs(&self, windows: bool) -> Vec<(String, String)> {
        self.checksums
            .iter()
            .filter(|sum| sum.platform.starts_with("windows-") == windows)
            .map(|sum| (sum.platform.clone(), sum.sha256.clone()))
            .collect()
    }

    /// Commande Unix complète pour ce jeton.
    pub fn linux_command(&self, token: &str) -> String {
        let mut command =
            install_linux(&self.base_url, token, &self.pairs(false), self.https_pin.as_deref());
        if let (Some(url), Some(pin)) = (&self.agent_url, &self.ca_pin) {
            command.push_str(&format!(" --tls-url={url} --ca-pin={pin}"));
        }
        if let Some(transport) = self.transport {
            command.push_str(&format!(" --transport={transport}"));
        }
        if self.open_gateway {
            command.push_str(" --open-gateway");
        }
        command
    }

    /// Commande PowerShell complète pour ce jeton.
    pub fn windows_command(&self, token: &str) -> String {
        let mut command =
            install_windows(&self.base_url, token, &self.pairs(true), self.https_pin.as_deref());
        if let (Some(url), Some(pin)) = (&self.agent_url, &self.ca_pin) {
            command.push_str(&format!(" -TlsUrl {url} -CaPin {pin}"));
        }
        if let Some(transport) = self.transport {
            command.push_str(&format!(" -Transport {transport}"));
        }
        if self.open_gateway {
            command.push_str(" -OpenGateway");
        }
        command
    }
}

/// Transport du canal chiffré que la commande d'installation doit donner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportWish {
    /// Au jugé : la WebSocket quand l'interface est vue à travers un
    /// intermédiaire qui termine TLS et qu'aucune adresse dédiée aux agents
    /// n'est réglée ; sinon rien (l'agent essaie le port des agents, puis la
    /// WebSocket de lui-même).
    #[default]
    Auto,
    /// Toujours par l'adresse de l'interface (WebSocket).
    Websocket,
    /// Jamais de transport imposé dans la commande.
    Direct,
}

/// Valeur de `--transport` quand les agents passent par l'adresse de
/// l'interface.
pub const TRANSPORT_WEBSOCKET: &str = "websocket";

/// Paramètres d'installation pour une URL de base donnée (celle par laquelle
/// les machines joignent l'interface ; à défaut, l'adresse d'écoute), en direct
/// ou à travers un agent relais, avec le transport demandé. Avec les en-têtes
/// de la requête entrante, sans relais, le serveur sonde l'adresse que le
/// navigateur a ouverte pour y relever le certificat réellement présenté.
/// `agent_port_only` : sans relais, tout passe par le port des agents.
///
/// Derrière Cloudflare Tunnel ou un mandataire inverse qui termine TLS, le
/// port des agents n'est pas joignable à l'adresse du navigateur : la
/// commande fait alors passer l'agent par la WebSocket de l'interface
/// (`--transport=websocket`). Scripts et binaires viennent de cette même
/// adresse, jamais du port des agents.
pub async fn install_parameters_for(
    state: &AppState,
    base_url: Option<&str>,
    route: Option<RelayRoute>,
    headers: Option<&HeaderMap>,
    agent_port_only: bool,
    wish: TransportWish,
) -> InstallParams {
    let web_url = match &route {
        Some(route) => format!("https://{}:{}", route.host, route.port),
        None => normalise_base_url(base_url, state.config.bind),
    };
    let offer = crate::agent_tls::offer(state);
    let agent_url = match &route {
        Some(_) => offer.as_ref().map(|_| web_url.clone()),
        None => offer.as_ref().and_then(|offer| direct_agent_url(state, &web_url, offer.port)),
    };
    // Le navigateur est sur le port unique : interface et agents y passent
    // déjà tous deux, la commande n'a qu'un port de toute façon. Scripts et
    // binaires s'y lisent sous le certificat de l'interface (un client sans la
    // marque ALPN des agents y voit l'interface) : c'est donc son épingle, pas
    // celle du port des agents, que la commande porte.
    let shared = route.is_none() && on_single_port(state, &web_url);
    // Ce que présente réellement l'adresse ouverte par le navigateur : le
    // certificat de DumbMonit, ou celui d'un intermédiaire (son épingle, et
    // de quoi savoir qu'il y en a un).
    let probed = match (&route, headers) {
        (None, Some(headers)) => {
            let target = pin_probe::probe_target(&web_url, headers);
            let seen = match target {
                Some((host, port)) => pin_probe::probe(&host, port).await,
                None => None,
            };
            Some(pin_probe::decide(seen.as_ref(), web_pin(state)))
        }
        _ => None,
    };
    let behind_proxy = matches!(
        probed,
        Some((_, Some(pin_probe::PinSource::Proxy | pin_probe::PinSource::Trusted)))
    );
    let websocket = route.is_none()
        && offer.is_some()
        && state.config.agent_websocket
        && match wish {
            TransportWish::Websocket => true,
            TransportWish::Direct => false,
            TransportWish::Auto => behind_proxy && state.config.agent_public_url.is_none(),
        };
    // Un seul port : il faut le canal ouvert et la clé de son certificat, que
    // les commandes épinglent pour lire scripts et binaires. Pas par la
    // WebSocket : le port des agents n'est justement pas joignable.
    let server_pin = state.agent_tls().server_pin();
    let single_port =
        route.is_none() && agent_port_only && server_pin.is_some() && !shared && !websocket;
    let (base_url, agent_port_only) = match (&agent_url, single_port) {
        (Some(url), true) => (url.clone(), true),
        _ => (web_url, false),
    };
    let mut pin_source = None;
    let https_pin = match &route {
        Some(_) => server_pin,
        None if agent_port_only => server_pin,
        None => match probed {
            Some((pin, source)) => {
                pin_source = source.map(pin_probe::PinSource::as_str);
                pin
            }
            None => web_pin(state),
        },
    };
    let checksums = crate::api::agent_files::platform_checksums(&state.config.agent_dir)
        .await
        .into_iter()
        .map(|(platform, sha256)| InstallChecksum { platform, sha256 })
        .collect();
    // L'autorité active seule : une suivante (rotation en cours) est apprise
    // par le canal chiffré lui-même.
    let ca_pin = offer.and_then(|offer| offer.pins.into_iter().next());
    let mut params = InstallParams {
        base_url,
        agent_port_only,
        agent_url,
        ca_pin,
        checksums,
        linux_template: String::new(),
        windows_template: String::new(),
        https_pin,
        pin_source,
        via_relay: route,
        open_gateway: false,
        transport: websocket.then_some(TRANSPORT_WEBSOCKET),
    };
    params.linux_template = params.linux_command(TOKEN_PLACEHOLDER);
    params.windows_template = params.windows_command(TOKEN_PLACEHOLDER);
    params
}

/// URL du canal chiffré pour une installation directe : l'adresse dédiée aux
/// agents (`DUMBMONIT_AGENT_PUBLIC_URL`) quand il y en a une ; sinon, si le
/// navigateur est sur le port unique (`DUMBMONIT_SINGLE_PORT`), cette même
/// adresse — les agents s'y désignent par ALPN ; sinon le même hôte que
/// l'interface vue par le navigateur, sur le port des agents
/// (`DUMBMONIT_AGENT_PUBLIC_PORT`, à défaut celui de l'écoute).
pub(super) fn direct_agent_url(state: &AppState, web_url: &str, port: u16) -> Option<String> {
    if let Some(url) = state.config.agent_public_url.clone() {
        return Some(url);
    }
    if on_single_port(state, web_url) {
        return Some(web_url.to_string());
    }
    dumbmonit_proto::agent_channel_url(web_url, port)
}

/// Vrai si l'adresse de l'interface vue par le navigateur est celle du port
/// unique : même numéro de port (443 quand l'adresse n'en dit rien). Derrière
/// une redirection vers un autre port, `DUMBMONIT_AGENT_PUBLIC_URL` le dit.
pub(super) fn on_single_port(state: &AppState, web_url: &str) -> bool {
    let Some(single) = state.config.single_port() else { return false };
    reqwest::Url::parse(web_url).ok().and_then(|url| url.port_or_known_default()) == Some(single)
}

/// Clé à épingler pour joindre le port de l'interface : celle du certificat
/// autosigné. Un certificat fourni est vérifié par les autorités du système.
pub(super) fn web_pin(state: &AppState) -> Option<String> {
    let summary = state.web_tls().summary()?;
    match summary.source {
        crate::web_tls::Source::SelfSigned => summary.public_key_pin,
        crate::web_tls::Source::Provided => None,
    }
}

#[derive(Debug, Deserialize)]
pub struct InstallQuery {
    #[serde(default)]
    pub base_url: Option<String>,
    /// Commandes à travers ce relais.
    #[serde(default)]
    pub relay_id: Option<TargetId>,
    #[serde(default)]
    pub relay_host: Option<String>,
    /// Commandes qui n'utilisent que le port des agents (site distant).
    #[serde(default)]
    pub agent_port_only: bool,
    /// Transport du canal chiffré (voir [`TokenPayload::transport`]).
    #[serde(default)]
    pub transport: TransportWish,
}

/// `GET /api/agent/install?base_url=…` : paramètres et gabarits de la commande
/// d'installation, sans jeton.
///
/// Rien de secret : l'URL, l'empreinte publique de l'autorité, celles des
/// binaires. Ouverte à toute session et aux jetons `read`.
pub async fn install_params(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<InstallQuery>,
) -> ApiResult<Json<InstallParams>> {
    let route = match query.relay_id {
        Some(relay) => Some(relay_route(&state, relay, query.relay_host.as_deref(), None).await?),
        None => None,
    };
    Ok(Json(
        install_parameters_for(
            &state,
            query.base_url.as_deref(),
            route,
            Some(&headers),
            query.agent_port_only,
            query.transport,
        )
        .await,
    ))
}

/// `GET /api/agent/tokens/{id}/machines` : machines enrôlées par ce jeton, la
/// plus récente d'abord. Le flux d'ajout d'un agent l'interroge pour constater
/// la première connexion, puis nommer et ranger la machine sur place.
pub async fn token_machines(
    _: AdminIdentity,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<agent::TokenMachine>>> {
    Ok(Json(agent::token_machines(&state.pool, id).await?))
}

pub async fn revoke_token(
    State(state): State<AppState>,
    Authenticated(me): Authenticated,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if agent::revoke_token(&state.pool, id).await? {
        let subject = id.to_string();
        audit::record(&state.pool, Some(&me.username), "agent_token.revoked", Some(&subject), ip)
            .await;
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound(format!("Token {id} not found or already revoked.")))
    }
}

/// URL de base à écrire dans la commande d'installation : toujours en
/// `https://`, le port ne servant que HTTPS.
///
/// Elle vient du navigateur et finit dans une commande que l'administrateur
/// colle dans un terminal : seuls les caractères d'une URL ordinaire y entrent
/// (ni blanc, ni `;`, `|`, `$`, guillemet…). Sinon, ou sans URL fournie,
/// repli honnête sur l'adresse d'écoute : juste sur un réseau local, et
/// visiblement fausse — donc corrigeable — derrière un mandataire.
pub(super) fn normalise_base_url(provided: Option<&str>, bind: std::net::SocketAddr) -> String {
    let fallback = || format!("https://{bind}");
    let Some(url) = provided.map(str::trim).filter(|url| !url.is_empty()) else {
        return fallback();
    };
    let url = url.trim_end_matches('/');
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    // Une adresse en clair (copiée d'un ancien lien) : même hôte, en HTTPS.
    if !matches!(scheme.to_ascii_lowercase().as_str(), "https" | "http") {
        return fallback();
    }
    let safe = |b: u8| b.is_ascii_alphanumeric() || b"-._~:/[]%".contains(&b);
    if rest.is_empty() || rest.starts_with('/') || !rest.bytes().all(safe) {
        return fallback();
    }
    format!("https://{rest}")
}

/// En-tête `SubjectPublicKeyInfo` d'une clé P-256 (celles du certificat
/// autosigné et du canal des agents), en octets décimaux : PowerShell refait
/// l'épingle `sha256//…` de curl à partir du point de la clé.
pub(super) const P256_SPKI_PREFIX: &str =
    "48,89,48,19,6,7,42,134,72,206,61,2,1,6,8,42,134,72,206,61,3,1,7,3,66,0";

/// Une épingle telle que la produit [`crate::web_tls::cert::spki_pin`] : rien
/// d'autre n'entre dans une commande.
pub(super) fn clean_pin(pin: Option<&str>) -> Option<&str> {
    let base64 = |b: &u8| b.is_ascii_alphanumeric() || *b == b'+' || *b == b'/';
    pin.filter(|pin| {
        let digest = pin.strip_prefix("sha256//").unwrap_or_default().as_bytes();
        digest.len() == 44 && digest[43] == b'=' && digest[..43].iter().all(base64)
    })
}

/// Commande d'installation Unix. Elle porte l'empreinte de chaque binaire Unix
/// que l'image livre (`--sha256=linux-x86_64:…,…`) : le script refuse un
/// téléchargement qui n'y correspond pas, ou pour lequel elle manque.
pub(super) fn install_linux(
    base_url: &str,
    token: &str,
    checksums: &[(String, String)],
    https_pin: Option<&str>,
) -> String {
    let mut command = match clean_pin(https_pin) {
        // `-k` lève la vérification par une autorité, que le certificat
        // autosigné ne peut pas passer ; `--pinnedpubkey` la remplace par la
        // clé attendue, et le script fait de même pour le binaire.
        Some(pin) => format!(
            "curl -sSL -k --pinnedpubkey {pin} {base_url}/install.sh | sh -s -- \
             --token={token} --url={base_url} --pinned-key={pin}"
        ),
        None => {
            format!("curl -sSL {base_url}/install.sh | sh -s -- --token={token} --url={base_url}")
        }
    };
    let unix: Vec<String> = checksums
        .iter()
        .filter(|(platform, _)| !platform.starts_with("windows-"))
        .map(|(platform, digest)| format!("{platform}:{digest}"))
        .collect();
    if !unix.is_empty() {
        command.push_str(&format!(" --sha256={}", unix.join(",")));
    }
    command
}

/// Commande d'installation PowerShell. TLS 1.2 est d'abord autorisé (Windows
/// PowerShell 5 ne le propose pas toujours). Avec une épingle, un rappel de
/// validation n'accepte que le certificat dont la clé publique a cette
/// empreinte, avant que le script ne soit lu ; l'installateur le garde pour le
/// binaire, l'écrit dans la configuration de l'agent, puis le retire.
pub(super) fn install_windows(
    base_url: &str,
    token: &str,
    checksums: &[(String, String)],
    https_pin: Option<&str>,
) -> String {
    const TLS12: &str = "[Net.ServicePointManager]::SecurityProtocol = \
                         [Net.ServicePointManager]::SecurityProtocol -bor 3072";
    // `iex` ne sait pas passer d'arguments : il faut construire un bloc de script.
    // C'est la formule consacrée pour un installateur PowerShell paramétré.
    let mut command = match clean_pin(https_pin) {
        Some(pin) => format!(
            "{TLS12}; [Net.ServicePointManager]::ServerCertificateValidationCallback = \
             {{ param($s, $c) ('sha256//' + [Convert]::ToBase64String(\
             [Security.Cryptography.SHA256]::Create().ComputeHash(\
             [byte[]](({P256_SPKI_PREFIX}) + $c.GetPublicKey())))) -ceq '{pin}' }}; \
             & ([scriptblock]::Create((New-Object Net.WebClient).DownloadString(\
             '{base_url}/install.ps1'))) -Token {token} -Url {base_url} -PinnedKey {pin}"
        ),
        None => format!(
            "{TLS12}; & ([scriptblock]::Create((irm {base_url}/install.ps1))) \
             -Token {token} -Url {base_url}"
        ),
    };
    let windows: Vec<String> = checksums
        .iter()
        .filter(|(platform, _)| platform.starts_with("windows-"))
        .map(|(platform, digest)| format!("{platform}:{digest}"))
        .collect();
    if !windows.is_empty() {
        command.push_str(&format!(" -Sha256 {}", windows.join(",")));
    }
    command
}
