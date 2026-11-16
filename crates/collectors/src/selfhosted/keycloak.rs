//! Keycloak.
//!
//! Deux niveaux, le second facultatif.
//!
//! 1. **La santé** : `/health/ready` et `/health/live`, que Keycloak sert quand
//!    il est lancé avec `health-enabled=true`. Depuis Keycloak 25 elles
//!    écoutent sur l'**interface de gestion**, port 9000 (option
//!    `health_port`) ; avant, sur le port principal (`health_port` = `same`).
//!    Un `503` avec un corps JSON n'est pas une erreur de transport : c'est
//!    Keycloak qui dit quelle vérification (base de données…) est tombée.
//! 2. **L'administration** : avec un client de service (`client_id` et secret,
//!    flux « client credentials »), la version, les domaines (realms), et pour
//!    chacun le nombre de sessions et d'utilisateurs. Le compte de service ne
//!    reçoit que des rôles de lecture ; un refus (403) saute la page sans
//!    échec.
//!
//! Trois pièges de Keycloak 26, que le module signale au lieu de les taire :
//!
//! * depuis 26.8.2, l'API d'administration refuse (401) un jeton sans
//!   l'audience que pose le client scope `kc-admin-api` : le jeton est délivré,
//!   puis refusé. `keycloak_admin_readable` vaut alors 0, et le message nomme
//!   le client scope ;
//! * `client-session-stats` ne compte que les clients que le compte peut voir
//!   (`view-clients`) : sans ce rôle, la réponse est `[]`, comme pour un domaine
//!   sans session. Une liste vide est donc vérifiée par la liste des clients ;
//!   s'ils sont cachés, aucun nombre de sessions n'est publié pour ce domaine,
//!   et l'appel compte dans `keycloak_scrape_errors` ;
//! * `/admin/serverinfo` ne sert la version, la durée de vie et la mémoire
//!   (`systemInfo`, `memoryInfo`) qu'à un compte du domaine master qui a
//!   `manage-realm` ; sans ce rôle, la page répond sans eux.
//!
//! La panne que ce module existe pour voir : **un Keycloak qui répond mais dont
//! la base est tombée** — la page de connexion s'affiche, plus personne ne peut
//! se connecter — et un domaine désactivé par erreur.

use async_trait::async_trait;
use dumbmonit_proto::{Collector, Credential, ProbeError, Sample, Target};
use reqwest::StatusCode;
use serde_json::Value;

use super::client::{Auth, HttpClient};
use super::options::tag;
use super::{MAX_NAMED, flag, gauge, http_client, metric, now_ms, number, settle};

pub const DEFAULT_PORT: u16 = 8080;
pub const DEFAULT_HEALTH_PORT: u16 = 9000;
pub const DEFAULT_AUTH_REALM: &str = "master";
const PRODUCT: &str = "Keycloak";
const P: &str = "keycloak";

/// Au plus autant de domaines interrogés par sonde (deux appels chacun).
pub const MAX_REALMS: usize = 20;

/// Encode un segment de chemin : tout sauf les caractères non réservés.
fn encode(segment: &str) -> String {
    let mut out = String::new();
    for byte in segment.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// L'hôte d'une adresse saisie, sans protocole, port ni chemin : l'interface
/// de gestion écoute sur la même machine mais sur un autre port.
fn host_only(address: &str) -> String {
    let address = address.trim();
    let address = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
        .unwrap_or(address);
    let address = address.split('/').next().unwrap_or(address);
    if address.starts_with('[') {
        return match address.find(']') {
            Some(end) => address[..=end].to_string(),
            None => address.to_string(),
        };
    }
    // Plusieurs `:` : une IPv6 sans crochets, sans port.
    if address.matches(':').count() > 1 {
        return address.to_string();
    }
    address.split(':').next().unwrap_or(address).to_string()
}

// ------------------------------------------------------------ traduction

/// Une réponse de `/health/*` : `{"status": "UP|DOWN", "checks": [...]}`.
pub fn health_samples(name: &str, body: &Value, ts_ms: i64) -> Vec<Sample> {
    let up = |value: &Value| value.get("status").and_then(Value::as_str) == Some("UP");
    let mut out = vec![flag(&metric(P, name), up(body), ts_ms)];
    // Le détail des vérifications n'est publié que pour `ready`, où se trouvent
    // la base de données et le cache.
    if name == "ready" {
        let checks = body.get("checks").and_then(Value::as_array).map(Vec::as_slice);
        for check in checks.unwrap_or(&[]).iter().take(MAX_NAMED) {
            if let Some(label) = check.get("name").and_then(Value::as_str) {
                out.push(
                    flag(&metric(P, "health_check_up"), up(check), ts_ms)
                        .with_label("check", label),
                );
            }
        }
    }
    out
}

/// `GET /admin/serverinfo`.
pub fn serverinfo_samples(body: &Value, ts_ms: i64) -> Vec<Sample> {
    let mut out = Vec::new();
    if let Some(version) = body.pointer("/systemInfo/version").and_then(Value::as_str) {
        out.push(gauge(&metric(P, "version_info"), 1.0, ts_ms).with_label("version", version));
    }
    if let Some(millis) = number(body.pointer("/systemInfo/uptimeMillis")) {
        out.push(gauge(&metric(P, "uptime_seconds"), millis / 1000.0, ts_ms));
    }
    let total = number(body.pointer("/memoryInfo/total"));
    let used = number(body.pointer("/memoryInfo/used"));
    if let (Some(total), Some(used)) = (total, used)
        && total > 0.0
    {
        out.push(gauge(&metric(P, "memory_used_percent"), 100.0 * used / total, ts_ms));
    }
    out
}

/// Les noms des domaines à interroger, et les compteurs de domaines.
/// `wanted` restreint la liste (option `realms`) ; vide, tous (jusqu'à
/// [`MAX_REALMS`]).
pub fn realm_samples(body: &Value, wanted: &[String], ts_ms: i64) -> (Vec<Sample>, Vec<String>) {
    let list = body.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let disabled =
        list.iter().filter(|r| r.get("enabled").and_then(Value::as_bool) == Some(false)).count();
    let samples = vec![
        gauge(&metric(P, "realms_total"), list.len() as f64, ts_ms),
        gauge(&metric(P, "realms_disabled"), disabled as f64, ts_ms),
    ];
    let names = list
        .iter()
        .filter_map(|r| r.get("realm").and_then(Value::as_str))
        .filter(|name| wanted.is_empty() || wanted.iter().any(|w| w == name))
        .take(MAX_REALMS)
        .map(str::to_string)
        .collect();
    (samples, names)
}

/// La liste des clients d'un domaine (`GET /admin/realms/{realm}/clients?max=1`)
/// dit si ses sessions sont visibles : `client-session-stats` saute les clients
/// que le compte ne peut pas voir. Une liste vide (le compte peut lister mais
/// pas voir), absente (403) ou marquée `"access": {"view": false}` : cachés.
pub fn clients_viewable(body: Option<&Value>) -> bool {
    let first = body.and_then(Value::as_array).and_then(|list| list.first());
    first.is_some_and(|c| c.pointer("/access/view").and_then(Value::as_bool) != Some(false))
}

/// `GET /admin/realms/{realm}/client-session-stats` : `[{"clientId", "active",
/// "offline"}]`, les nombres arrivant en texte. Rend (actives, hors ligne).
pub fn session_counts(body: &Value) -> (f64, f64) {
    let list = body.as_array().map(Vec::as_slice).unwrap_or(&[]);
    let sum = |key: &str| list.iter().filter_map(|c| number(c.get(key))).sum::<f64>();
    (sum("active"), sum("offline"))
}

pub fn realm_detail_samples(
    realm: &str,
    sessions: Option<(f64, f64)>,
    users: Option<f64>,
    ts_ms: i64,
) -> Vec<Sample> {
    let mut out = Vec::new();
    if let Some((active, offline)) = sessions {
        out.push(gauge(&metric(P, "realm_sessions"), active, ts_ms).with_label("realm", realm));
        out.push(
            gauge(&metric(P, "realm_offline_sessions"), offline, ts_ms).with_label("realm", realm),
        );
    }
    if let Some(users) = users {
        out.push(gauge(&metric(P, "realm_users"), users, ts_ms).with_label("realm", realm));
    }
    out
}

// ------------------------------------------------------------------ sonde

/// Le client qui interroge l'interface de santé.
fn health_client(target: &Target) -> Result<HttpClient, ProbeError> {
    let port = match tag(target, "health_port") {
        Some("same") => return http_client(target, "http", DEFAULT_PORT, Auth::default(), PRODUCT),
        Some(raw) => raw.parse::<u16>().ok().filter(|p| *p > 0).ok_or_else(|| {
            ProbeError::Config(format!("Invalid health port: \"{raw}\" (a number, or \"same\")"))
        })?,
        None => DEFAULT_HEALTH_PORT,
    };
    let mut management = target.clone();
    management.address = host_only(&target.address);
    management.tags.insert("port".to_string(), port.to_string());
    management
        .tags
        .insert("scheme".to_string(), tag(target, "health_scheme").unwrap_or("http").to_string());
    http_client(&management, "http", DEFAULT_HEALTH_PORT, Auth::default(), PRODUCT)
}

fn wanted_realms(target: &Target) -> Vec<String> {
    tag(target, "realms")
        .map(|list| {
            list.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
        })
        .unwrap_or_default()
}

/// Un jeton d'accès par le flux « client credentials ».
async fn admin_token(
    client: &HttpClient,
    realm: &str,
    client_id: &str,
    secret: &str,
) -> Result<String, ProbeError> {
    let path = format!("/realms/{}/protocol/openid-connect/token", encode(realm));
    let form =
        [("grant_type", "client_credentials"), ("client_id", client_id), ("client_secret", secret)];
    let (status, _, body) = client.login(&path, Some(&form[..]), None).await?;
    if matches!(status.as_u16(), 400 | 401 | 403) {
        return Err(ProbeError::Auth(format!(
            "Keycloak refused the client credentials ({status}): check the client ID and secret, \
             that \"Client authentication\" and \"Service accounts roles\" are on, and the \
             realm \"{realm}\"."
        )));
    }
    if !status.is_success() {
        return Err(client.status_error(status, &body, &path));
    }
    let token: Value = client.decode(&body, &path)?;
    token
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ProbeError::Protocol("Keycloak sent no access token.".to_string()))
}

/// Un `GET` de l'API d'administration. Un 403 ou un 404 saute la page. Un 401
/// sur un jeton qui vient d'être délivré est, depuis Keycloak 26.8.2, l'audience
/// manquante (client scope `kc-admin-api`) : le message le dit.
async fn admin_get(admin: &HttpClient, path: &str) -> Result<Option<Value>, ProbeError> {
    let (status, body) = admin.get(path).await?;
    match status {
        StatusCode::UNAUTHORIZED => Err(ProbeError::Auth(format!(
            "Keycloak issued a token, but its admin API refused it on {path} (401). Since \
             Keycloak 26.8.2 the client needs the kc-admin-api client scope: add it as Default \
             on the client's Client scopes tab."
        ))),
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => Ok(None),
        status if !status.is_success() => Err(admin.status_error(status, &body, path)),
        _ => admin.decode(&body, path).map(Some),
    }
}

/// Le message d'un compte de service à qui `/admin/realms` ne montre rien.
const NO_REALM: &str =
    "Keycloak shows the service account no realm on /admin/realms: give it the view-realm role.";

async fn admin_samples(
    target: &Target,
    client_id: &str,
    secret: &str,
    ts_ms: i64,
    errors: &mut u32,
) -> Result<Vec<Sample>, ProbeError> {
    let anonymous = http_client(target, "http", DEFAULT_PORT, Auth::default(), PRODUCT)?;
    let realm = tag(target, "auth_realm").unwrap_or(DEFAULT_AUTH_REALM);
    let token = admin_token(&anonymous, realm, client_id, secret).await?;
    let admin = http_client(
        target,
        "http",
        DEFAULT_PORT,
        Auth::header("Authorization", format!("Bearer {token}")),
        PRODUCT,
    )?;
    // Lisible veut dire que l'API d'administration a accepté le jeton, pas
    // seulement que Keycloak l'a délivré.
    let realms = match admin_get(&admin, "/admin/realms").await {
        Ok(Some(realms)) => Ok(realms),
        Ok(None) => Err(ProbeError::Auth(NO_REALM.to_string())),
        Err(error) => Err(error),
    };
    let mut out = vec![flag(&metric(P, "admin_readable"), realms.is_ok(), ts_ms)];
    let Some(realms) = settle(realms, errors, target.id, "realms") else { return Ok(out) };
    let info = admin_get(&admin, "/admin/serverinfo").await;
    if let Some(Some(info)) = settle(info, errors, target.id, "serverinfo") {
        out.extend(serverinfo_samples(&info, ts_ms));
    }
    let (counts, names) = realm_samples(&realms, &wanted_realms(target), ts_ms);
    out.extend(counts);

    let admin_ref = &admin;
    let details = futures::future::join_all(names.iter().map(|name| async move {
        let encoded = encode(name);
        let stats_path = format!("/admin/realms/{encoded}/client-session-stats");
        let users_path = format!("/admin/realms/{encoded}/users/count");
        let (sessions, users) = futures::join!(
            admin_get(admin_ref, &stats_path),
            admin_get(admin_ref, &users_path),
        );
        // `[]` : aucun client n'a de session, ou le compte ne voit pas les clients.
        let sessions = match sessions {
            Ok(Some(list)) if list.as_array().is_some_and(Vec::is_empty) => {
                let clients_path = format!("/admin/realms/{encoded}/clients?first=0&max=1");
                match admin_get(admin_ref, &clients_path).await {
                    Ok(clients) if clients_viewable(clients.as_ref()) => Ok(Some(list)),
                    Ok(_) => Err(ProbeError::Auth(format!(
                        "Keycloak hides the sessions of realm \"{name}\" from the service \
                         account: give it the view-clients role on that realm to count them."
                    ))),
                    Err(error) => Err(error),
                }
            }
            other => other,
        };
        (name, sessions, users)
    }))
    .await;
    let mut total_active = 0.0;
    let mut readable = false;
    for (name, sessions, users) in details {
        let sessions = settle(sessions, errors, target.id, "client-session-stats").flatten();
        let users = settle(users, errors, target.id, "users/count").flatten();
        let counts = sessions.as_ref().map(session_counts);
        if let Some((active, _)) = counts {
            total_active += active;
            readable = true;
        }
        out.extend(realm_detail_samples(
            name,
            counts,
            users.as_ref().and_then(|u| number(Some(u))),
            ts_ms,
        ));
    }
    if readable {
        out.push(gauge(&metric(P, "sessions_active"), total_active, ts_ms));
    }
    Ok(out)
}

pub async fn probe(target: &Target, ts_ms: i64) -> Result<Vec<Sample>, ProbeError> {
    let health = health_client(target)?;
    let mut errors = 0u32;
    // La preuve de vie : le seul appel de santé qui condamne.
    let ready: Value = health.get_json_even_on_error("/health/ready").await?;
    let mut out = health_samples("ready", &ready, ts_ms);
    let live = health.get_json_even_on_error::<Value>("/health/live").await;
    if let Some(live) = settle(live, &mut errors, target.id, "health/live") {
        out.extend(health_samples("live", &live, ts_ms));
    }
    match &target.credential {
        Credential::None => {}
        Credential::UsernamePassword { username, password } => {
            out.extend(admin_samples(target, username, password, ts_ms, &mut errors).await?);
        }
        other => {
            return Err(ProbeError::Config(format!(
                "Keycloak expects the client ID and secret of a service account (or nothing, \
                 for the health checks only), configured: {other}"
            )));
        }
    }
    out.push(gauge(&metric(P, "scrape_errors"), f64::from(errors), ts_ms));
    Ok(out)
}

// ------------------------------------------------------------ collecteur

#[derive(Default)]
pub struct KeycloakCollector;

impl KeycloakCollector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Collector for KeycloakCollector {
    fn kind(&self) -> &str {
        "keycloak"
    }

    async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        probe(target, now_ms()).await
    }

    async fn discover(&self, target: &Target) -> Result<Option<String>, ProbeError> {
        let _: Value = health_client(target)?.get_json_even_on_error("/health/ready").await?;
        Ok(Some("keycloak".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use axum::Router;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::{get, post};

    use super::*;

    // Réponses d'un Keycloak 26.0.5 : santé de l'interface de gestion (prête,
    // puis base de données tombée), `serverinfo`, trois domaines dont un
    // désactivé, statistiques de sessions du domaine « homelab », et le refus
    // d'un client dont le secret est faux.
    const READY: &str = include_str!("testdata/keycloak_26.0.5/health_ready.json");
    const READY_DOWN: &str = include_str!("testdata/keycloak_26.0.5/health_ready_down.json");
    const LIVE: &str = include_str!("testdata/keycloak_26.0.5/health_live.json");
    const SERVERINFO: &str = include_str!("testdata/keycloak_26.0.5/serverinfo.json");
    const REALMS: &str = include_str!("testdata/keycloak_26.0.5/realms.json");
    const SESSIONS: &str =
        include_str!("testdata/keycloak_26.0.5/client_session_stats_homelab.json");
    const TOKEN: &str = include_str!("testdata/keycloak_26.0.5/token.json");
    const TOKEN_REFUSED: &str = include_str!("testdata/keycloak_26.0.5/token_refused.json");

    // Keycloak 26.8.2, d'après le relevé d'une instance réelle : les domaines
    // `master`, `labrealm` (3 utilisateurs, 6 sessions) et `lockedrealm`
    // (désactivé), les statistiques de sessions de `labrealm` vues avec les
    // bons rôles, puis `[]` (200) pour le même domaine sans `view-clients`.
    // `admin_unauthorized.json` : le corps du 401 de l'API d'administration
    // quand le jeton n'a pas l'audience `kc-admin-api` (le motif n'est écrit
    // que dans le journal de Keycloak). Les listes de clients suivent
    // `ClientRepresentation` de l'API d'administration (champ `access` posé
    // par `ClientsResource.getClients`) : un client visible, puis la liste
    // vide qu'obtient un compte qui peut lister sans voir. `serverinfo_limited`
    // : la page sans `systemInfo` ni `memoryInfo`, servie sans `manage-realm`.
    const REALMS_26: &str = include_str!("testdata/keycloak_26.8.2/realms.json");
    const SESSIONS_LAB: &str =
        include_str!("testdata/keycloak_26.8.2/client_session_stats_labrealm.json");
    const SESSIONS_HIDDEN: &str =
        include_str!("testdata/keycloak_26.8.2/client_session_stats_hidden.json");
    const CLIENTS_VIEWABLE: &str = include_str!("testdata/keycloak_26.8.2/clients_viewable.json");
    const CLIENTS_HIDDEN: &str = include_str!("testdata/keycloak_26.8.2/clients_hidden.json");
    const ADMIN_UNAUTHORIZED: &str =
        include_str!("testdata/keycloak_26.8.2/admin_unauthorized.json");
    const SERVERINFO_LIMITED: &str =
        include_str!("testdata/keycloak_26.8.2/serverinfo_limited.json");

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn value(samples: &[Sample], name: &str) -> Option<f64> {
        samples.iter().find(|s| s.metric == name).map(|s| s.value)
    }

    fn realm(samples: &[Sample], name: &str, realm: &str) -> Option<f64> {
        samples
            .iter()
            .find(|s| s.metric == name && s.labels.get("realm").map(String::as_str) == Some(realm))
            .map(|s| s.value)
    }

    #[test]
    fn la_base_de_donnees_tombee_se_voit_dans_les_verifications() {
        let samples = health_samples("ready", &json(READY_DOWN), 0);
        assert_eq!(value(&samples, "keycloak_ready"), Some(0.0));
        let check = samples.iter().find(|s| s.metric == "keycloak_health_check_up").unwrap();
        assert_eq!(check.value, 0.0);
        assert!(check.labels["check"].contains("database"));
        let samples = health_samples("ready", &json(READY), 0);
        assert_eq!(value(&samples, "keycloak_ready"), Some(1.0));
        let live = health_samples("live", &json(LIVE), 0);
        assert_eq!(live.len(), 1);
        assert_eq!(value(&live, "keycloak_live"), Some(1.0));
    }

    #[test]
    fn serverinfo_donne_version_duree_de_vie_et_memoire() {
        let samples = serverinfo_samples(&json(SERVERINFO), 0);
        let info = samples.iter().find(|s| s.metric == "keycloak_version_info").unwrap();
        assert_eq!(info.labels["version"], "26.0.5");
        assert_eq!(value(&samples, "keycloak_uptime_seconds"), Some(443_528.0));
        assert_eq!(value(&samples, "keycloak_memory_used_percent"), Some(75.0));
    }

    #[test]
    fn les_domaines_se_comptent_et_se_filtrent() {
        let (samples, names) = realm_samples(&json(REALMS), &[], 0);
        assert_eq!(value(&samples, "keycloak_realms_total"), Some(3.0));
        assert_eq!(value(&samples, "keycloak_realms_disabled"), Some(1.0));
        assert_eq!(names, ["master", "homelab", "old test"]);
        let (_, names) = realm_samples(&json(REALMS), &["homelab".to_string()], 0);
        assert_eq!(names, ["homelab"]);
    }

    #[test]
    fn les_sessions_arrivent_en_texte_et_se_somment() {
        let counts = session_counts(&json(SESSIONS));
        assert_eq!(counts, (13.0, 1.0));
        let samples = realm_detail_samples("homelab", Some(counts), Some(42.0), 0);
        assert_eq!(realm(&samples, "keycloak_realm_sessions", "homelab"), Some(13.0));
        assert_eq!(realm(&samples, "keycloak_realm_offline_sessions", "homelab"), Some(1.0));
        assert_eq!(realm(&samples, "keycloak_realm_users", "homelab"), Some(42.0));
        assert!(realm_detail_samples("x", None, None, 0).is_empty());
        assert_eq!(session_counts(&json(SESSIONS_LAB)), (6.0, 0.0));
        assert_eq!(session_counts(&json(SESSIONS_HIDDEN)), (0.0, 0.0));
    }

    #[test]
    fn des_clients_caches_rendent_les_sessions_illisibles() {
        assert!(clients_viewable(Some(&json(CLIENTS_VIEWABLE))));
        assert!(!clients_viewable(Some(&json(CLIENTS_HIDDEN))));
        assert!(!clients_viewable(None), "403 on the client list");
        let refused = json(r#"[{"clientId":"account","access":{"view":false}}]"#);
        assert!(!clients_viewable(Some(&refused)));
    }

    #[test]
    fn sans_manage_realm_serverinfo_ne_donne_ni_version_ni_memoire() {
        assert!(serverinfo_samples(&json(SERVERINFO_LIMITED), 0).is_empty());
    }

    #[test]
    fn l_adresse_de_gestion_garde_l_hote_seulement() {
        assert_eq!(host_only("sso.lan"), "sso.lan");
        assert_eq!(host_only("sso.lan:8080"), "sso.lan");
        assert_eq!(host_only("https://sso.example.com/auth"), "sso.example.com");
        assert_eq!(host_only("[fd00::1]:8443"), "[fd00::1]");
        assert_eq!(host_only("fd00::1"), "fd00::1");
        assert_eq!(encode("old test"), "old%20test");
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn target(address: String, credential: Credential, tags: &[(&str, &str)]) -> Target {
        Target {
            id: 6,
            name: "sso".into(),
            address,
            kind: "keycloak".into(),
            profile_id: None,
            parent_id: None,
            interval: Duration::from_secs(60),
            enabled: true,
            tags: tags
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<_, _>>(),
            credential,
            group_name: String::new(),
            position: 0,
        }
    }

    fn secret(password: &str) -> Credential {
        Credential::UsernamePassword { username: "dumbmonit".into(), password: password.into() }
    }

    fn app() -> Router {
        Router::new()
            .route("/health/ready", get(|| async { READY }))
            .route("/health/live", get(|| async { LIVE }))
            .route(
                "/realms/master/protocol/openid-connect/token",
                post(|body: String| async move {
                    if body.contains("client_secret=bon") {
                        (StatusCode::OK, TOKEN)
                    } else {
                        (StatusCode::UNAUTHORIZED, TOKEN_REFUSED)
                    }
                }),
            )
            .route(
                "/admin/serverinfo",
                get(|headers: HeaderMap| async move {
                    let bearer = headers.get("authorization").and_then(|v| v.to_str().ok());
                    if bearer.is_some_and(|b| b.starts_with("Bearer eyJ")) {
                        (StatusCode::OK, SERVERINFO)
                    } else {
                        (StatusCode::UNAUTHORIZED, "{}")
                    }
                }),
            )
            .route("/admin/realms", get(|| async { REALMS }))
            .route("/admin/realms/homelab/client-session-stats", get(|| async { SESSIONS }))
            .route("/admin/realms/homelab/users/count", get(|| async { "42" }))
            // Les autres domaines : le compte de service n'a pas le droit d'y lire.
            .fallback(|| async { (StatusCode::FORBIDDEN, r#"{"error":"HTTP 403 Forbidden"}"#) })
    }

    #[tokio::test]
    async fn la_sante_seule_se_lit_sans_identifiants() {
        let base = serve(app()).await;
        let tags = [("health_port", "same")];
        let samples =
            KeycloakCollector::new().probe(&target(base, Credential::None, &tags)).await.unwrap();
        assert_eq!(value(&samples, "keycloak_ready"), Some(1.0));
        assert_eq!(value(&samples, "keycloak_live"), Some(1.0));
        assert!(value(&samples, "keycloak_admin_readable").is_none());
        assert_eq!(value(&samples, "keycloak_scrape_errors"), Some(0.0));
    }

    #[tokio::test]
    async fn l_administration_donne_domaines_sessions_et_utilisateurs() {
        let base = serve(app()).await;
        let tags = [("health_port", "same"), ("realms", "homelab, master")];
        let samples =
            KeycloakCollector::new().probe(&target(base, secret("bon"), &tags)).await.unwrap();
        assert_eq!(value(&samples, "keycloak_admin_readable"), Some(1.0));
        assert_eq!(value(&samples, "keycloak_realms_total"), Some(3.0));
        assert_eq!(realm(&samples, "keycloak_realm_sessions", "homelab"), Some(13.0));
        assert_eq!(realm(&samples, "keycloak_realm_users", "homelab"), Some(42.0));
        // « master » est interdit au compte de service : sauté, jamais une panne.
        assert!(realm(&samples, "keycloak_realm_sessions", "master").is_none());
        assert_eq!(value(&samples, "keycloak_sessions_active"), Some(13.0));
        assert_eq!(value(&samples, "keycloak_uptime_seconds"), Some(443_528.0));
    }

    #[tokio::test]
    async fn un_secret_refuse_est_une_erreur_d_authentification() {
        let base = serve(app()).await;
        let tags = [("health_port", "same")];
        let error =
            KeycloakCollector::new().probe(&target(base, secret("faux"), &tags)).await.unwrap_err();
        assert!(matches!(error, ProbeError::Auth(_)));
        assert!(!error.means_down());
    }

    /// Un Keycloak 26.8.2 : le jeton est délivré, puis l'API d'administration
    /// le refuse (401) faute de client scope `kc-admin-api`.
    fn app_audience_refusee() -> Router {
        Router::new()
            .route("/health/ready", get(|| async { READY }))
            .route("/health/live", get(|| async { LIVE }))
            .route("/realms/master/protocol/openid-connect/token", post(|| async { TOKEN }))
            .fallback(|| async { (StatusCode::UNAUTHORIZED, ADMIN_UNAUTHORIZED) })
    }

    #[tokio::test]
    async fn un_jeton_refuse_par_l_administration_n_est_pas_lisible() {
        let base = serve(app_audience_refusee()).await;
        let tags = [("health_port", "same")];
        let target = target(base, secret("bon"), &tags);
        let samples = KeycloakCollector::new().probe(&target).await.unwrap();
        assert_eq!(value(&samples, "keycloak_ready"), Some(1.0));
        assert_eq!(value(&samples, "keycloak_admin_readable"), Some(0.0));
        assert!(value(&samples, "keycloak_realms_total").is_none());
        assert!(value(&samples, "keycloak_uptime_seconds").is_none());
        // `/admin/realms` seul : `serverinfo` n'est pas tenté après le refus.
        assert_eq!(value(&samples, "keycloak_scrape_errors"), Some(1.0));

        let admin = http_client(&target, "http", DEFAULT_PORT, Auth::default(), PRODUCT).unwrap();
        let error = admin_get(&admin, "/admin/realms").await.unwrap_err();
        assert!(matches!(error, ProbeError::Auth(ref m) if m.contains("kc-admin-api")), "{error}");
    }

    /// Les domaines du relevé : `master` sans session (clients visibles),
    /// `labrealm` avec ou sans le rôle `view-clients`.
    fn app_sessions(view_clients: bool) -> Router {
        let (stats, clients) = if view_clients {
            (SESSIONS_LAB, CLIENTS_VIEWABLE)
        } else {
            (SESSIONS_HIDDEN, CLIENTS_HIDDEN)
        };
        Router::new()
            .route("/health/ready", get(|| async { READY }))
            .route("/health/live", get(|| async { LIVE }))
            .route("/realms/master/protocol/openid-connect/token", post(|| async { TOKEN }))
            .route("/admin/serverinfo", get(|| async { SERVERINFO_LIMITED }))
            .route("/admin/realms", get(|| async { REALMS_26 }))
            .route("/admin/realms/master/client-session-stats", get(|| async { SESSIONS_HIDDEN }))
            .route("/admin/realms/master/clients", get(|| async { CLIENTS_VIEWABLE }))
            .route("/admin/realms/labrealm/client-session-stats", get(move || async move { stats }))
            .route("/admin/realms/labrealm/clients", get(move || async move { clients }))
            .route("/admin/realms/labrealm/users/count", get(|| async { "3" }))
            .fallback(|| async { (StatusCode::FORBIDDEN, r#"{"error":"HTTP 403 Forbidden"}"#) })
    }

    #[tokio::test]
    async fn les_sessions_cachees_ne_valent_pas_zero() {
        let tags = [("health_port", "same")];
        let base = serve(app_sessions(true)).await;
        let samples =
            KeycloakCollector::new().probe(&target(base, secret("bon"), &tags)).await.unwrap();
        assert_eq!(value(&samples, "keycloak_admin_readable"), Some(1.0));
        assert_eq!(value(&samples, "keycloak_realms_total"), Some(3.0));
        assert_eq!(value(&samples, "keycloak_realms_disabled"), Some(1.0));
        assert_eq!(realm(&samples, "keycloak_realm_sessions", "labrealm"), Some(6.0));
        assert_eq!(realm(&samples, "keycloak_realm_users", "labrealm"), Some(3.0));
        // Aucune session dans `master`, et des clients visibles : un vrai zéro.
        assert_eq!(realm(&samples, "keycloak_realm_sessions", "master"), Some(0.0));
        assert_eq!(value(&samples, "keycloak_sessions_active"), Some(6.0));
        // Sans `manage-realm`, ni version ni durée de vie : rien n'est inventé.
        assert!(value(&samples, "keycloak_version_info").is_none());
        assert_eq!(value(&samples, "keycloak_scrape_errors"), Some(0.0));

        let base = serve(app_sessions(false)).await;
        let samples =
            KeycloakCollector::new().probe(&target(base, secret("bon"), &tags)).await.unwrap();
        assert!(realm(&samples, "keycloak_realm_sessions", "labrealm").is_none());
        assert_eq!(realm(&samples, "keycloak_realm_users", "labrealm"), Some(3.0));
        assert_eq!(realm(&samples, "keycloak_realm_sessions", "master"), Some(0.0));
        assert_eq!(value(&samples, "keycloak_scrape_errors"), Some(1.0));
    }

    #[tokio::test]
    async fn la_sante_en_echec_reste_une_mesure_et_l_injoignable_une_panne() {
        let app = Router::new()
            .route("/health/ready", get(|| async { (StatusCode::SERVICE_UNAVAILABLE, READY_DOWN) }))
            .route("/health/live", get(|| async { LIVE }));
        let base = serve(app).await;
        let tags = [("health_port", "same")];
        let samples =
            KeycloakCollector::new().probe(&target(base, Credential::None, &tags)).await.unwrap();
        assert_eq!(value(&samples, "keycloak_ready"), Some(0.0));

        let error = KeycloakCollector::new()
            .probe(&target("127.0.0.1".into(), Credential::None, &[("health_port", "1")]))
            .await
            .unwrap_err();
        assert!(error.means_down());
        let error = KeycloakCollector::new()
            .probe(&target("127.0.0.1".into(), Credential::None, &[("health_port", "neuf")]))
            .await
            .unwrap_err();
        assert!(matches!(error, ProbeError::Config(_)));
    }
}
