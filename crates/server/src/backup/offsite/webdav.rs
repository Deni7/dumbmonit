//! Destination WebDAV : `PUT` du fichier dans le dossier, `PROPFIND`
//! (profondeur 1) pour le lister, `DELETE` pour élaguer. Authentification
//! HTTP basique, comme Nextcloud la demande avec un mot de passe
//! d'application.

use std::path::Path;
use std::time::Duration;

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE};
use reqwest::{Method, StatusCode, Url};

use super::transfer::{self, Failure, SHORT_TIMEOUT, TRANSFER_TIMEOUT};
use super::{WebDavSettings, check_name};

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:resourcetype/></d:prop></d:propfind>"#;

pub struct Client {
    settings: WebDavSettings,
    password: Option<String>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(settings: WebDavSettings, password: Option<String>) -> Result<Self, Failure> {
        Ok(Self { settings, password, http: transfer::http_client()? })
    }

    fn collection(&self) -> Result<Url, Failure> {
        let mut raw = self.settings.url.clone();
        if !raw.ends_with('/') {
            raw.push('/');
        }
        Url::parse(&raw).map_err(|error| Failure::fatal(format!("WebDAV address {raw}: {error}")))
    }

    fn file_url(&self, name: &str) -> Result<Url, Failure> {
        check_name(name)?;
        self.collection()?
            .join(name)
            .map_err(|error| Failure::fatal(format!("WebDAV address of {name}: {error}")))
    }

    fn request(&self, method: Method, url: Url, timeout: Duration) -> reqwest::RequestBuilder {
        let request = self.http.request(method, url).timeout(timeout);
        if self.settings.username.is_empty() {
            request
        } else {
            request.basic_auth(&self.settings.username, self.password.as_deref())
        }
    }

    /// Une réponse hors 2xx, dite pour l'administrateur.
    async fn check(
        &self,
        response: reqwest::Response,
        what: &str,
    ) -> Result<reqwest::Response, Failure> {
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let detail = match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                "the server refused the user name or password (Nextcloud needs an app password)"
                    .to_string()
            }
            StatusCode::NOT_FOUND | StatusCode::CONFLICT => {
                format!("the folder {} does not exist on the server", self.settings.url)
            }
            StatusCode::INSUFFICIENT_STORAGE => "the server is out of space".to_string(),
            _ => transfer::one_line(&transfer::read_snippet(response).await, 200),
        };
        Err(Failure::status(what, status, &detail))
    }

    pub async fn put_file(&self, name: &str, path: &Path) -> Result<u64, Failure> {
        let url = self.file_url(name)?;
        let size = transfer::file_size(path).await?;
        let file = transfer::open(path).await?;
        let what = format!("uploading {name}");
        let response = self
            .request(Method::PUT, url, TRANSFER_TIMEOUT)
            .header(CONTENT_LENGTH, size)
            .body(transfer::file_body(file))
            .send()
            .await
            .map_err(|error| Failure::request(&what, &error))?;
        self.check(response, &what).await?;
        Ok(size)
    }

    pub async fn put_bytes(&self, name: &str, data: Vec<u8>) -> Result<(), Failure> {
        let url = self.file_url(name)?;
        let what = format!("writing {name}");
        let response = self
            .request(Method::PUT, url, SHORT_TIMEOUT)
            .body(data)
            .send()
            .await
            .map_err(|error| Failure::request(&what, &error))?;
        self.check(response, &what).await?;
        Ok(())
    }

    /// Noms des fichiers du dossier (les sous-dossiers sont ignorés).
    pub async fn list(&self) -> Result<Vec<String>, Failure> {
        let what = "listing the folder";
        let propfind = Method::from_bytes(b"PROPFIND").expect("PROPFIND is a valid method");
        let response = self
            .request(propfind, self.collection()?, SHORT_TIMEOUT)
            .header("Depth", "1")
            .header(CONTENT_TYPE, "application/xml; charset=utf-8")
            .body(PROPFIND_BODY)
            .send()
            .await
            .map_err(|error| Failure::request(what, &error))?;
        let response = self.check(response, what).await?;
        let body = response.text().await.map_err(|error| Failure::request(what, &error))?;
        parse_multistatus(&body)
    }

    pub async fn delete(&self, name: &str) -> Result<(), Failure> {
        let url = self.file_url(name)?;
        let what = format!("deleting {name}");
        let response = self
            .request(Method::DELETE, url, SHORT_TIMEOUT)
            .send()
            .await
            .map_err(|error| Failure::request(&what, &error))?;
        // Déjà parti : c'est ce qu'on voulait.
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        self.check(response, &what).await?;
        Ok(())
    }
}

/// Noms des fichiers d'une réponse `207 Multi-Status` : le dernier segment de
/// chaque `href`, décodé ; les collections (terminées par `/`) sont écartées,
/// le dossier lui-même compris.
pub fn parse_multistatus(body: &str) -> Result<Vec<String>, Failure> {
    let document = roxmltree::Document::parse(body)
        .map_err(|error| Failure::fatal(format!("the folder listing is not XML: {error}")))?;
    let mut names = Vec::new();
    for response in document.descendants() {
        if !response.is_element() || response.tag_name().name() != "response" {
            continue;
        }
        let Some(href) = response
            .descendants()
            .find(|n| n.is_element() && n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .map(str::trim)
        else {
            continue;
        };
        if href.ends_with('/') {
            continue;
        }
        let last = href.rsplit('/').next().unwrap_or(href);
        let name = transfer::percent_decode(last);
        if !name.is_empty() {
            names.push(name);
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nextcloud_listing_gives_the_file_names() {
        let body = r#"<?xml version="1.0"?>
            <d:multistatus xmlns:d="DAV:" xmlns:oc="http://owncloud.org/ns">
              <d:response>
                <d:href>/remote.php/dav/files/me/Backups/</d:href>
                <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop>
                <d:status>HTTP/1.1 200 OK</d:status></d:propstat>
              </d:response>
              <d:response>
                <d:href>/remote.php/dav/files/me/Backups/dumbmonit-20260922-190000.db</d:href>
                <d:propstat><d:prop><d:resourcetype/></d:prop></d:propstat>
              </d:response>
              <d:response>
                <d:href>https://cloud.example/remote.php/dav/files/me/Backups/a%20b.txt</d:href>
              </d:response>
              <d:response><d:href>/remote.php/dav/files/me/Backups/old/</d:href></d:response>
            </d:multistatus>"#;
        assert_eq!(
            parse_multistatus(body).unwrap(),
            vec!["dumbmonit-20260922-190000.db".to_string(), "a b.txt".to_string()]
        );
    }

    #[test]
    fn a_listing_with_another_prefix_is_read_too() {
        let body = r#"<multistatus xmlns="DAV:"><response><href>/dav/x.db</href></response>
            </multistatus>"#;
        assert_eq!(parse_multistatus(body).unwrap(), vec!["x.db".to_string()]);
        assert!(parse_multistatus("not xml").is_err());
    }
}
