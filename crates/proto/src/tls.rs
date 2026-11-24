//! Canal chiffré des agents : TLS 1.3, certificats clients (mTLS) et
//! épinglage de l'autorité du serveur.
//!
//! Le serveur tient sa propre autorité de certification, créée au premier
//! démarrage. Chaque agent génère sa paire de clés sur sa machine — la clé
//! privée ne la quitte jamais —, présente la clé publique une fois avec son
//! jeton d'enrôlement et reçoit un certificat client court, renouvelé de
//! lui-même avant échéance. Le serveur, lui, n'est reconnu que par l'empreinte
//! SHA-256 de son autorité : un certificat public valide présenté par un
//! intermédiaire ne trompe pas l'agent.
//!
//! Tout ce que les deux côtés doivent écrire de la même façon est défini ici.

use serde::{Deserialize, Serialize};

use crate::AgentIdentity;

/// Port d'écoute par défaut du canal des agents.
pub const DEFAULT_AGENT_TLS_PORT: u16 = 8443;

/// Nom porté par le certificat du serveur sur le canal des agents.
///
/// L'agent joint le serveur par l'adresse qu'on lui a donnée (IP, nom local,
/// nom public derrière une redirection de port) : aucune ne peut figurer
/// d'avance dans le certificat. Il vérifie donc ce nom fixe, sous une autorité
/// épinglée que personne d'autre ne détient — c'est l'épinglage qui fait la
/// sécurité, pas le nom.
pub const AGENT_TLS_SERVER_NAME: &str = "agents.dumbmonit.internal";

/// Protocole ALPN par lequel un agent se désigne dans son ClientHello.
///
/// Un agent à jour le propose à chaque poignée de main, à côté de `h2` et de
/// `http/1.1` : sur le port unique du serveur (`DUMBMONIT_SINGLE_PORT`), c'est
/// ce qui aiguille la connexion vers le canal des agents (certificat client,
/// autorité épinglée) plutôt que vers l'interface. Le protocole effectivement
/// parlé reste HTTP (`h2` ou `http/1.1`, choisi par le serveur) : la marque ne
/// sert qu'à l'aiguillage. Un serveur antérieur l'ignore et choisit
/// `http/1.1` ; un agent antérieur ne la propose pas et passe par le port des
/// agents (8443).
pub const AGENT_ALPN: &[u8] = b"dumbmonit-agent/1";

/// Enrôlement : clé publique de l'agent contre un certificat client.
pub const TLS_ENROL_PATH: &str = "/api/agent/tls/enroll";

/// Renouvellement, sous le certificat en cours.
pub const TLS_RENEW_PATH: &str = "/api/agent/tls/renew";

/// En-tête par lequel le serveur annonce son canal chiffré :
/// `port=8443; pins=<sha256>[,<sha256>]`, suivi de `; url=https://hôte:port`
/// quand le serveur a une adresse dédiée aux agents.
///
/// Sur le canal chiffré, il tient à jour les empreintes épinglées par l'agent
/// (rotation de l'autorité). Sur l'ancien canal en clair, il invite un agent à
/// jour à migrer — c'est alors une confiance au premier usage, documentée
/// comme telle.
pub const AGENT_TLS_HEADER: &str = "x-dumbmonit-agent-tls";

/// Refus du canal chiffré à une requête sans certificat client valable
/// (absent, expiré, révoqué, ou machine supprimée). L'agent qui le reçoit
/// abandonne son certificat et se réenrôle avec son jeton.
pub const CERT_REFUSED: &str = "No valid agent certificate: this agent must enrol again \
     (its machine may have been deleted from DumbMonit).";

/// Refus de l'ancien canal en clair quand le serveur exige mTLS.
///
/// Un agent à jour lit l'en-tête [`AGENT_TLS_HEADER`] de cette réponse et
/// bascule de lui-même ; un binaire plus ancien doit être mis à jour.
pub const MTLS_REQUIRED: &str = "This server only accepts agents over its encrypted agent \
     channel (TLS 1.3 with client certificates). Update the agent by re-running the install \
     command shown in DumbMonit, and make sure the agent port of the server is reachable.";

/// Tunnel ouvert par un agent relais pour un agent de son site.
///
/// Le relais, déjà authentifié par son propre certificat client, demande ici
/// une mise à niveau HTTP (`Upgrade:` [`RELAY_TUNNEL_PROTOCOL`]). Le serveur
/// traite ensuite le flux comme une connexion neuve à son port des agents : la
/// poignée de main TLS 1.3 et le certificat client sont ceux de l'agent du
/// site, de bout en bout. Le relais ne transporte que des octets chiffrés — il
/// ne lit rien, ne peut rien modifier sans être détecté, et ne peut pas se
/// faire passer pour l'agent : il n'a ni sa clé privée ni son jeton.
pub const RELAY_TUNNEL_PATH: &str = "/api/agent/relay/tunnel";

/// Protocole de la mise à niveau HTTP d'un tunnel de relais.
pub const RELAY_TUNNEL_PROTOCOL: &str = "dumbmonit-tunnel/1";

/// Protocole de la mise à niveau HTTP qui ouvre la connexion multiplexée d'un
/// relais.
///
/// Même route que [`RELAY_TUNNEL_PATH`], autre `Upgrade:` : au lieu d'un
/// tunnel par connexion d'agent du site, le relais garde une seule connexion
/// vers le serveur et y ouvre un flux [yamux] par connexion. Chaque flux est
/// ensuite traité exactement comme un tunnel : poignée de main TLS de l'agent
/// du site, de bout en bout, et le relais ne voit que du chiffré.
///
/// Négociation : un serveur antérieur répond `426` à ce protocole ; le relais
/// revient alors aux tunnels [`RELAY_TUNNEL_PROTOCOL`] et réessaie plus tard.
/// Un relais antérieur ne demande que des tunnels, que le serveur sert
/// toujours.
///
/// [yamux]: https://github.com/hashicorp/yamux/blob/master/spec.md
pub const RELAY_MUX_PROTOCOL: &str = "dumbmonit-mux/1";

/// Transport WebSocket du canal des agents, à l'adresse de l'interface.
///
/// Derrière Cloudflare Tunnel, un mandataire inverse qui termine TLS ou un
/// mandataire d'entreprise qui ne laisse passer que HTTP, le port des agents
/// est injoignable : seule l'adresse de l'interface l'est. L'agent y ouvre
/// une WebSocket (`GET` de mise à niveau, sous-protocole
/// [`AGENT_WS_PROTOCOL`]) et fait passer *dedans* sa session TLS habituelle —
/// TLS 1.3, autorité épinglée, certificat client. Le serveur traite le flux
/// comme une connexion neuve à son port des agents : mêmes routes, mêmes
/// contrôles. L'intermédiaire ne voit que des messages binaires opaques ; le
/// TLS extérieur ne prouve rien de l'agent.
///
/// Cadrage : des messages binaires, dans l'ordre, qui portent les octets de
/// la session TLS tels quels ; un message texte est une faute, `Close` est la
/// fin du flux. L'agent envoie un ping toutes les 20 s, pour qu'aucun
/// intermédiaire ne coupe une connexion au repos.
pub const AGENT_WS_PATH: &str = "/api/agent/ws";

/// Sous-protocole WebSocket (`Sec-WebSocket-Protocol`) du canal des agents :
/// un jeton HTTP, sans `/`.
pub const AGENT_WS_PROTOCOL: &str = "dumbmonit-agent.v1";

/// Octets au plus par message envoyé sur la WebSocket des agents.
pub const AGENT_WS_CHUNK: usize = 64 * 1024;

/// Taille maximale d'un message (et d'une trame) reçu : au-delà, la
/// connexion est coupée. Le double de [`AGENT_WS_CHUNK`], pour qu'un
/// intermédiaire qui regroupe deux messages ne casse rien.
pub const AGENT_WS_MAX_MESSAGE: usize = 2 * AGENT_WS_CHUNK;

// Un message envoyé doit toujours pouvoir être reçu.
const _: () = assert!(AGENT_WS_MAX_MESSAGE >= AGENT_WS_CHUNK);

/// Port d'écoute par défaut de la passerelle d'un relais (côté site).
pub const DEFAULT_RELAY_GATEWAY_PORT: u16 = 8443;

/// Durée de vie d'un certificat client, en jours.
pub const CLIENT_CERT_DAYS: i64 = 30;

/// Marge de renouvellement : l'agent renouvelle quand il reste moins que cela.
pub const RENEW_BEFORE_DAYS: i64 = 10;

/// Demande d'enrôlement sur le canal chiffré.
///
/// Authentifiée par le jeton d'enrôlement (`Authorization: Bearer`) et, pour
/// une machine déjà liée, par son secret de liaison — exactement comme un lot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlsEnrolRequest {
    pub identity: AgentIdentity,
    /// Clé publique ECDSA P-256 de l'agent, point non compressé (65 octets), en
    /// base64 standard.
    pub public_key: String,
}

/// Demande de renouvellement, sous le certificat en cours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlsRenewRequest {
    /// Nouvelle clé publique : chaque renouvellement change aussi de clé.
    pub public_key: String,
}

/// Certificat remis à l'agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TlsCertificate {
    /// Certificat X.509 DER, en base64 standard.
    pub certificate: String,
    /// Fin de validité, en millisecondes depuis l'époque Unix.
    pub not_after_ms: i64,
    /// Empreintes SHA-256 (hexadécimal) des autorités à épingler.
    pub pins: Vec<String>,
    /// Secret de liaison attribué par cet enrôlement, la seule et unique fois.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_secret: Option<String>,
}

/// Annonce du canal chiffré, telle que l'en-tête [`AGENT_TLS_HEADER`] la porte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsOffer {
    pub port: u16,
    pub pins: Vec<String>,
    /// Adresse dédiée aux agents (`https://hôte:port`), quand le serveur en a
    /// une : elle prime sur l'hôte du `server_url` de l'agent. Un agent
    /// antérieur l'ignore.
    pub url: Option<String>,
}

impl TlsOffer {
    /// Valeur de l'en-tête.
    pub fn to_header(&self) -> String {
        let mut header = format!("port={}; pins={}", self.port, self.pins.join(","));
        if let Some(url) = self.url.as_deref().filter(|url| is_offer_url(url)) {
            header.push_str("; url=");
            header.push_str(url);
        }
        header
    }

    /// Lit l'en-tête ; `None` s'il est illisible, sans port ou sans empreinte
    /// valable. Une empreinte doit être un SHA-256 hexadécimal : tout le reste
    /// est écarté plutôt qu'épinglé.
    pub fn parse(value: &str) -> Option<Self> {
        let mut port = None;
        let mut pins = Vec::new();
        let mut url = None;
        for part in value.split(';') {
            let Some((key, raw)) = part.split_once('=') else { continue };
            match key.trim() {
                "port" => port = raw.trim().parse::<u16>().ok().filter(|port| *port != 0),
                "pins" => {
                    pins = raw
                        .split(',')
                        .map(|pin| pin.trim().to_ascii_lowercase())
                        .filter(|pin| is_pin(pin))
                        .collect();
                }
                "url" => url = Some(raw.trim().to_string()).filter(|url| is_offer_url(url)),
                _ => {}
            }
        }
        let port = port?;
        if pins.is_empty() {
            return None;
        }
        Some(Self { port, pins, url })
    }
}

/// Une adresse d'annonce acceptable : `https://hôte:port`, sans chemin ni
/// caractère qui sortirait de l'en-tête ou d'une URL ordinaire. Le canal reste
/// épinglé : une adresse annoncée ne peut mener qu'au même serveur.
fn is_offer_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let safe = |b: u8| b.is_ascii_alphanumeric() || b"-._:[]".contains(&b);
    !rest.is_empty() && rest.len() <= 260 && rest.bytes().all(safe)
}

/// Vrai pour une empreinte SHA-256 en hexadécimal minuscule.
pub fn is_pin(pin: &str) -> bool {
    pin.len() == 64
        && pin.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Normalise une empreinte saisie (`AB:CD…`, majuscules, espaces) ; `None`
/// si ce n'en est pas une.
pub fn normalise_pin(raw: &str) -> Option<String> {
    let pin: String = raw
        .chars()
        .filter(|c| !matches!(c, ':' | ' ' | '-'))
        .collect::<String>()
        .to_ascii_lowercase();
    is_pin(&pin).then_some(pin)
}

/// URL du canal chiffré pour une URL de serveur donnée : le même hôte, le
/// port des agents. `None` pour une adresse d'écoute qui n'en est pas une
/// (`0.0.0.0`).
pub fn agent_channel_url(server_url: &str, port: u16) -> Option<String> {
    let rest = server_url.split_once("://").map_or(server_url, |(_, rest)| rest);
    let authority = rest.split('/').next().unwrap_or(rest);
    let authority = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    // `[::1]:8080` → `[::1]` ; `hôte:8080` → `hôte` ; `hôte` → `hôte`.
    let host = match authority.find(']') {
        Some(end) => &authority[..=end],
        None => authority.split(':').next().unwrap_or(authority),
    };
    let host = host.trim();
    if host.is_empty() || host == "0.0.0.0" || host == "[::]" {
        return None;
    }
    Some(format!("https://{host}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PIN_A: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const PIN_B: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

    #[test]
    fn the_offer_round_trips_through_its_header() {
        let offer = TlsOffer { port: 8443, pins: vec![PIN_A.into(), PIN_B.into()], url: None };
        assert_eq!(TlsOffer::parse(&offer.to_header()), Some(offer));
        let dedicated = TlsOffer {
            port: 8443,
            pins: vec![PIN_A.into()],
            url: Some("https://agents.example.com:443".into()),
        };
        assert!(dedicated.to_header().ends_with("; url=https://agents.example.com:443"));
        assert_eq!(TlsOffer::parse(&dedicated.to_header()), Some(dedicated));
    }

    #[test]
    fn an_announced_address_is_https_host_and_port_or_nothing() {
        let header = |url: &str| format!("port=8443; pins={PIN_A}; url={url}");
        for refused in
            ["http://agents.example.com:80", "https://a.example/p", "https://", "ftp://x"]
        {
            assert_eq!(TlsOffer::parse(&header(refused)).unwrap().url, None, "{refused}");
        }
        let offer = TlsOffer::parse(&header(" https://[fd00::5]:9443 ")).unwrap();
        assert_eq!(offer.url.as_deref(), Some("https://[fd00::5]:9443"));
        // Une annonce antérieure, sans adresse, reste lisible.
        assert_eq!(TlsOffer::parse(&format!("port=8443; pins={PIN_A}")).unwrap().url, None);
    }

    #[test]
    fn an_offer_without_a_valid_pin_or_port_is_ignored() {
        assert_eq!(TlsOffer::parse("port=8443; pins="), None);
        assert_eq!(TlsOffer::parse("port=8443; pins=pas-une-empreinte"), None);
        assert_eq!(TlsOffer::parse(&format!("pins={PIN_A}")), None);
        assert_eq!(TlsOffer::parse(&format!("port=0; pins={PIN_A}")), None);
        // Une empreinte douteuse est écartée, la bonne gardée.
        let offer = TlsOffer::parse(&format!("port=9443; pins=zz,{}", PIN_A.to_uppercase()));
        assert_eq!(offer, Some(TlsOffer { port: 9443, pins: vec![PIN_A.into()], url: None }));
    }

    #[test]
    fn the_agent_channel_url_keeps_the_host_and_takes_the_agent_port() {
        assert_eq!(
            agent_channel_url("https://monit.example.com", 8443).as_deref(),
            Some("https://monit.example.com:8443")
        );
        assert_eq!(
            agent_channel_url("http://192.168.1.10:8080/", 9443).as_deref(),
            Some("https://192.168.1.10:9443")
        );
        assert_eq!(
            agent_channel_url("http://[fd00::1]:8080", 8443).as_deref(),
            Some("https://[fd00::1]:8443")
        );
        assert_eq!(agent_channel_url("http://0.0.0.0:8080", 8443), None);
    }

    #[test]
    fn the_agent_websocket_sits_under_the_api_and_names_a_valid_subprotocol() {
        assert!(AGENT_WS_PATH.starts_with("/api/agent/"));
        // Un jeton HTTP (RFC 9110) : ni séparateur ni blanc, sans quoi un
        // mandataire peut refuser l'en-tête.
        let token = |b: u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b);
        assert!(AGENT_WS_PROTOCOL.bytes().all(token), "{AGENT_WS_PROTOCOL}");
    }

    #[test]
    fn a_pin_typed_with_colons_is_normalised() {
        let colons: String = PIN_A
            .as_bytes()
            .chunks(2)
            .map(|pair| std::str::from_utf8(pair).unwrap().to_uppercase())
            .collect::<Vec<_>>()
            .join(":");
        assert_eq!(normalise_pin(&colons).as_deref(), Some(PIN_A));
        assert_eq!(normalise_pin("abc"), None);
    }
}
