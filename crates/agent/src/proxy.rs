//! Mandataire sortant de l'agent, vers le serveur DumbMonit.
//!
//! Dans une entreprise, une machine ne sort souvent que par un mandataire HTTP.
//! L'agent le traverse par `CONNECT` : le mandataire ouvre un tuyau TCP vers le
//! port des agents, et la session TLS 1.3 de l'agent (certificat client,
//! autorité épinglée, échange post-quantique) va de bout en bout jusqu'au
//! serveur. Le mandataire ne voit que le nom et le port demandés, puis du
//! chiffré ; un mandataire qui déchiffre (inspection TLS) est refusé par
//! l'épinglage, comme tout intermédiaire.
//!
//! D'où vient le réglage, par ordre de priorité :
//!
//! - `proxy:` dans agent.yaml (ou `DUMBMONIT_AGENT_PROXY`) : `http://[user:mot
//!   de passe@]hôte:port`, `none` (toujours en direct) ou `system` (le
//!   mandataire système de Windows : WinHTTP, puis celui d'Internet Explorer du
//!   compte du service) ; `no_proxy:` (ou `DUMBMONIT_AGENT_NO_PROXY`) liste les
//!   hôtes joints en direct ; `proxy_user` / `proxy_password` évitent d'encoder
//!   des identifiants dans l'URL ;
//! - sinon, l'environnement : `HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY` et
//!   `NO_PROXY` (et leurs formes en minuscules), lus par le client HTTP.
//!
//! Seuls les mandataires `http://` sont acceptés : la connexion au mandataire
//! lui-même n'est pas chiffrée, c'est ce qu'elle transporte qui l'est. Un
//! mandataire `https://` demanderait de faire confiance à son certificat, ce que
//! le canal épinglé refuse par construction.
//!
//! Le réglage ne vaut que pour les connexions au serveur (mesures, commandes,
//! relais, mises à jour, sentinelle). Les sondes d'un relais vers les
//! équipements de son site et l'essai de sa passerelle partent en direct.

use std::fmt;

use anyhow::{Result, bail};

/// Ce que la configuration demande.
#[derive(Clone, Default, PartialEq, Eq)]
pub enum ProxyMode {
    /// Rien de réglé : les variables d'environnement habituelles.
    #[default]
    Environment,
    /// `none` : toujours en direct, même si l'environnement déclare un mandataire.
    Direct,
    /// `system` : le mandataire système de Windows ; ailleurs, l'environnement.
    System,
    /// Un mandataire explicite.
    Explicit(ProxyUrl),
}

/// Un mandataire `http://`, identifiants à part.
#[derive(Clone, PartialEq, Eq)]
pub struct ProxyUrl {
    /// `http://hôte:port/`, sans identifiants.
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

/// Réglage complet : mode et hôtes joints en direct.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct ProxyConfig {
    pub mode: ProxyMode,
    /// Hôtes, domaines (`.corp.lan` ou `corp.lan` couvre ses sous-domaines),
    /// adresses ou réseaux (`10.0.0.0/8`) joints sans mandataire.
    pub no_proxy: Vec<String>,
}

/// Les identifiants n'apparaissent jamais dans une trace.
impl fmt::Debug for ProxyUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyUrl")
            .field("url", &self.url)
            .field("credentials", &self.username.is_some())
            .finish()
    }
}

impl fmt::Debug for ProxyMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Environment => f.write_str("Environment"),
            Self::Direct => f.write_str("Direct"),
            Self::System => f.write_str("System"),
            Self::Explicit(url) => f.debug_tuple("Explicit").field(url).finish(),
        }
    }
}

impl fmt::Debug for ProxyConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProxyConfig")
            .field("mode", &self.mode)
            .field("no_proxy", &self.no_proxy)
            .finish()
    }
}

impl ProxyConfig {
    /// Lit `proxy`, `no_proxy`, `proxy_user` et `proxy_password`, déjà fusionnés
    /// avec l'environnement par l'appelant.
    pub fn parse(
        proxy: Option<&str>,
        no_proxy: Vec<String>,
        user: Option<&str>,
        password: Option<&str>,
    ) -> Result<Self> {
        let mode = match proxy.map(str::trim).filter(|raw| !raw.is_empty()) {
            None => ProxyMode::Environment,
            Some(raw) => match raw.to_ascii_lowercase().as_str() {
                "none" | "off" => ProxyMode::Direct,
                "system" => ProxyMode::System,
                _ => ProxyMode::Explicit(parse_url(raw, user, password)?),
            },
        };
        let has_credentials = user.is_some_and(|user| !user.trim().is_empty())
            || password.is_some_and(|password| !password.is_empty());
        if has_credentials && !matches!(mode, ProxyMode::Explicit(_)) {
            bail!("proxy_user and proxy_password need an explicit proxy (http://host:port)");
        }
        let no_proxy = no_proxy
            .iter()
            .map(|entry| entry.trim().to_string())
            .filter(|entry| !entry.is_empty())
            .collect();
        Ok(Self { mode, no_proxy })
    }

    /// Mandataire effectif : `system` résolu sur la machine (Windows), les
    /// autres tels quels.
    pub fn resolve(&self) -> ProxyConfig {
        if self.mode != ProxyMode::System {
            return self.clone();
        }
        match system_proxy() {
            Some((proxy, bypass)) => match from_windows(&proxy, &bypass) {
                Some((url, mut no_proxy)) => {
                    tracing::info!(proxy = url.as_str(), "outbound proxy: Windows system proxy");
                    no_proxy.extend(self.no_proxy.iter().cloned());
                    let target = ProxyUrl { url, username: None, password: None };
                    ProxyConfig { mode: ProxyMode::Explicit(target), no_proxy }
                }
                None => {
                    tracing::warn!(
                        proxy = proxy.as_str(),
                        "outbound proxy: Windows system proxy is not HTTP, going direct"
                    );
                    ProxyConfig { mode: ProxyMode::Direct, no_proxy: Vec::new() }
                }
            },
            None => {
                tracing::info!(
                    "outbound proxy: no Windows system proxy (netsh winhttp show proxy), \
                     using the environment"
                );
                ProxyConfig { mode: ProxyMode::Environment, no_proxy: self.no_proxy.clone() }
            }
        }
    }

    /// Applique le réglage (résolu) à un client HTTP en construction.
    pub fn apply(&self, builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder> {
        Ok(match &self.mode {
            // Le client lit lui-même HTTPS_PROXY, ALL_PROXY, NO_PROXY…
            ProxyMode::Environment | ProxyMode::System => builder,
            ProxyMode::Direct => builder.no_proxy(),
            ProxyMode::Explicit(target) => {
                let mut proxy = reqwest::Proxy::all(target.url.as_str())
                    .map_err(|error| anyhow::anyhow!("proxy {}: {error}", target.url))?;
                if let Some(user) = &target.username {
                    proxy = proxy.basic_auth(user, target.password.as_deref().unwrap_or(""));
                }
                let bypass = reqwest::NoProxy::from_string(&self.no_proxy.join(","));
                if !self.no_proxy.is_empty() {
                    proxy = proxy.no_proxy(bypass);
                }
                builder.proxy(proxy)
            }
        })
    }

    /// Ligne de journal au démarrage, sans identifiants.
    pub fn describe(&self) -> String {
        match &self.mode {
            ProxyMode::Environment => "from the environment (HTTPS_PROXY), if any".into(),
            ProxyMode::Direct => "none (direct)".into(),
            ProxyMode::System => "Windows system proxy".into(),
            ProxyMode::Explicit(target) => {
                let mut text = target.url.trim_end_matches('/').to_string();
                if target.username.is_some() {
                    text.push_str(" (with credentials)");
                }
                if !self.no_proxy.is_empty() {
                    text.push_str(&format!(", direct for {}", self.no_proxy.join(",")));
                }
                text
            }
        }
    }
}

/// Analyse `http://[user:mot de passe@]hôte[:port]`. Les identifiants de l'URL
/// (encodés en pourcentage) ou ceux des clés à part sont séparés de l'adresse.
fn parse_url(raw: &str, user: Option<&str>, password: Option<&str>) -> Result<ProxyUrl> {
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("https://") {
        bail!(
            "proxy: only http:// proxies are supported (got an https:// address). The agent \
             tunnels its own end-to-end TLS through CONNECT: give the proxy's http:// address, \
             the traffic inside stays encrypted"
        );
    }
    if !lower.starts_with("http://") {
        bail!("proxy: http://host:port, none or system expected (got '{}')", redact(raw));
    }
    let mut url = reqwest::Url::parse(raw)
        .map_err(|error| anyhow::anyhow!("proxy: invalid address '{}': {error}", redact(raw)))?;
    if url.host_str().is_none_or(str::is_empty) {
        bail!("proxy: the address has no host ('{}')", redact(raw));
    }
    if !matches!(url.path(), "" | "/") || url.query().is_some() {
        bail!("proxy: http://host:port only, without a path ('{}')", redact(raw));
    }
    let from_url_user = (!url.username().is_empty()).then(|| percent_decode(url.username()));
    let from_url_password = url.password().map(percent_decode);
    let username = user
        .map(str::trim)
        .filter(|user| !user.is_empty())
        .map(str::to_string)
        .or(from_url_user);
    let password = password.filter(|p| !p.is_empty()).map(str::to_string).or(from_url_password);
    if password.is_some() && username.is_none() {
        bail!("proxy: a password without a user name");
    }
    let _ = url.set_username("");
    let _ = url.set_password(None);
    Ok(ProxyUrl { url: url.to_string(), username, password })
}

/// L'adresse sans ses identifiants, pour les messages d'erreur.
fn redact(raw: &str) -> String {
    match (raw.find("://"), raw.rfind('@')) {
        (Some(scheme), Some(at)) if at > scheme => {
            format!("{}<credentials>{}", &raw[..scheme + 3], &raw[at..])
        }
        _ => raw.to_string(),
    }
}

/// Décodage `%XX` d'un composant d'URL (identifiants).
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = bytes
            .get(i + 1..i + 3)
            .filter(|_| bytes[i] == b'%')
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escaped {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Mandataire système de Windows, tel que WinHTTP le donne (adresse et liste
/// de contournement) : `None` s'il n'y en a pas, ou hors de Windows.
#[cfg(windows)]
fn system_proxy() -> Option<(String, String)> {
    windows_system::lookup()
}

#[cfg(not(windows))]
fn system_proxy() -> Option<(String, String)> {
    None
}

/// Traduit le réglage de Windows : `proxy:3128`, `http=proxy:3128;https=…`,
/// et une liste de contournement `<local>;*.corp.lan;intranet`. Préfère
/// l'entrée `https=` (c'est vers un port TLS que l'agent va), puis `http=`, puis
/// une entrée sans protocole ; écarte `socks=`. Dans la liste, `*.x` devient
/// `x` (les sous-domaines suivent) ; `<local>` et les autres jokers, que le
/// client ne sait pas exprimer, sont laissés de côté.
pub fn from_windows(proxy: &str, bypass: &str) -> Option<(String, Vec<String>)> {
    let entries: Vec<&str> =
        proxy.split([';', ' ']).map(str::trim).filter(|entry| !entry.is_empty()).collect();
    let pick = |prefix: &str| {
        entries.iter().find_map(|entry| {
            let (key, value) = entry.split_once('=')?;
            key.trim().eq_ignore_ascii_case(prefix).then_some(value.trim())
        })
    };
    let chosen = pick("https")
        .or_else(|| pick("http"))
        .or_else(|| entries.iter().copied().find(|entry| !entry.contains('=')))?;
    let address = chosen.strip_prefix("http://").unwrap_or(chosen);
    if address.is_empty() || address.contains("://") || address.contains('/') {
        return None;
    }
    let no_proxy = bypass
        .split([';', ' ', ','])
        .map(str::trim)
        .filter(|entry| !entry.is_empty() && !entry.eq_ignore_ascii_case("<local>"))
        .filter_map(|entry| {
            let entry = entry.strip_prefix("*.").unwrap_or(entry);
            (!entry.contains('*')).then(|| entry.to_string())
        })
        .collect();
    Some((format!("http://{address}"), no_proxy))
}

#[cfg(windows)]
mod windows_system {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::Networking::WinHttp::{
        WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_PROXY_INFO,
        WinHttpGetDefaultProxyConfiguration, WinHttpGetIEProxyConfigForCurrentUser,
    };

    /// Lit puis libère une chaîne que WinHTTP a allouée (`GlobalFree`).
    ///
    /// # Safety
    ///
    /// `text` est nul ou une chaîne UTF-16 terminée par zéro, allouée par
    /// WinHTTP et pas encore libérée.
    unsafe fn take(text: *mut u16) -> String {
        if text.is_null() {
            return String::new();
        }
        let mut len = 0;
        // SAFETY : chaîne terminée par zéro, d'après le contrat ci-dessus.
        while unsafe { *text.add(len) } != 0 {
            len += 1;
        }
        // SAFETY : `len` unités lisibles, comptées juste au-dessus.
        let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, len) });
        // SAFETY : allouée par WinHTTP, libérée une seule fois.
        unsafe { GlobalFree(text.cast()) };
        value
    }

    /// Mandataire WinHTTP (`netsh winhttp set proxy`, celui des services), puis
    /// celui d'Internet Explorer du compte qui exécute l'agent. Une
    /// configuration automatique (PAC, WPAD) n'est pas suivie.
    pub fn lookup() -> Option<(String, String)> {
        let mut info = WINHTTP_PROXY_INFO::default();
        // SAFETY : structure valide, remplie par l'appel ; ses chaînes sont
        // libérées par `take`.
        if unsafe { WinHttpGetDefaultProxyConfiguration(&mut info) } != 0 {
            // SAFETY : chaînes rendues par WinHTTP, chacune libérée une fois.
            let proxy = unsafe { take(info.lpszProxy) };
            let bypass = unsafe { take(info.lpszProxyBypass) };
            if !proxy.trim().is_empty() {
                return Some((proxy, bypass));
            }
        }
        let mut ie = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
        // SAFETY : comme ci-dessus.
        if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&mut ie) } != 0 {
            // SAFETY : chaînes rendues par WinHTTP, chacune libérée une fois.
            let _ = unsafe { take(ie.lpszAutoConfigUrl) };
            let proxy = unsafe { take(ie.lpszProxy) };
            let bypass = unsafe { take(ie.lpszProxyBypass) };
            if !proxy.trim().is_empty() {
                return Some((proxy, bypass));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> Result<ProxyConfig> {
        ProxyConfig::parse(Some(raw), Vec::new(), None, None)
    }

    fn explicit(config: &ProxyConfig) -> &ProxyUrl {
        match &config.mode {
            ProxyMode::Explicit(url) => url,
            other => panic!("mandataire explicite attendu : {other:?}"),
        }
    }

    fn mode(raw: Option<&str>) -> ProxyMode {
        ProxyConfig::parse(raw, Vec::new(), None, None).unwrap().mode
    }

    #[test]
    fn nothing_set_means_the_environment_and_none_means_direct() {
        assert_eq!(mode(None), ProxyMode::Environment);
        assert_eq!(mode(Some("  ")), ProxyMode::Environment);
        assert_eq!(mode(Some("none")), ProxyMode::Direct);
        assert_eq!(mode(Some("OFF")), ProxyMode::Direct);
        assert_eq!(mode(Some("System")), ProxyMode::System);
    }

    #[test]
    fn credentials_come_from_the_url_or_their_own_keys_and_never_show() {
        let config = parse("http://jo%40corp:p%3Ass@proxy.corp:3128").unwrap();
        let url = explicit(&config);
        assert_eq!(url.url, "http://proxy.corp:3128/");
        assert_eq!(url.username.as_deref(), Some("jo@corp"));
        assert_eq!(url.password.as_deref(), Some("p:ss"));
        let shown = format!("{config:?} {}", config.describe());
        assert!(!shown.contains("p:ss") && !shown.contains("p%3Ass"), "{shown}");
        assert!(!shown.contains("jo@corp") && !shown.contains("jo%40corp"), "{shown}");

        // Les clés à part priment, sans rien encoder.
        let config = ProxyConfig::parse(
            Some("http://proxy.corp:3128"),
            Vec::new(),
            Some("DOMAIN\\jo"),
            Some("s3cr&t"),
        )
        .unwrap();
        let url = explicit(&config);
        assert_eq!(url.username.as_deref(), Some("DOMAIN\\jo"));
        assert_eq!(url.password.as_deref(), Some("s3cr&t"));
    }

    #[test]
    fn only_a_plain_http_proxy_address_is_accepted() {
        for refused in [
            "https://proxy.corp:3128",
            "socks5://proxy.corp:1080",
            "proxy.corp:3128",
            "http://proxy.corp:3128/path",
            "http://",
        ] {
            assert!(parse(refused).is_err(), "{refused}");
        }
        let error = parse("https://u:secret@p:1").unwrap_err().to_string();
        assert!(error.contains("http://") && !error.contains("secret"), "{error}");
        let error = parse("ftp://u:secret@p:1").unwrap_err().to_string();
        assert!(!error.contains("secret"), "{error}");
        // Des identifiants sans mandataire explicite ne mènent nulle part.
        assert!(ProxyConfig::parse(None, Vec::new(), Some("jo"), Some("x")).is_err());
        assert!(ProxyConfig::parse(Some("http://p:1"), Vec::new(), None, Some("x")).is_err());
    }

    #[test]
    fn the_hosts_reached_directly_are_cleaned() {
        let config = ProxyConfig::parse(
            Some("http://proxy:3128"),
            vec![" .corp.lan ".into(), "".into(), "10.0.0.0/8".into()],
            None,
            None,
        )
        .unwrap();
        assert_eq!(config.no_proxy, [".corp.lan", "10.0.0.0/8"]);
        assert!(config.describe().ends_with(", direct for .corp.lan,10.0.0.0/8"));
    }

    #[test]
    fn every_mode_builds_a_client() {
        let bypass = vec!["corp.lan".to_string()];
        let explicit = ProxyConfig::parse(Some("http://u:p@proxy:3128"), bypass, None, None);
        for config in [ProxyConfig::default(), parse("none").unwrap(), explicit.unwrap()] {
            let builder = config.resolve().apply(reqwest::Client::builder()).expect("réglage");
            builder.build().expect("client");
        }
    }

    #[test]
    fn the_windows_system_proxy_is_translated() {
        assert_eq!(
            from_windows("proxy.corp:3128", "<local>;*.corp.lan;intranet;10.*"),
            Some(("http://proxy.corp:3128".into(), vec!["corp.lan".into(), "intranet".into()]))
        );
        // Un réglage par protocole : HTTPS d'abord, c'est un port TLS que l'agent vise.
        assert_eq!(
            from_windows("http=p1:80;https=p2:8080;socks=s:1080", ""),
            Some(("http://p2:8080".into(), vec![]))
        );
        assert_eq!(
            from_windows("http=http://p1:80", "").map(|proxy| proxy.0).as_deref(),
            Some("http://p1:80")
        );
        assert_eq!(from_windows("socks=s:1080", ""), None);
        assert_eq!(from_windows("", ""), None);
    }

    #[test]
    fn percent_encoded_credentials_are_decoded() {
        assert_eq!(percent_decode("a%20b%2"), "a b%2");
        assert_eq!(percent_decode("%41%zz"), "A%zz");
        assert_eq!(redact("http://u:p@h:1"), "http://<credentials>@h:1");
        assert_eq!(redact("http://h:1"), "http://h:1");
    }
}
