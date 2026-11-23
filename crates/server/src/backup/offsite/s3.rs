//! Destination S3 et compatibles : un `PUT` signé par objet, la liste par
//! `ListObjectsV2` (pour l'élagage), `DELETE`.
//!
//! Le corps est signé (`x-amz-content-sha256` = l'empreinte réelle du
//! fichier, calculée d'abord par une lecture à part) : tous les compatibles
//! l'acceptent, ce qui n'est pas le cas de `UNSIGNED-PAYLOAD`, et le serveur
//! vérifie ainsi l'intégrité de ce qu'il reçoit. Un seul `PUT` par fichier :
//! jusqu'à 5 Gio, bien au-delà d'une base SQLite de supervision.

use std::path::Path;
use std::time::Duration;

use reqwest::header::{AUTHORIZATION, CONTENT_LENGTH};
use reqwest::{Method, Url};

use super::sigv4::{self, EMPTY_SHA256};
use super::transfer::{self, Failure, MAX_LISTING_BYTES, SHORT_TIMEOUT, TRANSFER_TIMEOUT};
use super::{S3Settings, check_name};

/// Plus gros objet qu'un seul `PUT` accepte.
const MAX_SINGLE_PUT: u64 = 5 * 1024 * 1024 * 1024;
/// Pages de liste au plus (1 000 objets chacune).
const MAX_PAGES: usize = 100;
const DEFAULT_REGION: &str = "us-east-1";

pub struct Client {
    settings: S3Settings,
    secret_key: String,
    http: reqwest::Client,
}

/// Où part une requête : URL, en-tête `Host` signé, chemin canonique.
struct Location {
    url: Url,
    authority: String,
    path: String,
}

impl Client {
    pub fn new(settings: S3Settings, secret_key: String) -> Result<Self, Failure> {
        Ok(Self { settings, secret_key, http: transfer::http_client()? })
    }

    fn region(&self) -> &str {
        if self.settings.region.is_empty() { DEFAULT_REGION } else { &self.settings.region }
    }

    fn endpoint(&self) -> Result<Url, Failure> {
        let raw = if self.settings.endpoint.is_empty() {
            format!("https://s3.{}.amazonaws.com", self.region())
        } else {
            self.settings.endpoint.clone()
        };
        Url::parse(&raw).map_err(|error| Failure::fatal(format!("S3 endpoint {raw}: {error}")))
    }

    fn object_key(&self, name: &str) -> String {
        if self.settings.prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{name}", self.settings.prefix)
        }
    }

    /// `key` absent : le seau lui-même (liste).
    fn location(&self, key: Option<&str>, query: &[(String, String)]) -> Result<Location, Failure> {
        let base = self.endpoint()?;
        let host = base.host_str().ok_or_else(|| Failure::fatal("the S3 endpoint has no host"))?;
        let port = base.port().map(|port| format!(":{port}")).unwrap_or_default();
        let key = key.map(|key| sigv4::uri_encode(key, false)).unwrap_or_default();
        let bucket = sigv4::uri_encode(&self.settings.bucket, true);
        let (authority, path) = if self.settings.path_style {
            (format!("{host}{port}"), format!("/{bucket}/{key}"))
        } else {
            (format!("{bucket}.{host}{port}"), format!("/{key}"))
        };
        let mut raw = format!("{}://{authority}{path}", base.scheme());
        let query = sigv4::canonical_query(query);
        if !query.is_empty() {
            raw.push('?');
            raw.push_str(&query);
        }
        let url = Url::parse(&raw)
            .map_err(|error| Failure::fatal(format!("S3 address {raw}: {error}")))?;
        Ok(Location { url, authority, path })
    }

    /// Envoie une requête signée ; une réponse hors 2xx devient une erreur
    /// lisible (code et message d'S3, et une piste quand on la connaît).
    #[allow(clippy::too_many_arguments)]
    async fn send(
        &self,
        method: Method,
        key: Option<&str>,
        query: Vec<(String, String)>,
        payload_hash: &str,
        body: Option<(reqwest::Body, u64)>,
        timeout: Duration,
        what: &str,
    ) -> Result<reqwest::Response, Failure> {
        let location = self.location(key, &query)?;
        let amz_date = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let headers = vec![
            ("host".to_string(), location.authority.clone()),
            ("x-amz-content-sha256".to_string(), payload_hash.to_string()),
            ("x-amz-date".to_string(), amz_date.clone()),
        ];
        let signer = sigv4::Signer {
            access_key: &self.settings.access_key_id,
            secret_key: &self.secret_key,
            region: self.region(),
            service: "s3",
        };
        let signed = sigv4::Request {
            method: method.as_str(),
            canonical_uri: &location.path,
            query: &query,
            headers: &headers,
            payload_hash,
        };
        let authorization = signer.authorization(&signed, &amz_date);

        let mut request = self
            .http
            .request(method, location.url)
            .header("x-amz-content-sha256", payload_hash)
            .header("x-amz-date", &amz_date)
            .header(AUTHORIZATION, authorization)
            .timeout(timeout);
        if let Some((body, length)) = body {
            request = request.header(CONTENT_LENGTH, length).body(body);
        }
        let response = request.send().await.map_err(|error| Failure::request(what, &error))?;
        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }
        let text = transfer::read_snippet(response).await;
        Err(Failure::status(what, status, &describe_error(&text)))
    }

    /// Envoie un fichier local sous `name`. Rend sa taille.
    pub async fn put_file(&self, name: &str, path: &Path) -> Result<u64, Failure> {
        check_name(name)?;
        let size = transfer::file_size(path).await?;
        if size > MAX_SINGLE_PUT {
            return Err(Failure::fatal(format!(
                "{name} is larger than 5 GiB, the most one S3 upload takes"
            )));
        }
        let hash = transfer::sha256_file(path).await?;
        let file = transfer::open(path).await?;
        let what = format!("uploading {name}");
        let body = Some((transfer::file_body(file), size));
        let key = self.object_key(name);
        self.send(Method::PUT, Some(&key), Vec::new(), &hash, body, TRANSFER_TIMEOUT, &what)
            .await?;
        Ok(size)
    }

    pub async fn put_bytes(&self, name: &str, data: Vec<u8>) -> Result<(), Failure> {
        check_name(name)?;
        let hash = sigv4::sha256_hex(&data);
        let length = data.len() as u64;
        let key = self.object_key(name);
        let body = Some((reqwest::Body::from(data), length));
        let what = format!("writing {name}");
        self.send(Method::PUT, Some(&key), Vec::new(), &hash, body, SHORT_TIMEOUT, &what).await?;
        Ok(())
    }

    /// Noms des objets directement sous le préfixe.
    pub async fn list(&self) -> Result<Vec<String>, Failure> {
        let prefix = if self.settings.prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.settings.prefix)
        };
        let mut names = Vec::new();
        let mut token: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![
                ("list-type".to_string(), "2".to_string()),
                ("delimiter".to_string(), "/".to_string()),
            ];
            if !prefix.is_empty() {
                query.push(("prefix".to_string(), prefix.clone()));
            }
            if let Some(token) = &token {
                query.push(("continuation-token".to_string(), token.clone()));
            }
            let what = "listing the bucket";
            let response = self
                .send(Method::GET, None, query, EMPTY_SHA256, None, SHORT_TIMEOUT, what)
                .await?;
            let body = transfer::read_listing(response, what, MAX_LISTING_BYTES).await?;
            let page = parse_list(&body, &prefix)?;
            names.extend(page.names);
            match page.next {
                Some(next) if page.truncated => token = Some(next),
                _ => break,
            }
        }
        Ok(names)
    }

    pub async fn delete(&self, name: &str) -> Result<(), Failure> {
        check_name(name)?;
        let key = self.object_key(name);
        let what = format!("deleting {name}");
        self.send(Method::DELETE, Some(&key), Vec::new(), EMPTY_SHA256, None, SHORT_TIMEOUT, &what)
            .await?;
        Ok(())
    }
}

/// Une page de `ListObjectsV2`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ListPage {
    pub names: Vec<String>,
    pub truncated: bool,
    pub next: Option<String>,
}

fn child_text(node: roxmltree::Node<'_, '_>, tag: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == tag)
        .and_then(|child| child.text())
        .map(|text| text.trim().to_string())
}

/// Lit une page de liste : les clés sous `prefix`, sans sous-dossier.
pub fn parse_list(body: &str, prefix: &str) -> Result<ListPage, Failure> {
    let document = roxmltree::Document::parse(body)
        .map_err(|error| Failure::fatal(format!("the bucket listing is not XML: {error}")))?;
    let root = document.root_element();
    let mut names = Vec::new();
    for contents in root.children() {
        if contents.is_element()
            && contents.tag_name().name() == "Contents"
            && let Some(key) = child_text(contents, "Key")
            && let Some(name) = key.strip_prefix(prefix)
            && !name.is_empty()
            && !name.contains('/')
        {
            names.push(name.to_string());
        }
    }
    Ok(ListPage {
        names,
        truncated: child_text(root, "IsTruncated").is_some_and(|value| value == "true"),
        next: child_text(root, "NextContinuationToken"),
    })
}

/// Message d'une réponse d'erreur S3 (`<Error><Code>…<Message>…`), avec une
/// piste pour les erreurs de réglage les plus courantes.
pub fn describe_error(body: &str) -> String {
    let Ok(document) = roxmltree::Document::parse(body) else {
        return transfer::one_line(body, 200);
    };
    let root = document.root_element();
    let code = child_text(root, "Code").unwrap_or_default();
    let message = child_text(root, "Message").unwrap_or_default();
    let hint = match code.as_str() {
        "SignatureDoesNotMatch" => " Check the secret access key.",
        "InvalidAccessKeyId" => " Check the access key ID.",
        "NoSuchBucket" => " Check the bucket name.",
        "AuthorizationHeaderMalformed"
        | "PermanentRedirect"
        | "IllegalLocationConstraintException" => " Check the region.",
        "AccessDenied" => {
            " The key needs permission to put, list and delete objects in this bucket."
        }
        _ => "",
    };
    let text = match (code.is_empty(), message.is_empty()) {
        (true, true) => return transfer::one_line(body, 200),
        (false, true) => code,
        (true, false) => message,
        (false, false) => format!("{code}: {message}"),
    };
    format!("{}{hint}", transfer::one_line(&text, 200))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_listing_keeps_the_names_under_the_prefix_only() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
            <ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
              <Name>backups</Name><Prefix>home/</Prefix>
              <Contents><Key>home/dumbmonit-20260922-190000.db</Key><Size>10</Size></Contents>
              <Contents><Key>home/dumbmonit-20260922-190000.key</Key></Contents>
              <Contents><Key>home/</Key></Contents>
              <CommonPrefixes><Prefix>home/old/</Prefix></CommonPrefixes>
              <IsTruncated>true</IsTruncated>
              <NextContinuationToken>1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM=</NextContinuationToken>
            </ListBucketResult>"#;
        let page = parse_list(body, "home/").unwrap();
        assert_eq!(
            page.names,
            vec!["dumbmonit-20260922-190000.db", "dumbmonit-20260922-190000.key"]
        );
        assert!(page.truncated);
        assert_eq!(page.next.as_deref(), Some("1ueGcxLPRx1Tr/XYExHnhbYLgveDs2J/wm36Hy4vbOwM="));
    }

    #[test]
    fn s3_errors_are_read_with_a_hint() {
        let body = "<Error><Code>SignatureDoesNotMatch</Code><Message>The request signature we \
                    calculated does not match.</Message></Error>";
        let text = describe_error(body);
        assert!(text.starts_with("SignatureDoesNotMatch: The request signature"), "{text}");
        assert!(text.ends_with("Check the secret access key."), "{text}");
        assert_eq!(describe_error("<html>\n  Bad gateway </html>"), "<html> Bad gateway </html>");
    }

    #[test]
    fn requests_go_to_the_bucket_host_or_path() {
        let settings = S3Settings {
            endpoint: "http://minio.lan:9000".into(),
            bucket: "backups".into(),
            prefix: "home".into(),
            access_key_id: "AK".into(),
            path_style: true,
            ..S3Settings::default()
        };
        let client = Client::new(settings.clone(), "secret".into()).unwrap();
        let key = client.object_key("dumbmonit-20260922-190000.db");
        let location = client.location(Some(&key), &[]).unwrap();
        assert_eq!(
            location.url.as_str(),
            "http://minio.lan:9000/backups/home/dumbmonit-20260922-190000.db"
        );
        assert_eq!(location.authority, "minio.lan:9000");
        assert_eq!(location.path, "/backups/home/dumbmonit-20260922-190000.db");

        let settings = S3Settings {
            endpoint: String::new(),
            region: "eu-west-3".into(),
            path_style: false,
            ..settings
        };
        let client = Client::new(settings, "secret".into()).unwrap();
        let query = vec![("list-type".to_string(), "2".to_string())];
        let location = client.location(None, &query).unwrap();
        assert_eq!(
            location.url.as_str(),
            "https://backups.s3.eu-west-3.amazonaws.com/?list-type=2"
        );
        assert_eq!(location.authority, "backups.s3.eu-west-3.amazonaws.com");
        assert_eq!(location.path, "/");
    }
}
