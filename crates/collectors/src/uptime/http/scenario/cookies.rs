//! Magasin de cookies d'un scénario, d'une étape à l'autre.
//!
//! Écrit ici plutôt que pris dans `reqwest` (fonctionnalité `cookies`) : la
//! sonde n'a besoin que d'une fraction de la RFC 6265 — garder ce qu'une
//! réponse pose, le renvoyer aux requêtes suivantes du même site — et le
//! magasin ne vit que le temps d'une sonde. Il est neuf à chaque exécution : une
//! session ouverte par un passage n'est jamais réutilisée par le suivant, de
//! sorte que chaque contrôle refait vraiment la connexion.
//!
//! Les règles retenues :
//!
//! * un cookie sans `Domain` ne repart que vers l'hôte exact qui l'a posé ;
//!   avec `Domain`, vers ce domaine et ses sous-domaines, à condition que l'hôte
//!   qui le pose en fasse partie (un site ne pose pas de cookie pour un autre) ;
//! * `Path` restreint les chemins, `Secure` réserve le cookie à HTTPS ;
//! * `Max-Age` nul ou négatif, ou `Expires` passé, efface le cookie.

use std::net::IpAddr;

use reqwest::Url;
use reqwest::header::{HeaderMap, SET_COOKIE};

/// Cookies gardés au plus : une réponse qui en poserait des milliers ne doit pas
/// faire grossir la sonde.
const MAX_COOKIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Cookie {
    name: String,
    value: String,
    /// En minuscules, sans point initial.
    domain: String,
    /// Sans attribut `Domain` : seul l'hôte exact le reçoit.
    host_only: bool,
    path: String,
    secure: bool,
}

#[derive(Debug, Default)]
pub struct CookieJar {
    cookies: Vec<Cookie>,
}

impl CookieJar {
    /// Retient les `Set-Cookie` d'une réponse à `url`.
    pub fn store(&mut self, url: &Url, headers: &HeaderMap) {
        for value in headers.get_all(SET_COOKIE) {
            if let Ok(text) = value.to_str() {
                self.store_one(url, text);
            }
        }
    }

    fn store_one(&mut self, url: &Url, text: &str) {
        let Some(host) = host_of(url) else { return };
        let mut attributes = text.split(';');
        let Some((name, value)) = attributes.next().and_then(|pair| pair.split_once('=')) else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }

        let mut domain = None;
        let mut path = None;
        let mut secure = false;
        let mut max_age: Option<i64> = None;
        let mut expires_past = false;
        for attribute in attributes {
            let (key, raw) = match attribute.split_once('=') {
                Some((key, raw)) => (key.trim(), raw.trim()),
                None => (attribute.trim(), ""),
            };
            match key.to_ascii_lowercase().as_str() {
                "domain" => {
                    let wanted = raw.trim_start_matches('.').to_ascii_lowercase();
                    if !wanted.is_empty() {
                        domain = Some(wanted);
                    }
                }
                "path" if raw.starts_with('/') => path = Some(raw.to_string()),
                "secure" => secure = true,
                "max-age" => max_age = raw.parse().ok(),
                "expires" => {
                    expires_past = chrono::DateTime::parse_from_rfc2822(raw)
                        .is_ok_and(|when| when.with_timezone(&chrono::Utc) < chrono::Utc::now());
                }
                _ => {}
            }
        }
        // `Max-Age` l'emporte sur `Expires` quand les deux sont donnés.
        let expired = max_age.map_or(expires_past, |seconds| seconds <= 0);

        let (domain, host_only) = match domain {
            Some(domain) if domain_matches(&host, &domain) => (domain, false),
            // Un domaine qui ne contient pas l'hôte : le cookie est ignoré.
            Some(_) => return,
            None => (host, true),
        };
        let cookie = Cookie {
            name: name.to_string(),
            value: value.trim().to_string(),
            domain,
            host_only,
            path: path.unwrap_or_else(|| default_path(url)),
            secure,
        };

        self.cookies.retain(|known| {
            known.name != cookie.name || known.domain != cookie.domain || known.path != cookie.path
        });
        if !expired && self.cookies.len() < MAX_COOKIES {
            self.cookies.push(cookie);
        }
    }

    /// L'en-tête `Cookie` d'une requête vers `url`, `None` s'il n'y a rien à envoyer.
    pub fn header_for(&self, url: &Url) -> Option<String> {
        let host = host_of(url)?;
        let secure = url.scheme() == "https";
        let path = url.path();
        let pairs: Vec<String> = self
            .cookies
            .iter()
            .filter(|cookie| {
                let host_ok = if cookie.host_only {
                    host == cookie.domain
                } else {
                    domain_matches(&host, &cookie.domain)
                };
                host_ok && path_matches(path, &cookie.path) && (secure || !cookie.secure)
            })
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect();
        (!pairs.is_empty()).then(|| pairs.join("; "))
    }

    #[cfg(test)]
    pub fn count(&self) -> usize {
        self.cookies.len()
    }
}

fn host_of(url: &Url) -> Option<String> {
    url.host_str().map(|host| host.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase())
}

/// `host` est `domain` ou l'un de ses sous-domaines. Une adresse IP ne se
/// rattache qu'à elle-même.
fn domain_matches(host: &str, domain: &str) -> bool {
    if host == domain {
        return true;
    }
    host.parse::<IpAddr>().is_err()
        && host.len() > domain.len()
        && host.ends_with(domain)
        && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
}

/// Chemin par défaut d'un cookie : celui de la requête, jusqu'au dernier `/` exclu.
fn default_path(url: &Url) -> String {
    let path = url.path();
    match path.rfind('/') {
        Some(0) | None => "/".to_string(),
        Some(end) => path[..end].to_string(),
    }
}

fn path_matches(request: &str, cookie: &str) -> bool {
    request == cookie
        || (request.starts_with(cookie)
            && (cookie.ends_with('/') || request[cookie.len()..].starts_with('/')))
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn set(jar: &mut CookieJar, from: &str, cookies: &[&str]) {
        let mut headers = HeaderMap::new();
        for cookie in cookies {
            headers.append(SET_COOKIE, HeaderValue::from_str(cookie).unwrap());
        }
        jar.store(&url(from), &headers);
    }

    #[test]
    fn un_cookie_pose_revient_vers_le_meme_hote() {
        let mut jar = CookieJar::default();
        set(&mut jar, "https://app.example.com/login", &["session=abc; Path=/; HttpOnly"]);
        let session = jar.header_for(&url("https://app.example.com/me"));
        assert_eq!(session.as_deref(), Some("session=abc"));
        assert_eq!(jar.header_for(&url("https://autre.example.com/me")), None);
        assert_eq!(jar.header_for(&url("https://sous.app.example.com/me")), None, "host-only");
    }

    #[test]
    fn un_cookie_de_domaine_suit_les_sous_domaines_mais_pas_les_voisins() {
        let mut jar = CookieJar::default();
        set(&mut jar, "https://login.example.com/", &["sso=1; Domain=.example.com; Path=/"]);
        assert_eq!(jar.header_for(&url("https://app.example.com/")).as_deref(), Some("sso=1"));
        assert_eq!(jar.header_for(&url("https://example.com/")).as_deref(), Some("sso=1"));
        assert_eq!(jar.header_for(&url("https://badexample.com/")), None);

        // Un site ne pose pas de cookie pour un autre.
        set(&mut jar, "https://evil.test/", &["vol=1; Domain=example.com"]);
        assert_eq!(jar.count(), 1);
    }

    #[test]
    fn le_chemin_et_secure_restreignent_lenvoi() {
        let mut jar = CookieJar::default();
        set(&mut jar, "https://a.test/app/login", &["p=1", "s=2; Secure; Path=/"]);
        // Sans `Path`, le chemin par défaut est celui de la requête : `/app`.
        assert_eq!(jar.header_for(&url("https://a.test/app/x")).as_deref(), Some("p=1; s=2"));
        assert_eq!(jar.header_for(&url("https://a.test/apples")).as_deref(), Some("s=2"));
        assert_eq!(jar.header_for(&url("http://a.test/app/x")).as_deref(), Some("p=1"));
    }

    #[test]
    fn un_cookie_remplace_ou_efface_le_precedent() {
        let mut jar = CookieJar::default();
        set(&mut jar, "https://a.test/", &["id=1; Path=/"]);
        set(&mut jar, "https://a.test/", &["id=2; Path=/"]);
        assert_eq!(jar.header_for(&url("https://a.test/")).as_deref(), Some("id=2"));
        set(&mut jar, "https://a.test/", &["id=; Path=/; Max-Age=0"]);
        assert_eq!(jar.header_for(&url("https://a.test/")), None);
        let old = "old=1; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT";
        set(&mut jar, "https://a.test/", &[old]);
        assert_eq!(jar.count(), 0);
    }

    #[test]
    fn un_set_cookie_illisible_est_ignore() {
        let mut jar = CookieJar::default();
        set(&mut jar, "https://a.test/", &["sans-egal", "=valeur"]);
        assert_eq!(jar.count(), 0);
    }
}
