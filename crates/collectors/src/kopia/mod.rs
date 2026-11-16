//! Kopia, par l'API REST de son serveur (`kopia server start`, port 51515).
//!
//! Deux lectures, toutes deux avec les identifiants du serveur (authentification
//! HTTP Basic, `--server-username` / `--server-password`) :
//!
//! * `GET /api/v1/repo/status` : le dépôt est-il connecté, son stockage, son
//!   chiffrement. Lecture obligatoire : c'est elle qui valide les identifiants ;
//! * `GET /api/v1/sources` : les sources de sauvegarde connues du serveur, chacune
//!   avec son état (`IDLE`, `PENDING`, `UPLOADING`, `PAUSED`, `REMOTE`), sa
//!   planification et son dernier instantané (`lastSnapshot`, avec les
//!   statistiques `stats`). Lue seulement si le dépôt est connecté : Kopia
//!   répond par une erreur sinon.
//!
//! # Le jeton CSRF
//!
//! Un serveur lancé normalement refuse ces deux lectures en 401 « Invalid or
//! missing CSRF token. », même avec les bons identifiants : l'API de
//! l'interface web attend l'en-tête `X-Kopia-Csrf-Token` et le cookie de
//! session `Kopia-Session-Cookie` dont ce jeton est la signature HMAC
//! (`internal/server/server_authz_checks.go`). Le collecteur fait comme un
//! navigateur : `GET /` pose le cookie et contient le jeton
//! (`<meta name="kopia-csrf-token" content="…" />`). La session est gardée
//! par cible et rouverte quand Kopia la refuse (redémarrage du serveur, qui
//! change la clé de signature). Un serveur lancé avec
//! `--disable-csrf-token-checks` répond dès le premier appel, sans session.
//!
//! Kopia vérifie le mot de passe avant le jeton : un mauvais mot de passe
//! répond « Access denied. », jamais l'erreur CSRF, et les deux messages
//! restent distincts.
//!
//! `GET /api/v1/snapshots?…` (tous les instantanés d'une source) n'est pas lu :
//! le dernier instantané de chaque source figure déjà dans `/api/v1/sources`, et
//! interroger l'historique de chaque source multiplierait les appels sans rien
//! apporter aux alertes (échec, instantané trop ancien).
//!
//! # Ce que la sauvegarde « en échec » veut dire pour Kopia
//!
//! Kopia enregistre un instantané même quand certains fichiers n'ont pas pu être
//! lus : `errorCount` (ou `numFailed` du résumé du dossier racine) dit combien.
//! Une source dont la planification n'a pas pu s'exécuter n'a, elle, tout
//! simplement pas de nouvel instantané : c'est l'âge qui la trahit. Les deux
//! signaux sont donc publiés, et c'est la règle d'âge qui ne s'applique qu'aux
//! sources planifiées (une source manuelle n'a pas à être récente).
//!
//! Les sources `REMOTE` sont celles d'autres machines connues du dépôt : ce
//! serveur ne les sauvegarde pas, elles sont comptées mais jamais décrites.
//!
//! # Réglages, portés par les étiquettes de la cible
//!
//! | Étiquette | Défaut | Rôle |
//! |---|---|---|
//! | `scheme` | `https` | Protocole de l'API (`http` si le serveur tourne avec `--insecure`). |
//! | `port` | `51515` | Port de l'API. |
//! | `insecure_tls` | `false` | Accepte le certificat auto-signé de `--tls-generate-cert`. |
//! | `request_timeout_seconds` | `15` | Délai par requête HTTP. |

use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use dumbmonit_proto::{Collector, Credential, ProbeError, Sample, Target};
use reqwest::StatusCode;
use serde_json::Value;

use crate::rest::{
    MAX_NAMED, RestClient, TokenCache, age_seconds, flag, gauge, now_ms, number, text,
};

pub const DEFAULT_PORT: u16 = 51515;
const REPO_STATUS: &str = "/api/v1/repo/status";
const SOURCES: &str = "/api/v1/sources";

/// L'en-tête du jeton CSRF (`apiclient.CSRFTokenHeader` de Kopia).
pub const CSRF_HEADER: &str = "X-Kopia-Csrf-Token";
/// Le cookie de la session de l'interface web, que le jeton signe.
pub const SESSION_COOKIE: &str = "Kopia-Session-Cookie";
/// Une session vaut jusqu'au redémarrage du serveur : gardée une heure, et
/// rouverte plus tôt si Kopia la refuse.
const SESSION_LIFETIME: Duration = Duration::from_secs(3600);

const NO_CSRF_TOKEN: &str = "Kopia requires a CSRF token on its API and hands it out only with \
                             its web interface, but / gave none. Start the server with \
                             --disable-csrf-token-checks, or keep its web interface on.";
const CSRF_STILL_REFUSED: &str = "Kopia refused the CSRF token of a session it had just \
                                  opened. Start the server with --disable-csrf-token-checks.";

/// Une session de l'interface web : le cookie qui l'identifie et le jeton CSRF
/// que le serveur en a tiré.
pub struct Session {
    pub cookie: String,
    pub token: String,
}

impl Session {
    fn headers(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Cookie", format!("{SESSION_COOKIE}={}", self.cookie)),
            (CSRF_HEADER, self.token.clone()),
        ]
    }

    fn encode(&self) -> String {
        format!("{}\n{}", self.cookie, self.token)
    }

    fn decode(raw: &str) -> Option<Self> {
        let (cookie, token) = raw.split_once('\n')?;
        Some(Self { cookie: cookie.to_string(), token: token.to_string() })
    }
}

/// Le jeton CSRF que Kopia glisse dans sa page d'accueil :
/// `<meta name="kopia-csrf-token" content="…" />`.
pub fn csrf_token(html: &str) -> Option<&str> {
    let meta = &html[html.find(r#"name="kopia-csrf-token""#)?..];
    let content = &meta[meta.find(r#"content=""#)? + r#"content=""#.len()..];
    Some(&content[..content.find('"')?]).filter(|token| !token.is_empty())
}

/// La valeur du cookie de session posé par la page d'accueil.
pub fn session_cookie(headers: &reqwest::header::HeaderMap) -> Option<String> {
    headers
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .filter_map(|pair| pair.trim().strip_prefix(SESSION_COOKIE)?.strip_prefix('='))
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

/// Un refus faute de jeton CSRF valable : 401 « Invalid or missing CSRF token. ».
pub fn csrf_refused(status: StatusCode, body: &str) -> bool {
    status == StatusCode::UNAUTHORIZED && body.contains("CSRF token")
}

/// Un vrai refus d'identifiants (« Access denied. », « Missing credentials. »).
fn credentials_refused(path: &str) -> ProbeError {
    ProbeError::Auth(format!(
        "Kopia refused the user name or password on {path} (401): enter the server user \
         (--server-username) and its password (--server-password)."
    ))
}

/// Un `GET` de l'API, avec la session s'il y en a une. `Ok(None)` : Kopia
/// exige un jeton CSRF que la session n'a pas, ou plus.
async fn api_get(
    client: &RestClient,
    session: Option<&Session>,
    path: &str,
) -> Result<Option<Value>, ProbeError> {
    let extra = session.map(Session::headers).unwrap_or_default();
    let (status, body) = client.get_with(path, &extra).await?;
    if csrf_refused(status, &body) {
        return Ok(None);
    }
    if status == StatusCode::UNAUTHORIZED {
        return Err(credentials_refused(path));
    }
    if !status.is_success() {
        return Err(client.status_error(status, &body, path));
    }
    client.decode(&body, path).map(Some)
}

/// Les deux lectures. `Ok(None)` : refusées faute de jeton CSRF.
async fn read_with(
    client: &RestClient,
    session: Option<&Session>,
) -> Result<Option<Replies>, ProbeError> {
    let Some(status) = api_get(client, session, REPO_STATUS).await? else { return Ok(None) };
    let connected = status.get("connected").and_then(Value::as_bool).unwrap_or(false);
    let sources = if connected {
        let Some(sources) = api_get(client, session, SOURCES).await? else { return Ok(None) };
        Some(sources)
    } else {
        None
    };
    Ok(Some(Replies { status, sources }))
}

/// Ouvre une session comme un navigateur : la page d'accueil pose le cookie de
/// session et contient le jeton CSRF qui en dérive.
async fn open_session(client: &RestClient) -> Result<Session, ProbeError> {
    let (status, headers, body) = client.get_with_headers("/").await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(credentials_refused("/"));
    }
    if status == StatusCode::FORBIDDEN {
        return Err(ProbeError::Auth(
            "Kopia keeps its web interface, and so the CSRF token its API requires, from this \
             user (403): enter the server user (--server-username)."
                .to_string(),
        ));
    }
    let token = csrf_token(&body).filter(|_| status.is_success());
    match (token, session_cookie(&headers)) {
        (Some(token), Some(cookie)) => Ok(Session { cookie, token: token.to_string() }),
        _ => Err(ProbeError::Config(NO_CSRF_TOKEN.to_string())),
    }
}

#[derive(Default)]
pub struct KopiaCollector {
    /// Sessions CSRF ouvertes, par cible (`cookie` et jeton, voir [`Session`]).
    sessions: TokenCache,
}

impl KopiaCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Les deux lectures, en ouvrant une session CSRF si Kopia l'exige.
    async fn read(&self, target: &Target) -> Result<Replies, ProbeError> {
        let client = self.client(target)?;
        let key = format!("{}|{}", target.id, client.base_url());
        let cached = self.sessions.get(&key).and_then(|raw| Session::decode(&raw));
        if let Some(replies) = read_with(&client, cached.as_ref()).await? {
            return Ok(replies);
        }
        self.sessions.forget(&key);
        let session = open_session(&client).await?;
        let replies = read_with(&client, Some(&session))
            .await?
            .ok_or_else(|| ProbeError::Config(CSRF_STILL_REFUSED.to_string()))?;
        self.sessions.put(&key, &session.encode(), SESSION_LIFETIME);
        Ok(replies)
    }

    fn client(&self, target: &Target) -> Result<RestClient, ProbeError> {
        let client = RestClient::for_target(target, "https", DEFAULT_PORT, "Kopia")?;
        match &target.credential {
            // Un serveur lancé avec `--without-password` n'attend rien.
            Credential::None => Ok(client),
            Credential::UsernamePassword { username, password } if !username.trim().is_empty() => {
                let pair = format!("{}:{password}", username.trim());
                let encoded = base64::engine::general_purpose::STANDARD.encode(pair.as_bytes());
                Ok(client.with_header("Authorization", format!("Basic {encoded}")))
            }
            other => Err(ProbeError::Config(format!(
                "Kopia expects the server user name and password, or nothing, configured: {other}"
            ))),
        }
    }
}

#[async_trait]
impl Collector for KopiaCollector {
    fn kind(&self) -> &str {
        "kopia"
    }

    async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        Ok(samples(&self.read(target).await?, now_ms()))
    }

    async fn discover(&self, target: &Target) -> Result<Option<String>, ProbeError> {
        self.read(target).await?;
        Ok(Some("kopia".to_string()))
    }
}

pub struct Replies {
    pub status: Value,
    /// `None` quand le dépôt n'est pas connecté.
    pub sources: Option<Value>,
}

/// Un instant RFC 3339 (Kopia écrit les nanosecondes) en millisecondes Unix.
fn instant_ms(raw: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(raw.trim()).ok().map(|dt| dt.timestamp_millis())
}

/// Un champ présent et non vide, qu'il soit un texte ou une liste.
fn present(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Array(items)) => !items.is_empty(),
        Some(Value::String(s)) => !s.trim().is_empty(),
        _ => false,
    }
}

/// Vrai si Kopia déclenche lui-même les instantanés de cette source.
fn is_scheduled(entry: &Value) -> bool {
    let schedule = entry.get("schedule");
    let manual = schedule.and_then(|s| s.get("manual")).and_then(Value::as_bool).unwrap_or(false);
    if manual {
        return false;
    }
    present(entry.get("nextSnapshotTime"))
        || schedule.is_some_and(|s| {
            number(s.get("intervalSeconds")).is_some_and(|v| v > 0.0)
                || present(s.get("timeOfDay"))
                || present(s.get("cron"))
        })
}

/// `utilisateur@machine:/chemin`, la forme que Kopia affiche.
fn source_name(entry: &Value) -> Option<String> {
    let source = entry.get("source")?;
    let path = text(source, "path")?;
    let user = text(source, "userName").unwrap_or("?");
    let host = text(source, "host").unwrap_or("?");
    Some(format!("{user}@{host}:{path}"))
}

pub fn samples(r: &Replies, ts_ms: i64) -> Vec<Sample> {
    let g = |name: &str, value: f64| gauge(name, value, ts_ms);
    let mut out = Vec::new();

    let connected = r.status.get("connected").and_then(Value::as_bool).unwrap_or(false);
    out.push(flag("kopia_repo_connected", connected, ts_ms));
    if connected {
        let readonly = r.status.get("readonly").and_then(Value::as_bool).unwrap_or(false);
        out.push(flag("kopia_repo_readonly", readonly, ts_ms));
        out.push(
            g("kopia_repo_info", 1.0)
                .with_label("storage", text(&r.status, "storage").unwrap_or(""))
                .with_label("hash", text(&r.status, "hash").unwrap_or(""))
                .with_label("encryption", text(&r.status, "encryption").unwrap_or("")),
        );
    }

    let Some(sources) = &r.sources else { return out };
    let entries: Vec<&Value> =
        sources.get("sources").and_then(Value::as_array).into_iter().flatten().collect();
    let (mut total, mut remote, mut without, mut with_errors, mut uploading) = (0, 0, 0, 0, 0);
    for entry in entries {
        let status = text(entry, "status").unwrap_or("UNKNOWN");
        if status.eq_ignore_ascii_case("REMOTE") {
            remote += 1;
            continue;
        }
        let Some(name) = source_name(entry) else { continue };
        total += 1;
        let last = entry.get("lastSnapshot").filter(|v| v.is_object());
        let errors = last
            .map(|l| {
                number(l.pointer("/stats/errorCount"))
                    .or_else(|| number(l.pointer("/rootEntry/summ/numFailed")))
                    .unwrap_or(0.0)
            })
            .unwrap_or(0.0);
        let is_uploading = status.eq_ignore_ascii_case("UPLOADING");
        without += usize::from(last.is_none());
        with_errors += usize::from(errors > 0.0);
        uploading += usize::from(is_uploading);
        if total > MAX_NAMED {
            continue;
        }
        let named = |sample: Sample| sample.with_label("source", name.as_str());
        out.push(named(flag("kopia_source_has_snapshot", last.is_some(), ts_ms)));
        out.push(named(flag("kopia_source_scheduled", is_scheduled(entry), ts_ms)));
        out.push(named(flag("kopia_source_uploading", is_uploading, ts_ms)));
        out.push(named(g("kopia_source_status_info", 1.0).with_label("status", status)));
        let Some(last) = last else { continue };
        let start = text(last, "startTime");
        let end = text(last, "endTime");
        if let Some(age) = end.or(start).and_then(|t| age_seconds(t, ts_ms)) {
            out.push(named(g("kopia_source_last_snapshot_age_seconds", age)));
        }
        if let (Some(start), Some(end)) = (start.and_then(instant_ms), end.and_then(instant_ms))
            && end >= start
        {
            out.push(named(g(
                "kopia_source_last_snapshot_duration_seconds",
                (end - start) as f64 / 1000.0,
            )));
        }
        out.push(named(g("kopia_source_last_snapshot_errors", errors)));
        if let Some(size) = number(last.pointer("/stats/totalSize"))
            .or_else(|| number(last.pointer("/rootEntry/summ/size")))
        {
            out.push(named(g("kopia_source_last_snapshot_bytes", size)));
        }
        if let Some(files) = number(last.pointer("/stats/fileCount"))
            .or_else(|| number(last.pointer("/rootEntry/summ/files")))
        {
            out.push(named(g("kopia_source_last_snapshot_files", files)));
        }
    }
    out.push(g("kopia_sources", total as f64));
    out.push(g("kopia_sources_without_snapshot", without as f64));
    out.push(g("kopia_sources_with_errors", with_errors as f64));
    out.push(g("kopia_sources_uploading", uploading as f64));
    out.push(g("kopia_sources_remote", remote as f64));
    out
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::Router;
    use axum::http::{HeaderMap, StatusCode, Uri, header};
    use axum::response::IntoResponse;
    use axum::routing::get;

    use super::*;
    use crate::rest::test_support::{find, serve, value};
    use crate::uptime::tags::test_support::cible;

    // Le jeton CSRF d'un serveur Kopia 0.23.1 lancé sans
    // `--disable-csrf-token-checks`. `index.html` : la page d'accueil telle que
    // le serveur la sert, l'`index.html` de kopia/htmlui où
    // `Server.patchIndexBytes` (`internal/server/server.go`) a ajouté la
    // version au titre et la balise `kopia-csrf-token` avant `</head>`.
    // `csrf_refused.txt` et `access_denied.txt` : les corps des deux 401 relevés
    // sur ce serveur (`requireAuth`, puis `isAuthenticated` pour un mauvais mot
    // de passe).
    const INDEX: &str = include_str!("testdata/index.html");
    const CSRF_REFUSED: &str = include_str!("testdata/csrf_refused.txt");
    const ACCESS_DENIED: &str = include_str!("testdata/access_denied.txt");
    const TOKEN: &str = "0b6f9eece14bfe70e83bc5fd9fa4f5588fa78c52d58ad3eab6397f8e76e69a27";
    const SESSION_ID: &str = "c8a1d5e2-3f4b-4c6d-9e7f-0a1b2c3d4e5f";

    // Réponses construites d'après les structures de l'API du serveur Kopia
    // (`serverapi.StatusResponse`, `serverapi.SourceStatus`, `snapshot.Manifest`) :
    // des photos sauvegardées hier soir, des documents en cours de téléversement
    // dont le dernier instantané a trois fichiers en erreur, une archive sans
    // instantané depuis six jours, une source manuelle sans instantané, et une
    // source d'une autre machine.
    const STATUS: &str = include_str!("testdata/repo_status.json");
    const DISCONNECTED: &str = include_str!("testdata/repo_status_disconnected.json");
    const SOURCES_JSON: &str = include_str!("testdata/sources.json");

    fn json(raw: &str) -> Value {
        serde_json::from_str(raw).unwrap()
    }

    fn now() -> i64 {
        chrono::DateTime::parse_from_rfc3339("2026-10-26T01:00:00Z").unwrap().timestamp_millis()
    }

    #[test]
    fn sources_instantanes_erreurs() {
        let replies = Replies { status: json(STATUS), sources: Some(json(SOURCES_JSON)) };
        let s = samples(&replies, now());
        assert_eq!(value(&s, "kopia_repo_connected", &[]), 1.0);
        assert_eq!(find(&s, "kopia_repo_info", &[]).unwrap().labels["storage"], "filesystem");
        assert_eq!(value(&s, "kopia_sources", &[]), 4.0, "the remote source is not counted");
        assert_eq!(value(&s, "kopia_sources_remote", &[]), 1.0);
        assert_eq!(value(&s, "kopia_sources_without_snapshot", &[]), 1.0);
        assert_eq!(value(&s, "kopia_sources_with_errors", &[]), 1.0);
        assert_eq!(value(&s, "kopia_sources_uploading", &[]), 1.0);

        let photos = [("source", "root@nas:/data/photos")];
        let age = value(&s, "kopia_source_last_snapshot_age_seconds", &photos);
        assert!((age - 82_338.89).abs() < 0.5, "{age}");
        let duration = value(&s, "kopia_source_last_snapshot_duration_seconds", &photos);
        assert!((duration - 457.63).abs() < 0.01, "{duration}");
        assert_eq!(value(&s, "kopia_source_last_snapshot_bytes", &photos), 48_318_382_080.0);
        assert_eq!(value(&s, "kopia_source_last_snapshot_files", &photos), 15_234.0);
        assert_eq!(value(&s, "kopia_source_last_snapshot_errors", &photos), 0.0);
        assert_eq!(value(&s, "kopia_source_scheduled", &photos), 1.0);

        let documents = [("source", "root@nas:/data/documents")];
        assert_eq!(value(&s, "kopia_source_last_snapshot_errors", &documents), 3.0);
        assert_eq!(value(&s, "kopia_source_uploading", &documents), 1.0);
        assert_eq!(value(&s, "kopia_source_scheduled", &documents), 1.0, "timeOfDay");

        let archive = [("source", "root@nas:/data/archive")];
        assert_eq!(value(&s, "kopia_source_last_snapshot_age_seconds", &archive), 517_200.0);

        let scratch = [("source", "root@nas:/data/scratch")];
        assert_eq!(value(&s, "kopia_source_has_snapshot", &scratch), 0.0);
        assert_eq!(value(&s, "kopia_source_scheduled", &scratch), 0.0, "manual");
        assert!(find(&s, "kopia_source_last_snapshot_age_seconds", &scratch).is_none());
        assert!(
            find(&s, "kopia_source_has_snapshot", &[("source", "anna@laptop:/home/anna")])
                .is_none()
        );
    }

    #[test]
    fn un_depot_deconnecte_ne_publie_pas_de_sources() {
        let replies = Replies { status: json(DISCONNECTED), sources: None };
        let s = samples(&replies, now());
        assert_eq!(value(&s, "kopia_repo_connected", &[]), 0.0);
        assert!(find(&s, "kopia_sources", &[]).is_none());
        assert!(find(&s, "kopia_repo_info", &[]).is_none());
    }

    #[tokio::test]
    async fn interrogation_complete() {
        let expected =
            format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(b"kopia:s3cret"));
        let app = Router::new().fallback(get(move |headers: HeaderMap, uri: Uri| {
            let expected = expected.clone();
            async move {
                if headers.get("authorization").and_then(|v| v.to_str().ok())
                    != Some(expected.as_str())
                {
                    return (StatusCode::UNAUTHORIZED, "");
                }
                match uri.path() {
                    "/api/v1/repo/status" => (StatusCode::OK, STATUS),
                    "/api/v1/sources" => (StatusCode::OK, SOURCES_JSON),
                    _ => (StatusCode::NOT_FOUND, ""),
                }
            }
        }));
        let address = serve(app).await;
        let collector = KopiaCollector::new();
        let mut target = cible("kopia", &format!("http://{address}"), &[]);
        target.credential =
            Credential::UsernamePassword { username: "kopia".into(), password: "s3cret".into() };
        let s = collector.probe(&target).await.unwrap();
        assert_eq!(value(&s, "kopia_sources", &[]), 4.0);

        target.credential =
            Credential::UsernamePassword { username: "kopia".into(), password: "wrong".into() };
        let error = collector.probe(&target).await.unwrap_err();
        assert!(matches!(error, ProbeError::Auth(_)), "{error}");
        assert!(!error.means_down());

        target.credential = Credential::ApiToken { token: "x".into() };
        let error = collector.probe(&target).await.unwrap_err();
        assert!(matches!(error, ProbeError::Config(_)), "{error}");
    }

    #[tokio::test]
    async fn un_serveur_injoignable_est_en_panne() {
        let collector = KopiaCollector::new();
        let target = cible("kopia", "http://127.0.0.1:1", &[]);
        let error = collector.probe(&target).await.unwrap_err();
        assert!(error.means_down(), "{error}");
    }

    #[test]
    fn le_jeton_csrf_et_le_cookie_se_lisent_comme_kopia_les_pose() {
        assert_eq!(csrf_token(INDEX), Some(TOKEN));
        assert_eq!(csrf_token("<html><head></head></html>"), None);
        let mut headers = reqwest::header::HeaderMap::new();
        headers.append(
            reqwest::header::SET_COOKIE,
            "Kopia-Auth=eyJhbGciOiJIUzI1NiJ9.e30.c2ln; Path=/; HttpOnly; SameSite=Strict"
                .parse()
                .unwrap(),
        );
        headers.append(
            reqwest::header::SET_COOKIE,
            format!("{SESSION_COOKIE}={SESSION_ID}; Path=/; HttpOnly; SameSite=Strict")
                .parse()
                .unwrap(),
        );
        assert_eq!(session_cookie(&headers).as_deref(), Some(SESSION_ID));
        assert!(csrf_refused(reqwest::StatusCode::UNAUTHORIZED, CSRF_REFUSED));
        assert!(!csrf_refused(reqwest::StatusCode::UNAUTHORIZED, ACCESS_DENIED));
        let session = Session { cookie: SESSION_ID.into(), token: TOKEN.into() };
        let back = Session::decode(&session.encode()).unwrap();
        assert_eq!((back.cookie.as_str(), back.token.as_str()), (SESSION_ID, TOKEN));
    }

    /// Un serveur Kopia lancé normalement : les identifiants d'abord
    /// (« Access denied. »), puis sur l'API le cookie de session et le jeton
    /// CSRF qui en dérive (« Invalid or missing CSRF token. »). `/` pose le
    /// cookie et sert la page qui porte le jeton, sauf sans interface (`ui`).
    fn kopia_with_csrf(ui: bool, pages: Arc<AtomicUsize>) -> Router {
        let expected =
            format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(b"kopia:s3cret"));
        Router::new().fallback(get(move |headers: HeaderMap, uri: Uri| {
            let expected = expected.clone();
            let pages = pages.clone();
            async move {
                let basic = headers.get("authorization").and_then(|v| v.to_str().ok());
                if basic != Some(expected.as_str()) {
                    return (StatusCode::UNAUTHORIZED, ACCESS_DENIED).into_response();
                }
                if uri.path() == "/" {
                    if !ui {
                        return (StatusCode::NOT_FOUND, "404 page not found\n").into_response();
                    }
                    pages.fetch_add(1, Ordering::SeqCst);
                    let mut response = INDEX.into_response();
                    let cookies = [
                        "Kopia-Auth=eyJhbGciOiJIUzI1NiJ9.e30.c2ln; Path=/; HttpOnly".to_string(),
                        format!("{SESSION_COOKIE}={SESSION_ID}; Path=/; HttpOnly; SameSite=Strict"),
                    ];
                    for cookie in cookies {
                        response.headers_mut().append(header::SET_COOKIE, cookie.parse().unwrap());
                    }
                    return response;
                }
                let cookie = headers.get(header::COOKIE).and_then(|v| v.to_str().ok());
                let token = headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok());
                let session = format!("{SESSION_COOKIE}={SESSION_ID}");
                if cookie != Some(session.as_str()) || token != Some(TOKEN) {
                    return (StatusCode::UNAUTHORIZED, CSRF_REFUSED).into_response();
                }
                match uri.path() {
                    REPO_STATUS => (StatusCode::OK, STATUS).into_response(),
                    SOURCES => (StatusCode::OK, SOURCES_JSON).into_response(),
                    _ => StatusCode::NOT_FOUND.into_response(),
                }
            }
        }))
    }

    #[tokio::test]
    async fn le_jeton_csrf_est_demande_une_fois_puis_garde() {
        let pages = Arc::new(AtomicUsize::new(0));
        let address = serve(kopia_with_csrf(true, pages.clone())).await;
        let collector = KopiaCollector::new();
        let mut target = cible("kopia", &format!("http://{address}"), &[]);
        target.credential =
            Credential::UsernamePassword { username: "kopia".into(), password: "s3cret".into() };
        let s = collector.probe(&target).await.unwrap();
        assert_eq!(value(&s, "kopia_repo_connected", &[]), 1.0);
        assert_eq!(value(&s, "kopia_sources", &[]), 4.0);
        let s = collector.probe(&target).await.unwrap();
        assert_eq!(value(&s, "kopia_sources", &[]), 4.0);
        assert_eq!(pages.load(Ordering::SeqCst), 1, "the session is kept between probes");

        // Un mauvais mot de passe reste un mauvais mot de passe, pas une
        // affaire de jeton CSRF.
        target.credential =
            Credential::UsernamePassword { username: "kopia".into(), password: "wrong".into() };
        let error = KopiaCollector::new().probe(&target).await.unwrap_err();
        let ProbeError::Auth(message) = &error else { panic!("{error}") };
        assert!(message.contains("password") && !message.contains("CSRF"), "{message}");
    }

    #[tokio::test]
    async fn sans_interface_web_le_message_nomme_l_option_de_kopia() {
        let address = serve(kopia_with_csrf(false, Arc::default())).await;
        let mut target = cible("kopia", &format!("http://{address}"), &[]);
        target.credential =
            Credential::UsernamePassword { username: "kopia".into(), password: "s3cret".into() };
        let error = KopiaCollector::new().probe(&target).await.unwrap_err();
        assert!(
            matches!(error, ProbeError::Config(ref m) if m.contains("--disable-csrf-token-checks")),
            "{error}"
        );
        assert!(!error.means_down());
    }
}
