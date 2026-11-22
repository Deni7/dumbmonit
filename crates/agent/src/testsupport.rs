//! Faux serveurs pour les tests : un démon Docker sur socket Unix et un serveur
//! HTTP sur TCP. Ils répondent selon un scénario donné par le test et gardent la
//! trace de chaque requête reçue, dans l'ordre.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_tungstenite::tungstenite::handshake::server::{
    ErrorResponse, Request as WsRequest, Response as WsResponse,
};

/// Une requête reçue par un faux serveur.
#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    /// Chemin et paramètres, tels que le client les a écrits.
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

/// Réponse du scénario : statut et corps.
pub type Resp = (u16, String);

pub type Handler = Arc<dyn Fn(&Req) -> Resp + Send + Sync>;

/// Historique partagé des requêtes reçues.
#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<Vec<Req>>>);

impl Calls {
    pub fn all(&self) -> Vec<Req> {
        self.0.lock().unwrap().clone()
    }

    /// `METHODE chemin` de chaque requête, dans l'ordre d'arrivée.
    pub fn lines(&self) -> Vec<String> {
        self.all().iter().map(|r| format!("{} {}", r.method, r.path)).collect()
    }

    pub fn count(&self, method: &str, path_prefix: &str) -> usize {
        self.all().iter().filter(|r| r.method == method && r.path.starts_with(path_prefix)).count()
    }

    pub fn bodies(&self, method: &str, path_prefix: &str) -> Vec<String> {
        self.all()
            .into_iter()
            .filter(|r| r.method == method && r.path.starts_with(path_prefix))
            .map(|r| r.body)
            .collect()
    }
}

async fn read_request<S: AsyncRead + Unpin>(stream: &mut S) -> Option<Req> {
    let mut data = Vec::new();
    let mut chunk = [0u8; 4096];
    let split = loop {
        if let Some(at) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break at;
        }
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&data[..split]).to_string();
    let mut lines = head.lines();
    let mut first = lines.next()?.split_whitespace();
    let method = first.next()?.to_string();
    let path = first.next()?.to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = data[split + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Some(Req { method, path, headers, body: String::from_utf8_lossy(&body).to_string() })
}

pub async fn answer<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    handler: Handler,
    calls: Calls,
) {
    let Some(mut request) = read_request(&mut stream).await else { return };
    // Le client Docker préfixe ses chemins de la version de l'API.
    if let Some(rest) = request.path.strip_prefix("/v1.")
        && let Some(slash) = rest.find('/')
    {
        request.path = rest[slash..].to_string();
    }
    let (status, body) = handler(&request);
    calls.0.lock().unwrap().push(request);
    let reply = format!(
        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}

/// Faux démon Docker. Le répertoire doit vivre aussi longtemps que le test.
#[cfg(unix)]
pub fn fake_docker(
    dir: &tempfile::TempDir,
    handler: impl Fn(&Req) -> Resp + Send + Sync + 'static,
) -> (PathBuf, Calls) {
    let socket = dir.path().join("docker.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("socket");
    let calls = Calls::default();
    let handler: Handler = Arc::new(handler);
    let seen = calls.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            tokio::spawn(answer(stream, handler.clone(), seen.clone()));
        }
    });
    (socket, calls)
}

/// Faux serveur HTTP ; rend son URL de base.
pub async fn fake_http(handler: impl Fn(&Req) -> Resp + Send + Sync + 'static) -> (String, Calls) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("port");
    let url = format!("http://{}", listener.local_addr().unwrap());
    let calls = Calls::default();
    let handler: Handler = Arc::new(handler);
    let seen = calls.clone();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else { return };
            tokio::spawn(answer(stream, handler.clone(), seen.clone()));
        }
    });
    (url, calls)
}

/// Réponse de mise à niveau WebSocket qui annonce ce sous-protocole.
pub fn answering(
    protocol: &'static str,
) -> impl FnOnce(&WsRequest, WsResponse) -> Result<WsResponse, ErrorResponse> {
    move |_, mut response| {
        let value = reqwest::header::HeaderValue::from_static(protocol);
        response.headers_mut().insert(reqwest::header::SEC_WEBSOCKET_PROTOCOL, value);
        Ok(response)
    }
}

/// Configuration TLS d'un faux canal des agents : une autorité et le
/// certificat du canal qu'elle signe, comme ceux du serveur ; rend aussi
/// l'empreinte de l'autorité, à épingler.
pub fn channel_tls() -> (rustls::ServerConfig, String) {
    use rcgen::{
        BasicConstraints, CertificateParams, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
        KeyUsagePurpose, PKCS_ECDSA_P256_SHA256, SanType,
    };
    use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};

    let ca_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("clé");
    let mut ca = CertificateParams::default();
    ca.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::DigitalSignature];
    let ca_cert = ca.self_signed(&ca_key).expect("autorité");
    let issuer = Issuer::new(ca, ca_key);
    let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("clé");
    let mut leaf = CertificateParams::default();
    let name = dumbmonit_proto::AGENT_TLS_SERVER_NAME;
    leaf.subject_alt_names = vec![SanType::DnsName(name.try_into().expect("nom"))];
    leaf.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_cert = leaf.signed_by(&key, &issuer).expect("certificat");
    let chain = vec![leaf_cert.der().clone(), ca_cert.der().clone()];
    let private = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
    let mut config = rustls::ServerConfig::builder_with_provider(crate::tls::provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("TLS 1.3")
        .with_no_client_auth()
        .with_single_cert(chain, private)
        .expect("configuration");
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    (config, crate::tls::fingerprint(ca_cert.der().as_ref()))
}

/// Faux serveur DumbMonit vu par sa WebSocket : la mise à niveau à
/// `/api/agent/ws`, puis, dedans, TLS 1.3 sous une autorité à épingler et une
/// requête HTTP/1.1 par connexion, servie par le scénario. Rend l'adresse,
/// l'empreinte de l'autorité et l'historique.
pub async fn fake_channel(
    handler: impl Fn(&Req) -> Resp + Send + Sync + 'static,
) -> (std::net::SocketAddr, String, Calls) {
    let (config, pin) = channel_tls();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("port");
    let addr = listener.local_addr().expect("adresse");
    let calls = Calls::default();
    let handler: Handler = Arc::new(handler);
    let seen = calls.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else { return };
            let (acceptor, handler, seen) = (acceptor.clone(), handler.clone(), seen.clone());
            tokio::spawn(async move {
                let protocol = answering(dumbmonit_proto::AGENT_WS_PROTOCOL);
                let Ok(ws) = tokio_tungstenite::accept_hdr_async(socket, protocol).await else {
                    return;
                };
                // La WebSocket d'un côté, un flux d'octets de l'autre, sous TLS.
                let (inner, outer) = tokio::io::duplex(1 << 20);
                tokio::spawn(async move {
                    let _ = crate::ws::splice(outer, ws).await;
                });
                let Ok(tls) = acceptor.accept(inner).await else { return };
                answer(tls, handler, seen).await;
            });
        }
    });
    (addr, pin, calls)
}
