//! Scénario HTTP en plusieurs étapes (`kind = "http_steps"`).
//!
//! Là où la sonde HTTP vérifie une page, celle-ci rejoue un parcours : se
//! connecter, récupérer un jeton, l'utiliser pour lire une page protégée, puis
//! vérifier ce qu'elle contient. Chaque étape est une requête (méthode, URL,
//! en-têtes, corps) suivie de vérifications — statut, texte présent ou absent,
//! valeur JSON, reprises de la sonde HTTP — et d'extractions qui gardent une
//! valeur de la réponse (chemin JSON, en-tête, expression régulière) sous un nom
//! que les étapes suivantes citent en `{{nom}}`. Les cookies posés par une
//! réponse repartent avec les requêtes suivantes vers le même site.
//!
//! # Adresse et étiquettes
//!
//! Adresse : l'URL de base du service, `https://app.exemple.fr`. Une URL d'étape
//! relative (`/api/login`) s'y rattache ; une URL absolue peut viser un autre
//! site (un fournisseur d'identité, par exemple).
//!
//! | Étiquette | Défaut | Rôle |
//! |---|---|---|
//! | `steps` | — | Les étapes, tableau JSON (voir [`model`]). De 1 à 10. |
//! | `follow_redirects` | `true` | Suit les redirections de chaque étape. |
//! | `max_redirects` | `10` | Redirections suivies au plus, par étape. |
//! | `insecure_tls` | `false` | Accepte un certificat non vérifiable. |
//! | `allow_private_targets` | `false` | Autorise la boucle locale et le lien local. |
//! | `check_certificate` | `true` | Relève le certificat de l'adresse (HTTPS seulement). |
//! | `max_body_bytes` | `524288` | Corps lu au plus, par étape. |
//! | `user_agent` | `DumbMonit/…` | En-tête `User-Agent`. |
//! | `timeout_seconds` | `5` | Budget du scénario entier (1 à 60). |
//!
//! # Secrets
//!
//! L'identifiant chiffré de la cible fournit `{{secret.username}}`,
//! `{{secret.password}}` (nom d'utilisateur et mot de passe) ou
//! `{{secret.token}}` (jeton). Ils ne sont envoyés que là où une étape les
//! place : contrairement à la sonde HTTP, aucune authentification n'est ajoutée
//! d'office. Comme pour elle, les en-têtes saisis ne suivent pas une redirection
//! vers une autre origine, et un corps qui contient un secret n'y est jamais
//! renvoyé. Changer les étapes oblige à ressaisir l'identifiant (étiquette
//! `steps` dans les destinations de `api/targets.rs`).
//!
//! # Verdict
//!
//! La première étape en échec arrête le scénario. Une erreur de transport, un
//! statut refusé, une vérification ou une extraction manquée donnent
//! `probe_success = 0` avec la raison habituelle (`extract` pour une
//! extraction). Une variable utilisée avant d'être extraite, un secret absent
//! de l'identifiant ou une étape mal écrite sont des erreurs de configuration :
//! rien n'est écrit, aucune alerte de panne ne part.
//!
//! # Métriques
//!
//! En plus des métriques communes et de celles du certificat, une série par
//! étape, étiquette `step` (rang à partir de 1) :
//!
//! | Métrique | Sens |
//! |---|---|
//! | `probe_step_duration_seconds` | Durée de l'étape, redirections comprises. |
//! | `probe_step_status_code` | Statut de la dernière réponse de l'étape. |
//! | `probe_step_result` | 1 réussie, 0 en échec, 2 non jouée (une étape précédente a échoué). |

mod cookies;
mod model;
mod template;

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dumbmonit_proto::{Collector, Credential, ProbeError, Sample, Target};
use reqwest::header::{COOKIE, HeaderValue, USER_AGENT};
use reqwest::{Client, Method, StatusCode, Url};
use serde_json::Value;
use tracing::debug;

use super::check;
use super::options::{DEFAULT_MAX_BODY_BYTES, DEFAULT_MAX_REDIRECTS, parse_url};
use super::{HttpCollector, classify, read_body, redirect_target, same_origin};
use crate::uptime::guard;
use crate::uptime::outcome::{Failure, Report};
use crate::uptime::tags;
use cookies::CookieJar;
use model::{Assertion, Scenario, Source, Step, resolve_url};
use template::Secrets;

/// Valeurs de `probe_step_result`.
const STEP_PASSED: f64 = 1.0;
const STEP_FAILED: f64 = 0.0;
const STEP_SKIPPED: f64 = 2.0;

/// Part du budget laissée au relevé de certificat, comme pour la sonde HTTP.
const CERTIFICATE_BUDGET_RATIO: u32 = 2;

/// Temps minimal laissé aux étapes, quoi qu'aient coûté les vérifications.
const MIN_STEPS_BUDGET: Duration = Duration::from_secs(1);

/// Réglages du scénario, lus dans les étiquettes de la cible.
#[derive(Debug, Clone)]
struct Options {
    base: Url,
    scenario: Scenario,
    max_redirects: u32,
    insecure_tls: bool,
    check_certificate: bool,
    max_body_bytes: usize,
    user_agent: String,
    timeout: Duration,
}

impl Options {
    fn from_target(target: &Target) -> Result<Self, ProbeError> {
        let base = parse_url(&target.address)?;
        let raw = tags::tag(target, "steps").ok_or_else(|| {
            ProbeError::Config("the scenario has no step: add at least one request".to_string())
        })?;
        let scenario = Scenario::parse(raw, &base)?;
        let follow = tags::parse_bool(target, "follow_redirects", true)?;
        let max_redirects = if follow {
            tags::parse_u32(target, "max_redirects", DEFAULT_MAX_REDIRECTS, 1..=20)?
        } else {
            0
        };
        let secure = base.scheme() == "https";
        Ok(Self {
            scenario,
            max_redirects,
            insecure_tls: tags::parse_bool(target, "insecure_tls", false)?,
            check_certificate: secure && tags::parse_bool(target, "check_certificate", true)?,
            max_body_bytes: tags::parse_u32(
                target,
                "max_body_bytes",
                DEFAULT_MAX_BODY_BYTES,
                0..=16 * 1024 * 1024,
            )? as usize,
            user_agent: tags::tag(target, "user_agent")
                .map(str::to_string)
                .unwrap_or_else(|| concat!("DumbMonit/", env!("CARGO_PKG_VERSION")).to_string()),
            timeout: tags::parse_timeout(target, "timeout_seconds")?,
            base,
        })
    }

    /// Hôte et port pour le relevé du certificat de l'adresse.
    fn tls_endpoint(&self) -> Option<(String, u16)> {
        if !self.check_certificate {
            return None;
        }
        let host = self.base.host_str()?.to_string();
        Some((host, self.base.port_or_known_default().unwrap_or(443)))
    }

    /// Étiquette de série : l'adresse sans identifiants ni fragment.
    fn url_label(&self) -> String {
        let mut url = self.base.clone();
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_fragment(None);
        url.to_string()
    }
}

/// Les parts de l'identifiant que les étapes peuvent citer.
fn secrets_of(credential: &Credential) -> Result<Secrets<'_>, ProbeError> {
    Ok(match credential {
        Credential::None => Secrets::default(),
        Credential::UsernamePassword { username, password } => {
            Secrets { username: Some(username), password: Some(password), token: None }
        }
        Credential::ApiToken { token } => Secrets { token: Some(token), ..Secrets::default() },
        // Le libellé du type est repris, jamais le secret lui-même.
        other => {
            return Err(ProbeError::Config(format!(
                "an HTTP scenario accepts an API token or a username / password pair, \
                 configured credential: {}",
                other.kind_label()
            )));
        }
    })
}

/// Collecteur des scénarios HTTP.
#[derive(Default)]
pub struct HttpStepsCollector;

impl HttpStepsCollector {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Collector for HttpStepsCollector {
    fn kind(&self) -> &'static str {
        "http_steps"
    }

    async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        let begun = Instant::now();
        let options = Options::from_target(target)?;
        let secrets = secrets_of(&target.credential)?;
        options.scenario.check(&secrets)?;
        let allow_private = guard::allowed(target)?;
        // Le client de la sonde HTTP : mêmes réglages TLS, même garde-fou dans le
        // résolveur, pas de redirection automatique (elles passent par le garde).
        let (client, _clock) = HttpCollector::new().client(options.insecure_tls, allow_private)?;

        // Avant toute connexion : l'adresse et chaque URL d'étape connue d'avance.
        // Un refus est une erreur de configuration, pas une mesure.
        let mut vetted = HashSet::new();
        let fixed = options
            .scenario
            .steps
            .iter()
            .filter(|step| step.url.is_static())
            .map(|step| resolve_url(&options.base, &step.url_text).map_err(ProbeError::Config));
        for url in std::iter::once(Ok(options.base.clone())).chain(fixed) {
            let url = url?;
            let origin = (url.host_str().map(str::to_string), url.port_or_known_default());
            if vetted.insert(origin) {
                let remaining = options.timeout.saturating_sub(begun.elapsed());
                guard::vet_url(&url, allow_private, remaining.max(MIN_STEPS_BUDGET)).await?;
            }
        }

        let mut report = Report::new(self.kind()).label("url", options.url_label());
        if let Some((host, port)) = options.tls_endpoint() {
            crate::uptime::tls::measure(
                &mut report,
                &host,
                port,
                &host,
                options.insecure_tls,
                allow_private,
                options.timeout / CERTIFICATE_BUDGET_RATIO,
            )
            .await?;
        }

        let budget = options.timeout.saturating_sub(begun.elapsed()).max(MIN_STEPS_BUDGET);
        let session = Session { client, options: &options, secrets, allow_private };
        run_scenario(&mut report, &session, budget).await;

        if let Some(detail) = report.detail() {
            debug!(target_id = target.id, url = %options.url_label(), detail,
                "scénario HTTP en échec");
        }
        Ok(report.finish())
    }
}

/// Ce qui ne change pas d'une étape à l'autre.
struct Session<'a> {
    client: Client,
    options: &'a Options,
    secrets: Secrets<'a>,
    allow_private: bool,
}

/// Issue d'une étape : le statut obtenu, et le premier échec constaté.
#[derive(Debug, Default)]
struct StepOutcome {
    status: Option<u16>,
    failure: Option<(Failure, String)>,
}

impl StepOutcome {
    fn failed(mut self, reason: Failure, detail: impl Into<String>) -> Self {
        self.failure = Some((reason, detail.into()));
        self
    }
}

/// Joue les étapes dans l'ordre, jusqu'à la première en échec.
///
/// Les variables et les cookies vivent le temps d'un passage : rien n'est
/// gardé d'une sonde à la suivante.
async fn run_scenario(report: &mut Report, session: &Session<'_>, budget: Duration) {
    let started = Instant::now();
    let mut variables = HashMap::new();
    let mut jar = CookieJar::default();
    let mut stopped = false;

    for step in &session.options.scenario.steps {
        let label = step.index.to_string();
        if stopped {
            report.gauge_with("step_result", STEP_SKIPPED, "step", &label);
            continue;
        }
        let remaining = budget.saturating_sub(started.elapsed());
        let step_started = Instant::now();
        let outcome = if remaining.is_zero() {
            StepOutcome::default()
                .failed(Failure::Timeout, "the scenario ran out of time before this step")
        } else {
            run_step(session, step, &mut variables, &mut jar, remaining).await
        };
        let elapsed = step_started.elapsed().as_secs_f64();
        report.gauge_with("step_duration_seconds", elapsed, "step", &label);
        if let Some(code) = outcome.status {
            report.gauge_with("step_status_code", f64::from(code), "step", &label);
        }
        match outcome.failure {
            None => report.gauge_with("step_result", STEP_PASSED, "step", &label),
            Some((reason, detail)) => {
                report.gauge_with("step_result", STEP_FAILED, "step", &label);
                report.fail(reason, format!("{}: {detail}", step.title()));
                stopped = true;
            }
        }
    }
}

/// Texte d'un refus du garde-fou, sans le préfixe de `ProbeError`.
fn refusal_detail(error: ProbeError) -> String {
    match error {
        ProbeError::Config(detail) => detail,
        other => other.to_string(),
    }
}

/// Une valeur JSON extraite, telle qu'on l'insère dans un texte : une chaîne
/// sans ses guillemets, tout le reste sous sa forme JSON.
fn json_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Joue une étape : rendu des gabarits, requête (redirections suivies à la
/// main, comme la sonde HTTP), vérifications puis extractions.
///
/// Les messages ne reprennent jamais une valeur substituée — ni secret, ni
/// valeur extraite — mais le texte saisi.
async fn run_step(
    session: &Session<'_>,
    step: &Step,
    variables: &mut HashMap<String, String>,
    jar: &mut CookieJar,
    budget: Duration,
) -> StepOutcome {
    let outcome = StepOutcome::default();
    let options = session.options;
    let secrets = &session.secrets;

    let rendered = step.url.render(variables, secrets);
    let url = match rendered.and_then(|url| resolve_url(&options.base, &url.text)) {
        Ok(url) => url,
        Err(detail) => {
            let detail = format!("URL \"{}\": {detail}", step.url_text);
            return outcome.failed(Failure::Extract, detail);
        }
    };
    if !step.url.is_static()
        && let Err(error) = guard::vet_url(&url, session.allow_private, budget).await
    {
        let detail = format!("URL refused: {}", refusal_detail(error));
        return outcome.failed(Failure::Connect, detail);
    }

    let mut headers = Vec::with_capacity(step.headers.len());
    for (name, template) in &step.headers {
        let rendered = match template.render(variables, secrets) {
            Ok(rendered) => rendered,
            Err(detail) => {
                return outcome.failed(Failure::Extract, format!("header \"{name}\": {detail}"));
            }
        };
        let Ok(mut value) = HeaderValue::from_str(&rendered.text) else {
            return outcome.failed(
                Failure::Extract,
                format!("header \"{name}\" is not a valid header value once filled in"),
            );
        };
        value.set_sensitive(rendered.uses_secret);
        headers.push((name.clone(), value));
    }
    let body = match &step.body {
        None => None,
        Some(template) => match template.render(variables, secrets) {
            Ok(rendered) => Some(rendered),
            Err(detail) => return outcome.failed(Failure::Extract, format!("body: {detail}")),
        },
    };

    let started = Instant::now();
    let mut current = url.clone();
    let mut method = step.method.clone();
    let mut with_body = true;
    let mut redirects = 0u32;
    let mut response = loop {
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return outcome.failed(Failure::Timeout, "redirects exhausted the time budget");
        }
        // Les en-têtes saisis ne quittent pas l'origine de l'étape, comme
        // l'identifiant de la sonde HTTP.
        let same_origin = same_origin(&current, &url);
        let mut builder = session
            .client
            .request(method.clone(), current.clone())
            .timeout(remaining)
            .header(USER_AGENT, &options.user_agent);
        if same_origin {
            for (name, value) in &headers {
                builder = builder.header(name, value);
            }
        }
        if with_body && let Some(body) = &body {
            if !same_origin && body.uses_secret {
                return outcome.failed(
                    Failure::Status,
                    "redirect to another origin refused: the request body holds a secret",
                );
            }
            builder = builder.body(body.text.clone());
        }
        if let Some(cookies) = jar.header_for(&current) {
            builder = builder.header(COOKIE, cookies);
        }

        let response = match builder.send().await {
            Ok(response) => response,
            Err(error) => {
                // Sans l'URL : elle peut porter une valeur extraite (un jeton).
                let (reason, detail) = classify(&error.without_url());
                return outcome.failed(reason, detail);
            }
        };
        jar.store(&current, response.headers());

        let Some(next) = redirect_target(&response, &current) else { break response };
        if options.max_redirects == 0 {
            break response;
        }
        let next = match next {
            Ok(next) => next,
            Err(detail) => return outcome.failed(Failure::Status, detail),
        };
        redirects += 1;
        if redirects > options.max_redirects {
            let detail = format!("more than {} redirects", options.max_redirects);
            return outcome.failed(Failure::Status, detail);
        }
        if let Err(error) = guard::vet_url(&next, session.allow_private, remaining).await {
            let detail = format!("redirect refused: {}", refusal_detail(error));
            return outcome.failed(Failure::Connect, detail);
        }
        if matches!(
            response.status(),
            StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER
        ) {
            method = Method::GET;
            with_body = false;
        }
        current = next;
    };

    let status = response.status().as_u16();
    let outcome = StepOutcome { status: Some(status), ..outcome };
    if !step.accepted_status.accepts(status) {
        let accepted = step.accepted_status.describe();
        let detail = format!("status {status} is outside the accepted codes ({accepted})");
        return outcome.failed(Failure::Status, detail);
    }
    if !step.needs_body() && step.extractions.is_empty() {
        return outcome;
    }

    let response_headers = response.headers().clone();
    let (text, truncated) = if step.needs_body() {
        match read_body(&mut response, options.max_body_bytes).await {
            Ok((body, truncated)) => (String::from_utf8_lossy(&body).into_owned(), truncated),
            Err(error) => {
                let detail = format!("unreadable body: {}", error.without_url());
                return outcome.failed(Failure::Body, detail);
            }
        }
    } else {
        (String::new(), false)
    };
    let note = if truncated {
        format!(" (body truncated at {} bytes)", options.max_body_bytes)
    } else {
        String::new()
    };

    let needs_json = step.assertions.iter().any(|a| matches!(a, Assertion::Json(_)))
        || step.extractions.iter().any(|e| matches!(e.source, Source::Json(_)));
    let document = if needs_json {
        match serde_json::from_str::<Value>(&text) {
            Ok(document) => Some(document),
            Err(error) => {
                let detail = format!("invalid JSON response: {error}{note}");
                return outcome.failed(Failure::Body, detail);
            }
        }
    } else {
        None
    };

    for assertion in &step.assertions {
        match assertion {
            Assertion::Keyword(keyword) => {
                if let Some(detail) = keyword.verify(&text) {
                    return outcome.failed(Failure::Keyword, format!("{detail}{note}"));
                }
            }
            Assertion::Json(expectation) => {
                let Some(document) = &document else { continue };
                match check::json_at(document, &expectation.path) {
                    Err(detail) => return outcome.failed(Failure::Json, detail),
                    Ok(value) if !check::value_matches(value, &expectation.expected) => {
                        let detail = format!(
                            "\"{}\" is \"{}\", expected \"{}\"",
                            expectation.path,
                            check::value_label(value),
                            expectation.expected
                        );
                        return outcome.failed(Failure::Json, detail);
                    }
                    Ok(_) => {}
                }
            }
        }
    }

    for extraction in &step.extractions {
        let found = match &extraction.source {
            Source::Header(name) => response_headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .ok_or_else(|| format!("header \"{name}\" missing from the response")),
            Source::Json(path) => match &document {
                Some(document) => check::json_at(document, path).map(json_text),
                None => Err("the response is not JSON".to_string()),
            },
            Source::Regex(regex) => regex
                .captures(&text)
                .and_then(|captures| captures.get(1).or_else(|| captures.get(0)))
                .map(|found| found.as_str().to_string())
                .ok_or_else(|| format!("the pattern matched nothing{note}")),
        };
        match found {
            Ok(value) => {
                variables.insert(extraction.name.clone(), value);
            }
            Err(detail) => {
                let detail = format!("cannot extract \"{}\": {detail}", extraction.name);
                return outcome.failed(Failure::Extract, detail);
            }
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::Router;
    use axum::extract::State;
    use axum::http::header::{AUTHORIZATION, SET_COOKIE};
    use axum::http::{HeaderMap, StatusCode as Code};
    use axum::routing::{get, post};

    use super::*;
    use crate::uptime::tags::test_support::cible;

    /// Ce que le faux service a reçu : en-têtes `Cookie` et corps de connexion.
    #[derive(Default)]
    struct Vu {
        cookies: Vec<Option<String>>,
        login_bodies: Vec<String>,
    }
    type Etat = Arc<Mutex<Vu>>;

    /// Faux service sur la boucle locale : une connexion qui pose un cookie et
    /// rend un jeton, une page HTML qui exige les deux, une page simple et une
    /// page en panne.
    async fn faux_service(vu: Etat) -> String {
        async fn login(State(vu): State<Etat>, body: String) -> impl axum::response::IntoResponse {
            let ok = body.contains("\"password\":\"s3cr\\\"et\"");
            vu.lock().unwrap().login_bodies.push(body);
            if !ok {
                return (Code::UNAUTHORIZED, [(SET_COOKIE, "x=1")], String::new());
            }
            (
                Code::OK,
                [(SET_COOKIE, "session=abc123; Path=/; HttpOnly")],
                r#"{"ok":true,"token":"tok-42","user":{"id":7}}"#.to_string(),
            )
        }
        async fn me(State(vu): State<Etat>, headers: HeaderMap) -> (Code, HeaderMap, String) {
            let cookie = headers.get("cookie").map(|v| v.to_str().unwrap().to_string());
            vu.lock().unwrap().cookies.push(cookie.clone());
            let bearer = headers.get(AUTHORIZATION).map(|v| v.to_str().unwrap().to_string());
            if cookie.as_deref() != Some("session=abc123")
                || bearer.as_deref() != Some("Bearer tok-42")
            {
                return (Code::FORBIDDEN, HeaderMap::new(), String::new());
            }
            let mut out = HeaderMap::new();
            out.insert("x-request-id", "req-9".parse().unwrap());
            (Code::OK, out, "<p>Bonjour admin, commande n° 1234</p>".to_string())
        }

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let router = Router::new()
            .route("/login", post(login))
            .route("/me", get(me))
            .route("/items/{id}", get(|| async { "item" }))
            .route("/panne", get(|| async { (Code::INTERNAL_SERVER_ERROR, "oops") }))
            .with_state(vu);
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        format!("http://127.0.0.1:{port}")
    }

    fn scenario(base: &str, steps: &str, password: Option<&str>) -> Target {
        let mut target = cible(
            "http_steps",
            base,
            &[("steps", steps), ("allow_private_targets", "true"), ("timeout_seconds", "5")],
        );
        if let Some(password) = password {
            target.credential = Credential::UsernamePassword {
                username: "admin".into(),
                password: password.into(),
            };
        }
        target
    }

    fn valeur(samples: &[Sample], metric: &str, step: Option<&str>) -> Option<f64> {
        samples
            .iter()
            .find(|s| s.metric == metric && s.labels.get("step").map(String::as_str) == step)
            .map(|s| s.value)
    }

    fn raison(samples: &[Sample]) -> Option<String> {
        samples
            .iter()
            .find(|s| s.metric == "probe_failure_info")
            .and_then(|s| s.labels.get("reason").cloned())
    }

    const CONNEXION: &str = r#"{"name":"Login","method":"POST","url":"/login",
        "headers":[{"name":"Content-Type","value":"application/json"}],
        "body":"{\"user\":\"{{secret.username}}\",\"password\":\"{{secret.password|json}}\"}",
        "assertions":[{"type":"json","path":"$.ok","value":"true"}],
        "extract":[{"name":"token","from":"json","expression":"$.token"},
                   {"name":"uid","from":"json","expression":"$.user.id"}]}"#;

    #[test]
    fn le_collecteur_annonce_son_type() {
        assert_eq!(HttpStepsCollector::new().kind(), "http_steps");
    }

    #[tokio::test]
    async fn un_parcours_complet_reussit_avec_variables_et_cookies() {
        let vu = Etat::default();
        let base = faux_service(vu.clone()).await;
        let steps = format!(
            r#"[{CONNEXION},
               {{"name":"Me","url":"/me",
                 "headers":[{{"name":"Authorization","value":"Bearer {{{{token}}}}"}}],
                 "assertions":[{{"type":"contains","value":"bonjour ADMIN"}},
                               {{"type":"not_contains","value":"Exception"}}],
                 "extract":[{{"name":"rid","from":"header","expression":"X-Request-Id"}},
                            {{"name":"order","from":"regex","expression":"n° (\\d+)"}}]}},
               {{"url":"/items/{{{{uid}}}}-{{{{rid}}}}-{{{{order}}}}"}}]"#
        );
        let target = scenario(&base, &steps, Some("s3cr\"et"));
        let samples = HttpStepsCollector::new().probe(&target).await.expect("une mesure");

        assert_eq!(valeur(&samples, "probe_success", None), Some(1.0), "{samples:?}");
        for step in ["1", "2", "3"] {
            assert_eq!(valeur(&samples, "probe_step_result", Some(step)), Some(1.0), "{step}");
            assert_eq!(valeur(&samples, "probe_step_status_code", Some(step)), Some(200.0));
            assert!(valeur(&samples, "probe_step_duration_seconds", Some(step)).unwrap() >= 0.0);
        }
        let vu = vu.lock().unwrap();
        assert_eq!(vu.cookies, vec![Some("session=abc123".to_string())], "le cookie a suivi");
        // Le secret est échappé pour le JSON par le filtre `json`.
        assert!(vu.login_bodies[0].contains(r#""user":"admin""#), "{:?}", vu.login_bodies);
        // Aucune valeur substituée ne finit en étiquette.
        for sample in &samples {
            for value in sample.labels.values() {
                assert!(!value.contains("tok-42") && !value.contains("s3cr"), "{sample:?}");
            }
        }
    }

    #[tokio::test]
    async fn les_cookies_ne_survivent_pas_dun_passage_a_lautre() {
        let vu = Etat::default();
        let base = faux_service(vu.clone()).await;
        let steps = r#"[{"url":"/me","expect_status":"403"}]"#;
        let target = scenario(&base, steps, None);
        let collector = HttpStepsCollector::new();
        for _ in 0..2 {
            let samples = collector.probe(&target).await.unwrap();
            assert_eq!(valeur(&samples, "probe_success", None), Some(1.0));
        }
        assert_eq!(vu.lock().unwrap().cookies, vec![None, None]);
    }

    #[tokio::test]
    async fn une_etape_en_echec_arrete_le_scenario_et_marque_les_suivantes() {
        let base = faux_service(Etat::default()).await;
        let steps = r#"[{"url":"/items/1"},{"name":"Broken","url":"/panne"},{"url":"/items/2"}]"#;
        let samples = HttpStepsCollector::new().probe(&scenario(&base, steps, None)).await.unwrap();
        assert_eq!(valeur(&samples, "probe_success", None), Some(0.0));
        assert_eq!(raison(&samples).as_deref(), Some("status"));
        assert_eq!(valeur(&samples, "probe_step_result", Some("1")), Some(STEP_PASSED));
        assert_eq!(valeur(&samples, "probe_step_result", Some("2")), Some(STEP_FAILED));
        assert_eq!(valeur(&samples, "probe_step_status_code", Some("2")), Some(500.0));
        assert_eq!(valeur(&samples, "probe_step_result", Some("3")), Some(STEP_SKIPPED));
        assert_eq!(valeur(&samples, "probe_step_duration_seconds", Some("3")), None);
    }

    #[tokio::test]
    async fn un_mauvais_mot_de_passe_est_une_panne_de_statut() {
        let base = faux_service(Etat::default()).await;
        let steps = format!("[{CONNEXION}]");
        let samples =
            HttpStepsCollector::new().probe(&scenario(&base, &steps, Some("faux"))).await.unwrap();
        assert_eq!(valeur(&samples, "probe_success", None), Some(0.0));
        assert_eq!(raison(&samples).as_deref(), Some("status"));
    }

    #[tokio::test]
    async fn une_extraction_manquee_est_une_panne_dextraction() {
        let base = faux_service(Etat::default()).await;
        for extract in [
            r#"{"name":"x","from":"header","expression":"x-absent"}"#,
            r#"{"name":"x","from":"regex","expression":"introuvable (\\d+)"}"#,
        ] {
            let steps = format!(r#"[{{"url":"/items/1","extract":[{extract}]}}]"#);
            let samples =
                HttpStepsCollector::new().probe(&scenario(&base, &steps, None)).await.unwrap();
            assert_eq!(valeur(&samples, "probe_success", None), Some(0.0), "{extract}");
            assert_eq!(raison(&samples).as_deref(), Some("extract"), "{extract}");
        }
        // Un chemin JSON sur une réponse qui n'est pas du JSON : réponse illisible.
        let extract = r#"{"name":"x","from":"json","expression":"$.a"}"#;
        let steps = format!(r#"[{{"url":"/items/1","extract":[{extract}]}}]"#);
        let samples =
            HttpStepsCollector::new().probe(&scenario(&base, &steps, None)).await.unwrap();
        assert_eq!(raison(&samples).as_deref(), Some("body"));
    }

    #[tokio::test]
    async fn un_port_ferme_est_une_panne_et_non_une_erreur() {
        let ecoute = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = ecoute.local_addr().unwrap().port();
        drop(ecoute);
        let target = scenario(&format!("http://127.0.0.1:{port}"), r#"[{"url":"/"}]"#, None);
        let samples = HttpStepsCollector::new().probe(&target).await.expect("une mesure");
        assert_eq!(valeur(&samples, "probe_success", None), Some(0.0));
        assert_eq!(raison(&samples).as_deref(), Some("connect"));
        assert_eq!(valeur(&samples, "probe_step_result", Some("1")), Some(STEP_FAILED));
    }

    #[tokio::test]
    async fn une_erreur_de_redaction_nest_pas_une_panne() {
        let base = faux_service(Etat::default()).await;
        for (steps, password) in [
            // Variable jamais extraite.
            (r#"[{"url":"/items/{{inconnue}}"}]"#, None),
            // Secret absent de l'identifiant.
            (r#"[{"url":"/login","body":"{{secret.password}}"}]"#, None),
            // Étiquette illisible.
            ("pas du json", Some("x")),
        ] {
            let error = HttpStepsCollector::new()
                .probe(&scenario(&base, steps, password))
                .await
                .expect_err(steps);
            assert!(matches!(error, ProbeError::Config(_)), "{steps} : {error}");
            assert!(!error.means_down(), "{steps}");
        }
    }

    #[tokio::test]
    async fn la_boucle_locale_reste_refusee_sans_loption() {
        let base = faux_service(Etat::default()).await;
        let target = cible("http_steps", &base, &[("steps", r#"[{"url":"/items/1"}]"#)]);
        let error = HttpStepsCollector::new().probe(&target).await.unwrap_err();
        assert!(error.to_string().contains("allow_private_targets"), "{error}");
        assert!(!error.means_down());
    }

    #[test]
    fn un_identifiant_inadapte_est_refuse() {
        let credential = Credential::SnmpCommunity { community: "public".into() };
        let error = secrets_of(&credential).unwrap_err();
        assert!(!error.to_string().contains("public"), "le secret n'est pas repris");
        let token = Credential::ApiToken { token: "t".into() };
        assert_eq!(secrets_of(&token).unwrap().token, Some("t"));
    }
}
