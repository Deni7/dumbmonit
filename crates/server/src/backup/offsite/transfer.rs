//! Ce que les trois transports partagent : l'erreur et son caractère
//! réessayable, le client HTTP, le corps lu par morceaux depuis le fichier,
//! l'empreinte calculée sans charger le fichier en mémoire.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use futures::TryStreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

/// Délai d'établissement d'une connexion.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// Silence toléré au milieu d'un échange (aucun octet reçu).
pub const READ_TIMEOUT: Duration = Duration::from_secs(120);
/// Durée maximale d'un envoi de fichier : une base de plusieurs gigaoctets
/// sur une liaison lente passe, un envoi bloqué ne bloque pas la nuit suivante.
pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(2 * 3600);
/// Durée maximale d'une liste ou d'une suppression.
pub const SHORT_TIMEOUT: Duration = Duration::from_secs(60);
/// Taille des morceaux lus sur le disque.
const CHUNK: usize = 64 * 1024;
/// Octets d'une réponse d'erreur gardés pour le message.
const ERROR_BODY_BYTES: usize = 4096;
/// Taille maximale d'une liste de fichiers (S3, WebDAV) : des dizaines de
/// milliers de noms. Au-delà, ce n'est plus une liste de sauvegardes, et le
/// serveur ne doit pas la garder entière en mémoire.
pub const MAX_LISTING_BYTES: usize = 16 * 1024 * 1024;

/// Un échec de transfert, dit pour un administrateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub message: String,
    /// Vrai quand une nouvelle tentative a une chance d'aboutir (réseau,
    /// serveur surchargé) ; faux pour un refus (identifiants, droits, chemin).
    pub retry: bool,
    /// Empreinte de la clé d'hôte SFTP présentée, quand c'est elle qui a fait
    /// refuser la connexion (inconnue ou changée).
    pub host_key: Option<HostKey>,
}

/// Clé d'hôte vue lors d'un refus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostKey {
    pub fingerprint: String,
    /// Faux : aucune empreinte n'était épinglée (premier usage). Vrai : une
    /// autre empreinte l'était.
    pub changed: bool,
}

impl Failure {
    pub fn fatal(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry: false, host_key: None }
    }

    pub fn transient(message: impl Into<String>) -> Self {
        Self { message: message.into(), retry: true, host_key: None }
    }

    /// Erreur d'envoi de reqwest : réseau, délai, TLS. Toujours réessayable,
    /// sauf un certificat refusé, qui le sera encore à la tentative suivante.
    pub fn request(what: &str, error: &reqwest::Error) -> Self {
        let detail = chain(error);
        let tls = detail.to_ascii_lowercase().contains("certificate");
        let message = if error.is_timeout() {
            format!("{what}: no answer in time ({detail})")
        } else if error.is_connect() {
            format!("{what}: could not connect ({detail})")
        } else {
            format!("{what}: {detail}")
        };
        Self { message, retry: !tls, host_key: None }
    }

    /// Réponse HTTP en erreur. 5xx, 408 et 429 sont réessayés.
    pub fn status(what: &str, status: reqwest::StatusCode, detail: &str) -> Self {
        let retry = status.is_server_error()
            || status == reqwest::StatusCode::REQUEST_TIMEOUT
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS;
        let detail = detail.trim();
        let message = if detail.is_empty() {
            format!("{what}: the server answered {status}")
        } else {
            format!("{what}: the server answered {status}: {detail}")
        };
        Self { message, retry, host_key: None }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Le message d'une erreur et de ses causes, sur une ligne.
pub fn chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        source = cause.source();
    }
    out
}

/// Client HTTP des destinations : jamais de redirection suivie (une
/// redirection d'S3 signale une mauvaise région, la suivre perdrait la
/// signature et le corps), délais bornés.
pub fn http_client() -> Result<reqwest::Client, Failure> {
    reqwest::Client::builder()
        .user_agent(concat!("DumbMonit/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|error| Failure::fatal(format!("HTTP client: {}", chain(&error))))
}

/// Le début d'une réponse, pour un message d'erreur : borné, sur une ligne.
pub async fn read_snippet(mut response: reqwest::Response) -> String {
    let mut body: Vec<u8> = Vec::new();
    while body.len() < ERROR_BODY_BYTES {
        match response.chunk().await {
            Ok(Some(chunk)) => body.extend_from_slice(&chunk),
            _ => break,
        }
    }
    body.truncate(ERROR_BODY_BYTES);
    String::from_utf8_lossy(&body).into_owned()
}

/// Le corps d'une liste de fichiers, lu par morceaux et refusé au-delà de
/// `max` octets.
pub async fn read_listing(
    mut response: reqwest::Response,
    what: &str,
    max: usize,
) -> Result<String, Failure> {
    if response.content_length().is_some_and(|length| length > max as u64) {
        return Err(too_long(what, max));
    }
    let mut body: Vec<u8> = Vec::new();
    loop {
        let chunk = response.chunk().await.map_err(|error| Failure::request(what, &error))?;
        let Some(chunk) = chunk else { break };
        if body.len() + chunk.len() > max {
            return Err(too_long(what, max));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(String::from_utf8_lossy(&body).into_owned())
}

fn too_long(what: &str, max: usize) -> Failure {
    Failure::fatal(format!("{what}: the answer is larger than {} MiB", max / (1024 * 1024)))
}

/// Ramène un texte à une ligne d'au plus `max` caractères.
pub fn one_line(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        flat
    } else {
        let mut cut: String = flat.chars().take(max).collect();
        cut.push('…');
        cut
    }
}

/// Taille d'un fichier local.
pub async fn file_size(path: &Path) -> Result<u64, Failure> {
    tokio::fs::metadata(path)
        .await
        .map(|meta| meta.len())
        .map_err(|error| Failure::fatal(format!("reading {}: {error}", path.display())))
}

/// SHA-256 hexadécimal d'un fichier, lu par morceaux.
pub async fn sha256_file(path: &Path) -> Result<String, Failure> {
    let mut file = open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| Failure::fatal(format!("reading {}: {error}", path.display())))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub async fn open(path: &Path) -> Result<tokio::fs::File, Failure> {
    tokio::fs::File::open(path)
        .await
        .map_err(|error| Failure::fatal(format!("opening {}: {error}", path.display())))
}

/// Corps de requête lu depuis le fichier au fil de l'envoi : le fichier
/// n'est jamais entier en mémoire. La longueur est annoncée par l'appelant
/// (`Content-Length`), que S3 exige.
pub fn file_body(file: tokio::fs::File) -> reqwest::Body {
    let stream =
        tokio_util::io::ReaderStream::with_capacity(file, CHUNK).map_ok(http_body::Frame::data);
    reqwest::Body::wrap(http_body_util::StreamBody::new(stream))
}

/// Décode les `%XX` d'un chemin ; une séquence invalide reste telle quelle.
pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex_digit(bytes[i + 1]), hex_digit(bytes[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_sequences_are_decoded_and_broken_ones_kept() {
        assert_eq!(percent_decode("a%20b%2Fc"), "a b/c");
        assert_eq!(percent_decode("dumbmonit-20260922-190000.db"), "dumbmonit-20260922-190000.db");
        assert_eq!(percent_decode("bad%2"), "bad%2");
        assert_eq!(percent_decode("bad%zz"), "bad%zz");
        assert_eq!(percent_decode("%C3%A9"), "é");
    }

    #[test]
    fn only_overload_and_server_errors_are_retried() {
        use reqwest::StatusCode;
        assert!(Failure::status("x", StatusCode::BAD_GATEWAY, "").retry);
        assert!(Failure::status("x", StatusCode::TOO_MANY_REQUESTS, "").retry);
        assert!(!Failure::status("x", StatusCode::FORBIDDEN, "").retry);
        assert!(!Failure::status("x", StatusCode::NOT_FOUND, "").retry);
    }

    #[test]
    fn error_text_is_kept_short_and_on_one_line() {
        assert_eq!(one_line("  a\n\tb  c ", 10), "a b c");
        assert_eq!(one_line("abcdef", 3), "abc…");
    }

    #[tokio::test]
    async fn a_listing_is_read_up_to_its_ceiling() {
        let reply = |body: Vec<u8>| reqwest::Response::from(axum::http::Response::new(body));
        let read = read_listing(reply(b"<a/>".to_vec()), "listing", 16).await.unwrap();
        assert_eq!(read, "<a/>");
        let refused = read_listing(reply(vec![b'x'; 17]), "listing", 16).await.unwrap_err();
        assert!(refused.message.starts_with("listing: the answer is larger"), "{refused}");
        assert!(!refused.retry);
    }

    #[tokio::test]
    async fn the_file_hash_is_computed_in_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("f");
        let data = vec![7u8; CHUNK * 2 + 5];
        tokio::fs::write(&path, &data).await.unwrap();
        assert_eq!(sha256_file(&path).await.unwrap(), super::super::sigv4::sha256_hex(&data));
    }
}
