//! authentik.
//!
//! Un jeton d'API (`Authorization: Bearer …`) d'un compte de service en
//! lecture seule, et les routes d'administration de l'API v3 :
//! `/api/v3/admin/version/`, les travailleurs et les tâches,
//! `/outposts/instances/` et `/core/users/`. Les sondes de santé
//! (`/-/health/live/` et `/-/health/ready/`) sont publiques.
//!
//! Travailleurs et tâches ont changé d'adresse avec le passage à Dramatiq
//! (authentik 2025.10) : `/api/v3/tasks/workers/` (champ `version_matching`)
//! et `/api/v3/tasks/schedules/` (l'état de la dernière tâche de chaque
//! planification, `last_task_status`) remplacent `/api/v3/admin/workers/`
//! (`version_matches`) et `/api/v3/admin/system_tasks/`. La nouvelle route est
//! essayée d'abord, l'ancienne seulement sur un 404. `/api/v3/tasks/tasks/`
//! n'est pas lu : c'est l'historique de chaque exécution, où une erreur
//! ancienne reste après que la tâche a de nouveau réussi.
//!
//! Aucune page manquante ne passe en silence : un 404 sur les deux routes, ou
//! une page HTML à la place du JSON, compte dans `authentik_scrape_errors`
//! avec un message qui le dit. Seul un refus (403) saute la page.
//!
//! Les pannes que ce module existe pour voir : **un travailleur arrêté** (plus
//! de synchronisation LDAP, plus de courriels, plus de nettoyage des sessions,
//! sans rien d'autre qu'un compteur dans l'administration), **une tâche
//! planifiée en échec** (synchronisation d'une source, blueprint non appliqué)
//! et **un avant-poste déconnecté** (les applications qu'il protège ne
//! répondent plus). Les trois sont facultatives : une page que le jeton ne
//! peut pas lire est sautée.

use async_trait::async_trait;
use dumbmonit_proto::{Collector, ProbeError, Sample, Target};
use reqwest::StatusCode;
use serde_json::Value;

use super::client::{Auth, HttpClient};
use super::{
    MAX_NAMED, flag, gauge, http_client, instant, metric, now_ms, number, optional_json, settle,
    token,
};

pub const DEFAULT_PORT: u16 = 9000;
const PRODUCT: &str = "authentik";
const P: &str = "authentik";

/// Un avant-poste est connecté s'il a donné signe de vie dans ce délai.
pub const OUTPOST_FRESH_SECONDS: i64 = 300;

/// Au plus autant d'avant-postes interrogés par sonde (un appel chacun).
pub const MAX_OUTPOSTS: usize = 10;

/// Les travailleurs : la route d'authentik 2025.10 et suivants, puis l'ancienne.
pub const WORKERS: [&str; 2] = ["/api/v3/tasks/workers/", "/api/v3/admin/workers/"];

/// Les tâches : les planifications d'authentik 2025.10 et suivants (une page
/// suffit, une installation en compte une vingtaine), puis l'ancienne liste.
pub const TASKS: [&str; 2] =
    ["/api/v3/tasks/schedules/?page_size=100", "/api/v3/admin/system_tasks/"];

// ------------------------------------------------------------ traduction

/// `GET /api/v3/admin/version/`.
pub fn version_samples(body: &Value, ts_ms: i64) -> Vec<Sample> {
    let current = body.get("version_current").and_then(Value::as_str).unwrap_or("unknown");
    let mut out =
        vec![gauge(&metric(P, "version_info"), 1.0, ts_ms).with_label("version", current)];
    // `outdated` n'a de sens que si authentik a pu lire la dernière version
    // publiée (`version_latest_valid`) : sans accès sortant, il se tait.
    if body.get("version_latest_valid").and_then(Value::as_bool) == Some(true) {
        let latest = body.get("version_latest").and_then(Value::as_str).unwrap_or("");
        let outdated = body.get("outdated").and_then(Value::as_bool).unwrap_or(false);
        out.push(
            flag(&metric(P, "update_available"), outdated, ts_ms)
                .with_label("latest_version", latest),
        );
    }
    out
}

/// Les deux sondes de santé : un code 2xx est « prêt ».
pub fn health_samples(live: bool, ready: bool, ts_ms: i64) -> Vec<Sample> {
    vec![flag(&metric(P, "live"), live, ts_ms), flag(&metric(P, "ready"), ready, ts_ms)]
}

/// `GET /api/v3/tasks/workers/` : `[{"worker_id", "version", "version_matching"}]`,
/// ou avant 2025.10 `GET /api/v3/admin/workers/`, où le champ s'appelait
/// `version_matches`.
pub fn worker_samples(body: &Value, ts_ms: i64) -> Vec<Sample> {
    let Some(workers) = body.as_array() else { return Vec::new() };
    let matching = |w: &Value| {
        w.get("version_matching").or_else(|| w.get("version_matches")).and_then(Value::as_bool)
    };
    let mismatched = workers.iter().filter(|w| matching(w) == Some(false));
    vec![
        gauge(&metric(P, "workers"), workers.len() as f64, ts_ms),
        gauge(&metric(P, "workers_version_mismatch"), mismatched.count() as f64, ts_ms),
    ]
}

/// Un champ texte, vide s'il manque.
fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Les compteurs de tâches et une série par tâche en échec.
fn task_counts(failed: &[&str], warned: usize, ts_ms: i64) -> Vec<Sample> {
    let mut out = vec![
        gauge(&metric(P, "tasks_failed"), failed.len() as f64, ts_ms),
        gauge(&metric(P, "tasks_warning"), warned as f64, ts_ms),
    ];
    for name in failed.iter().take(MAX_NAMED) {
        out.push(gauge(&metric(P, "task_failed"), 1.0, ts_ms).with_label("task", *name));
    }
    out
}

/// `GET /api/v3/admin/system_tasks/` (avant 2025.10) : la dernière exécution
/// de chaque tâche, `SUCCESSFUL`, `WARNING`, `ERROR` ou `UNKNOWN`.
pub fn task_samples(body: &Value, ts_ms: i64) -> Vec<Sample> {
    let Some(tasks) = body.as_array() else { return Vec::new() };
    let failed: Vec<&str> = tasks
        .iter()
        .filter(|t| field(t, "status") == "ERROR")
        .map(|t| t.get("task_name").and_then(Value::as_str).unwrap_or("unknown"))
        .collect();
    let warned = tasks.iter().filter(|t| field(t, "status") == "WARNING").count();
    task_counts(&failed, warned, ts_ms)
}

/// `GET /api/v3/tasks/schedules/` (2025.10 et suivants) : chaque planification
/// avec l'état agrégé de sa dernière tâche, `last_task_status` (`done`,
/// `info`, `warning`, `error`, un état en cours, ou `null` si elle n'a jamais
/// tourné). Une planification en pause est ignorée : son dernier échec est
/// connu et voulu. Le nom publié est `uid` (l'acteur, suivi de l'objet
/// concerné : `…ldap_sync:ldap-corp`), pour distinguer deux sources.
pub fn schedule_samples(body: &Value, ts_ms: i64) -> Vec<Sample> {
    let Some(schedules) = body.get("results").and_then(Value::as_array) else { return Vec::new() };
    let active: Vec<&Value> = schedules
        .iter()
        .filter(|s| s.get("paused").and_then(Value::as_bool) != Some(true))
        .collect();
    let failed: Vec<&str> = active
        .iter()
        .filter(|s| field(s, "last_task_status") == "error")
        .map(|s| match field(s, "uid") {
            "" => field(s, "actor_name"),
            uid => uid,
        })
        .collect();
    let warned = active.iter().filter(|s| field(s, "last_task_status") == "warning").count();
    task_counts(&failed, warned, ts_ms)
}

/// Le nombre d'instances d'un avant-poste vues récemment, d'après
/// `GET /outposts/instances/{pk}/health/`.
pub fn fresh_instances(health: &Value, now_s: i64) -> usize {
    health
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|h| {
            h.get("last_seen")
                .and_then(Value::as_str)
                .and_then(instant)
                .is_some_and(|seen| now_s - seen <= OUTPOST_FRESH_SECONDS)
        })
        .count()
}

/// Les avant-postes qui servent au moins un fournisseur : `(pk, nom)`. Un
/// avant-poste sans fournisseur n'est pas censé tourner.
pub fn used_outposts(body: &Value) -> Vec<(String, String)> {
    body.get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|o| o.get("providers").and_then(Value::as_array).is_some_and(|p| !p.is_empty()))
        .filter_map(|o| {
            Some((o.get("pk")?.as_str()?.to_string(), o.get("name")?.as_str()?.to_string()))
        })
        .take(MAX_OUTPOSTS)
        .collect()
}

pub fn outpost_samples(connected: &[(String, usize)], ts_ms: i64) -> Vec<Sample> {
    let mut out = vec![
        gauge(&metric(P, "outposts_total"), connected.len() as f64, ts_ms),
        gauge(
            &metric(P, "outposts_disconnected"),
            connected.iter().filter(|(_, n)| *n == 0).count() as f64,
            ts_ms,
        ),
    ];
    for (name, instances) in connected {
        out.push(
            gauge(&metric(P, "outpost_instances"), *instances as f64, ts_ms)
                .with_label("outpost", name),
        );
    }
    out
}

// ------------------------------------------------------------------ sonde

/// Un code HTTP de sonde de santé : vrai pour un 2xx, faux pour le reste,
/// sans erreur (un 503 est la réponse).
async fn health(client: &HttpClient, path: &str) -> Result<bool, ProbeError> {
    Ok(client.get(path).await?.0.is_success())
}

/// Le JSON d'une réponse, avec un message clair quand authentik (ou un
/// mandataire placé devant) envoie une page web à la place.
fn json_body(client: &HttpClient, body: &str, path: &str) -> Result<Value, ProbeError> {
    if body.trim_start().starts_with('<') {
        return Err(ProbeError::Protocol(format!(
            "authentik sent a web page instead of JSON on {path}: this version may not have \
             that route, or a reverse proxy answers in its place."
        )));
    }
    client.decode(body, path)
}

/// La première de ces routes qui existe, dans l'ordre : `Some((rang, corps))`,
/// ou `None` sur un refus (403 : la page est sautée). Un 404 passe à la route
/// suivante ; un 404 sur toutes est une erreur, jamais un silence.
async fn first_route(
    client: &HttpClient,
    paths: &[&str],
) -> Result<Option<(usize, Value)>, ProbeError> {
    for (index, path) in paths.iter().enumerate() {
        let (status, body) = client.get(path).await?;
        match status {
            StatusCode::NOT_FOUND => continue,
            StatusCode::FORBIDDEN => return Ok(None),
            status if !status.is_success() => return Err(client.status_error(status, &body, path)),
            _ => return json_body(client, &body, path).map(|value| Some((index, value))),
        }
    }
    Err(ProbeError::Protocol(format!(
        "authentik answered 404 on {}: this version has none of these routes.",
        paths.join(" and ")
    )))
}

pub async fn probe(
    client: &HttpClient,
    target: &Target,
    ts_ms: i64,
) -> Result<Vec<Sample>, ProbeError> {
    // La preuve de vie et d'authentification : le seul appel qui condamne.
    let version: Value = client.get_json("/api/v3/admin/version/").await?;
    let mut out = version_samples(&version, ts_ms);
    let mut errors = 0u32;
    let (live, ready, workers, tasks, outposts, users) = futures::join!(
        health(client, "/-/health/live/"),
        health(client, "/-/health/ready/"),
        first_route(client, &WORKERS),
        first_route(client, &TASKS),
        optional_json(client, "/api/v3/outposts/instances/?page_size=100"),
        optional_json(client, "/api/v3/core/users/?page_size=1"),
    );
    let live = settle(live, &mut errors, target.id, "health/live");
    let ready = settle(ready, &mut errors, target.id, "health/ready");
    if let (Some(live), Some(ready)) = (live, ready) {
        out.extend(health_samples(live, ready, ts_ms));
    }
    if let Some(Some((_, body))) = settle(workers, &mut errors, target.id, "workers") {
        out.extend(worker_samples(&body, ts_ms));
    }
    match settle(tasks, &mut errors, target.id, "tasks") {
        Some(Some((0, body))) => out.extend(schedule_samples(&body, ts_ms)),
        Some(Some((_, body))) => out.extend(task_samples(&body, ts_ms)),
        _ => {}
    }
    if let Some(Some(body)) = settle(users, &mut errors, target.id, "core/users")
        && let Some(count) = number(body.pointer("/pagination/count"))
    {
        out.push(gauge(&metric(P, "users"), count, ts_ms));
    }
    if let Some(Some(body)) = settle(outposts, &mut errors, target.id, "outposts/instances") {
        let now_s = ts_ms / 1000;
        let paths: Vec<(String, String)> = used_outposts(&body)
            .into_iter()
            .map(|(pk, name)| (format!("/api/v3/outposts/instances/{pk}/health/"), name))
            .collect();
        let healths =
            futures::future::join_all(paths.iter().map(|(path, _)| optional_json(client, path)))
                .await;
        let mut connected = Vec::new();
        for ((_, name), health) in paths.iter().zip(healths) {
            if let Some(Some(body)) = settle(health, &mut errors, target.id, "outposts/health") {
                connected.push((name.clone(), fresh_instances(&body, now_s)));
            }
        }
        out.extend(outpost_samples(&connected, ts_ms));
    }
    out.push(gauge(&metric(P, "scrape_errors"), f64::from(errors), ts_ms));
    Ok(out)
}

// ------------------------------------------------------------ collecteur

#[derive(Default)]
pub struct AuthentikCollector;

impl AuthentikCollector {
    pub fn new() -> Self {
        Self
    }

    fn client(target: &Target) -> Result<HttpClient, ProbeError> {
        let Some(key) = token(&target.credential) else {
            return Err(ProbeError::Config(format!(
                "authentik expects an API token, configured: {}",
                target.credential
            )));
        };
        http_client(
            target,
            "http",
            DEFAULT_PORT,
            Auth::header("Authorization", format!("Bearer {key}")),
            PRODUCT,
        )
    }
}

#[async_trait]
impl Collector for AuthentikCollector {
    fn kind(&self) -> &str {
        "authentik"
    }

    async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        probe(&Self::client(target)?, target, now_ms()).await
    }

    async fn discover(&self, target: &Target) -> Result<Option<String>, ProbeError> {
        let _: Value = Self::client(target)?.get_json("/api/v3/admin/version/").await?;
        Ok(Some("authentik".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use axum::Router;
    use axum::http::{HeaderMap, StatusCode};
    use axum::routing::get;
    use dumbmonit_proto::Credential;

    use super::*;

    // Réponses d'un authentik 2025.8.4 : un travailleur en retard d'une version,
    // une synchronisation LDAP en échec, trois avant-postes dont un sans
    // fournisseur et un muet depuis la veille.
    const VERSION: &str = include_str!("testdata/authentik_2025.8.4/version.json");
    const VERSION_CURRENT: &str = include_str!("testdata/authentik_2025.8.4/version_current.json");
    const WORKERS: &str = include_str!("testdata/authentik_2025.8.4/workers.json");
    const TASKS: &str = include_str!("testdata/authentik_2025.8.4/system_tasks.json");
    const OUTPOSTS: &str = include_str!("testdata/authentik_2025.8.4/outposts.json");
    const HEALTH_EMBEDDED: &str =
        include_str!("testdata/authentik_2025.8.4/outpost_health_embedded.json");
    const HEALTH_LDAP: &str = include_str!("testdata/authentik_2025.8.4/outpost_health_ldap.json");
    const USERS: &str = include_str!("testdata/authentik_2025.8.4/users.json");

    // authentik 2026.8.5, après le passage aux routes `/api/v3/tasks/`.
    // `version.json` et `workers.json` : réponses relevées sur une instance
    // réelle (identifiant du travailleur complété, il était tronqué dans le
    // relevé). `workers_mismatch.json` y ajoute un travailleur resté sur une
    // autre version, et `schedules.json` suit le schéma `PaginatedScheduleList`
    // de l'OpenAPI officielle (goauthentik/authentik, `schema.yml`), avec les
    // noms d'acteurs complets qu'authentik publie : une synchronisation LDAP en
    // échec, une autre en échec mais en pause, un avertissement, une
    // planification réussie et une qui n'a jamais tourné. `not_found.html` :
    // la page web qu'authentik rend sur une route d'API qu'il n'a plus.
    const VERSION_2026: &str = include_str!("testdata/authentik_2026.8.5/version.json");
    const WORKERS_2026: &str = include_str!("testdata/authentik_2026.8.5/workers.json");
    const WORKERS_MISMATCH: &str =
        include_str!("testdata/authentik_2026.8.5/workers_mismatch.json");
    const SCHEDULES: &str = include_str!("testdata/authentik_2026.8.5/schedules.json");
    const NOT_FOUND: &str = include_str!("testdata/authentik_2026.8.5/not_found.html");

    // 2026-10-26T09:12:00Z
    const NOW: i64 = 1_793_005_920;

    fn json(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn value(samples: &[Sample], name: &str) -> Option<f64> {
        samples.iter().find(|s| s.metric == name).map(|s| s.value)
    }

    #[test]
    fn la_version_dit_si_une_mise_a_jour_existe() {
        let samples = version_samples(&json(VERSION), 0);
        assert_eq!(value(&samples, "authentik_update_available"), Some(1.0));
        let flag = samples.iter().find(|s| s.metric == "authentik_update_available").unwrap();
        assert_eq!(flag.labels["latest_version"], "2025.10.1");
        let info = samples.iter().find(|s| s.metric == "authentik_version_info").unwrap();
        assert_eq!(info.labels["version"], "2025.8.4");
        let samples = version_samples(&json(VERSION_CURRENT), 0);
        assert_eq!(value(&samples, "authentik_update_available"), Some(0.0));
        // Sans accès sortant, la dernière version est inconnue : aucun verdict.
        let blind = json(r#"{"version_current":"2025.8.4","version_latest_valid":false}"#);
        assert!(value(&version_samples(&blind, 0), "authentik_update_available").is_none());
    }

    #[test]
    fn un_travailleur_en_retard_se_compte() {
        let samples = worker_samples(&json(WORKERS), 0);
        assert_eq!(value(&samples, "authentik_workers"), Some(2.0));
        assert_eq!(value(&samples, "authentik_workers_version_mismatch"), Some(1.0));
        let samples = worker_samples(&json("[]"), 0);
        assert_eq!(value(&samples, "authentik_workers"), Some(0.0));
    }

    #[test]
    fn les_taches_en_echec_se_nomment() {
        let samples = task_samples(&json(TASKS), 0);
        assert_eq!(value(&samples, "authentik_tasks_failed"), Some(1.0));
        assert_eq!(value(&samples, "authentik_tasks_warning"), Some(1.0));
        let named = samples.iter().find(|s| s.metric == "authentik_task_failed").unwrap();
        assert!(named.labels["task"].ends_with("ldap_sync"));
    }

    #[test]
    fn les_travailleurs_des_routes_tasks_se_comptent() {
        let samples = worker_samples(&json(WORKERS_2026), 0);
        assert_eq!(value(&samples, "authentik_workers"), Some(1.0));
        assert_eq!(value(&samples, "authentik_workers_version_mismatch"), Some(0.0));
        let samples = worker_samples(&json(WORKERS_MISMATCH), 0);
        assert_eq!(value(&samples, "authentik_workers"), Some(2.0));
        assert_eq!(value(&samples, "authentik_workers_version_mismatch"), Some(1.0));
    }

    #[test]
    fn les_planifications_en_echec_se_nomment_et_celles_en_pause_se_taisent() {
        let samples = schedule_samples(&json(SCHEDULES), 0);
        assert_eq!(value(&samples, "authentik_tasks_failed"), Some(1.0));
        assert_eq!(value(&samples, "authentik_tasks_warning"), Some(1.0));
        let named: Vec<&str> = samples
            .iter()
            .filter(|s| s.metric == "authentik_task_failed")
            .map(|s| s.labels["task"].as_str())
            .collect();
        assert_eq!(named, ["authentik.sources.ldap.tasks.ldap_sync:ldap-corp"]);
    }

    #[test]
    fn un_avant_poste_muet_est_deconnecte_et_celui_sans_fournisseur_est_ignore() {
        let used = used_outposts(&json(OUTPOSTS));
        let names: Vec<&str> = used.iter().map(|(_, name)| name.as_str()).collect();
        assert_eq!(names, ["authentik Embedded Outpost", "ldap-outpost"]);
        assert_eq!(fresh_instances(&json(HEALTH_EMBEDDED), NOW), 1);
        assert_eq!(fresh_instances(&json(HEALTH_LDAP), NOW), 0);
        let samples =
            outpost_samples(&[("embedded".to_string(), 1), ("ldap-outpost".to_string(), 0)], 0);
        assert_eq!(value(&samples, "authentik_outposts_total"), Some(2.0));
        assert_eq!(value(&samples, "authentik_outposts_disconnected"), Some(1.0));
    }

    async fn serve(app: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{address}")
    }

    fn target(address: String, key: &str) -> Target {
        Target {
            id: 8,
            name: "sso".into(),
            address,
            kind: "authentik".into(),
            profile_id: None,
            parent_id: None,
            interval: Duration::from_secs(60),
            enabled: true,
            tags: BTreeMap::new(),
            credential: Credential::ApiToken { token: key.into() },
            group_name: String::new(),
            position: 0,
        }
    }

    fn app() -> Router {
        Router::new()
            .route(
                "/api/v3/admin/version/",
                get(|headers: HeaderMap| async move {
                    let ok = headers.get("authorization").and_then(|v| v.to_str().ok())
                        == Some("Bearer cle");
                    if ok {
                        (StatusCode::OK, VERSION)
                    } else {
                        (StatusCode::FORBIDDEN, r#"{"detail":"Token invalid/expired"}"#)
                    }
                }),
            )
            .route("/-/health/live/", get(|| async { StatusCode::NO_CONTENT }))
            .route(
                "/-/health/ready/",
                get(|| async { (StatusCode::SERVICE_UNAVAILABLE, "database") }),
            )
            .route("/api/v3/admin/workers/", get(|| async { WORKERS }))
            .route("/api/v3/admin/system_tasks/", get(|| async { TASKS }))
            .route("/api/v3/core/users/", get(|| async { USERS }))
            .route("/api/v3/outposts/instances/", get(|| async { OUTPOSTS }))
            .route(
                "/api/v3/outposts/instances/3f0f5a9e-1111-4a6e-9a0e-aaaaaaaaaaaa/health/",
                get(|| async { HEALTH_EMBEDDED }),
            )
            .route(
                "/api/v3/outposts/instances/3f0f5a9e-2222-4a6e-9a0e-bbbbbbbbbbbb/health/",
                get(|| async { HEALTH_LDAP }),
            )
    }

    #[tokio::test]
    async fn sonde_complete_avec_la_base_indisponible() {
        let base = serve(app()).await;
        let samples = AuthentikCollector::new().probe(&target(base, "cle")).await.unwrap();
        assert_eq!(value(&samples, "authentik_live"), Some(1.0));
        assert_eq!(value(&samples, "authentik_ready"), Some(0.0));
        assert_eq!(value(&samples, "authentik_workers"), Some(2.0));
        assert_eq!(value(&samples, "authentik_tasks_failed"), Some(1.0));
        assert_eq!(value(&samples, "authentik_users"), Some(37.0));
        assert_eq!(value(&samples, "authentik_outposts_total"), Some(2.0));
        // Les santés sont horodatées de 2026 : au moment du test, elles sont
        // toutes anciennes, quel que soit l'avant-poste.
        assert_eq!(value(&samples, "authentik_scrape_errors"), Some(0.0));
    }

    /// Un authentik 2026.8.5 : les anciennes routes répondent par la page web
    /// 404, les nouvelles par du JSON.
    fn app_2026() -> Router {
        let gone = || async { (StatusCode::NOT_FOUND, NOT_FOUND) };
        Router::new()
            .route("/api/v3/admin/version/", get(|| async { VERSION_2026 }))
            .route("/-/health/live/", get(|| async { StatusCode::OK }))
            .route("/-/health/ready/", get(|| async { StatusCode::OK }))
            .route("/api/v3/admin/workers/", get(gone))
            .route("/api/v3/admin/system_tasks/", get(gone))
            .route("/api/v3/tasks/workers/", get(|| async { WORKERS_2026 }))
            .route("/api/v3/tasks/schedules/", get(|| async { SCHEDULES }))
            .route("/api/v3/core/users/", get(|| async { USERS }))
    }

    #[tokio::test]
    async fn les_routes_tasks_d_authentik_recent_sont_lues() {
        let base = serve(app_2026()).await;
        let samples = AuthentikCollector::new().probe(&target(base, "cle")).await.unwrap();
        assert_eq!(value(&samples, "authentik_workers"), Some(1.0));
        assert_eq!(value(&samples, "authentik_workers_version_mismatch"), Some(0.0));
        assert_eq!(value(&samples, "authentik_tasks_failed"), Some(1.0));
        assert_eq!(value(&samples, "authentik_tasks_warning"), Some(1.0));
        assert_eq!(value(&samples, "authentik_update_available"), Some(0.0));
        assert_eq!(value(&samples, "authentik_scrape_errors"), Some(0.0));
    }

    #[tokio::test]
    async fn une_route_absente_ou_une_page_web_se_signale() {
        // Aucune des deux routes des tâches, et un mandataire qui répond par
        // une page web (200) sur celle des travailleurs.
        let app = Router::new()
            .route("/api/v3/admin/version/", get(|| async { VERSION_2026 }))
            .route("/-/health/live/", get(|| async { StatusCode::OK }))
            .route("/-/health/ready/", get(|| async { StatusCode::OK }))
            .route("/api/v3/tasks/workers/", get(|| async { NOT_FOUND }))
            .fallback(|| async { (StatusCode::NOT_FOUND, NOT_FOUND) });
        let base = serve(app).await;
        let target = target(base, "cle");
        let samples = AuthentikCollector::new().probe(&target).await.unwrap();
        assert!(value(&samples, "authentik_workers").is_none());
        assert!(value(&samples, "authentik_tasks_failed").is_none());
        assert_eq!(value(&samples, "authentik_scrape_errors"), Some(2.0));

        let client = AuthentikCollector::client(&target).unwrap();
        let error = first_route(&client, &WORKERS).await.unwrap_err();
        assert!(matches!(error, ProbeError::Protocol(ref m) if m.contains("web page")), "{error}");
        let error = first_route(&client, &TASKS).await.unwrap_err();
        assert!(
            matches!(error, ProbeError::Protocol(ref m) if m.contains("none of these routes")),
            "{error}"
        );
        assert!(!error.means_down());
    }

    #[tokio::test]
    async fn un_jeton_refuse_est_une_erreur_d_authentification_et_un_hote_eteint_une_panne() {
        let base = serve(app()).await;
        let error = AuthentikCollector::new().probe(&target(base, "faux")).await.unwrap_err();
        assert!(matches!(error, ProbeError::Auth(_)));
        assert!(!error.means_down());
        let error = AuthentikCollector::new()
            .probe(&target("http://127.0.0.1:1".into(), "cle"))
            .await
            .unwrap_err();
        assert!(error.means_down());
    }
}
