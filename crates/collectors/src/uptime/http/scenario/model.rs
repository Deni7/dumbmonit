//! Lecture du scénario : l'étiquette `steps`, un tableau JSON d'étapes.
//!
//! ```json
//! [
//!   { "name": "Log in", "method": "POST", "url": "/api/login",
//!     "headers": [{ "name": "Content-Type", "value": "application/json" }],
//!     "body": "{\"user\":\"{{secret.username}}\",\"password\":\"{{secret.password|json}}\"}",
//!     "expect_status": "200",
//!     "assertions": [{ "type": "json", "path": "$.ok", "value": "true" }],
//!     "extract": [{ "name": "token", "from": "json", "expression": "$.token" }] },
//!   { "name": "Dashboard", "url": "/api/me",
//!     "headers": [{ "name": "Authorization", "value": "Bearer {{token}}" }],
//!     "assertions": [{ "type": "contains", "value": "admin" }] }
//! ]
//! ```
//!
//! Tout est vérifié ici, avant le moindre appel réseau : une méthode inconnue,
//! une marque mal écrite, une variable utilisée avant d'avoir été extraite ou un
//! secret absent de l'identifiant sont des erreurs de configuration, pas des
//! pannes.

use std::collections::HashSet;

use dumbmonit_proto::ProbeError;
use regex::{Regex, RegexBuilder};
use reqwest::header::{HeaderName, HeaderValue};
use reqwest::{Method, Url};
use serde::Deserialize;

use super::template::{Secrets, Template, valid_name};
use crate::uptime::http::check::{Keyword, StatusRanges};
use crate::uptime::http::options::{JsonExpectation, parse_method};

/// Étapes au plus dans un scénario.
pub const MAX_STEPS: usize = 10;
/// En-têtes, vérifications et extractions au plus par étape.
const MAX_PER_STEP: usize = 20;
/// Taille maximale d'une expression régulière compilée : une saisie
/// pathologique ne doit pas réserver des mégaoctets à chaque sonde.
const REGEX_SIZE_LIMIT: usize = 256 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepSpec {
    #[serde(default)]
    name: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    headers: Vec<HeaderSpec>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    expect_status: String,
    #[serde(default)]
    assertions: Vec<AssertionSpec>,
    #[serde(default)]
    extract: Vec<ExtractionSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HeaderSpec {
    #[serde(default)]
    name: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssertionSpec {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtractionSpec {
    #[serde(default)]
    name: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    expression: String,
}

/// Vérification portant sur le corps d'une réponse.
#[derive(Debug, Clone)]
pub enum Assertion {
    /// Texte présent (ou absent, `Keyword::inverted`), sans tenir compte de la casse.
    Keyword(Keyword),
    /// Valeur attendue à un chemin du document JSON.
    Json(JsonExpectation),
}

/// D'où une extraction tire sa valeur.
#[derive(Debug, Clone)]
pub enum Source {
    /// Chemin dans le document JSON de la réponse (`$.token`).
    Json(String),
    /// En-tête de la réponse.
    Header(HeaderName),
    /// Expression régulière sur le corps : premier groupe, sinon la correspondance entière.
    Regex(Regex),
}

#[derive(Debug, Clone)]
pub struct Extraction {
    pub name: String,
    pub source: Source,
}

/// Une étape validée.
#[derive(Debug, Clone)]
pub struct Step {
    /// Rang à partir de 1 : c'est l'étiquette `step` des séries.
    pub index: usize,
    pub name: String,
    pub method: Method,
    /// Texte de l'URL tel que saisi, pour les messages : jamais de valeur substituée.
    pub url_text: String,
    pub url: Template,
    pub headers: Vec<(HeaderName, Template)>,
    pub body: Option<Template>,
    pub accepted_status: StatusRanges,
    pub assertions: Vec<Assertion>,
    pub extractions: Vec<Extraction>,
}

impl Step {
    /// Le nom saisi, ou « step N » : ce qui désigne l'étape dans les messages.
    pub fn title(&self) -> String {
        title_of(self.index, &self.name)
    }

    /// Vrai si une vérification ou une extraction lit le corps.
    pub fn needs_body(&self) -> bool {
        !self.assertions.is_empty()
            || self.extractions.iter().any(|e| !matches!(e.source, Source::Header(_)))
    }
}

/// Un scénario validé : de 1 à [`MAX_STEPS`] étapes.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub steps: Vec<Step>,
}

impl Scenario {
    /// Lit l'étiquette `steps`. `base` est l'adresse de la cible : une URL
    /// d'étape relative (`/api/login`) s'y rattache.
    pub fn parse(raw: &str, base: &Url) -> Result<Self, ProbeError> {
        let specs: Vec<StepSpec> = serde_json::from_str(raw)
            .map_err(|error| config(format!("\"steps\" is not a valid list of steps: {error}")))?;
        if specs.is_empty() {
            return Err(config("the scenario has no step: add at least one request"));
        }
        if specs.len() > MAX_STEPS {
            return Err(config(format!(
                "the scenario has {} steps, at most {MAX_STEPS} are allowed",
                specs.len()
            )));
        }
        let steps = specs
            .into_iter()
            .enumerate()
            .map(|(position, spec)| parse_step(position + 1, spec, base))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { steps })
    }

    /// Vérifie que chaque marque a une valeur au moment où l'étape s'exécute :
    /// une variable doit être extraite par une étape **précédente**, un secret
    /// doit figurer dans l'identifiant de la cible.
    pub fn check(&self, secrets: &Secrets<'_>) -> Result<(), ProbeError> {
        let mut known: HashSet<&str> = HashSet::new();
        for step in &self.steps {
            let templates = std::iter::once(&step.url)
                .chain(step.headers.iter().map(|(_, value)| value))
                .chain(step.body.iter());
            for template in templates {
                if let Some(name) = template.variables().find(|name| !known.contains(name)) {
                    return Err(config(format!(
                        "{} uses \"{{{{{name}}}}}\", which no earlier step extracts",
                        step.title()
                    )));
                }
                let missing = template.secrets().find(|field| secrets.get(*field).is_none());
                if let Some(field) = missing {
                    return Err(config(format!(
                        "{} uses \"{{{{secret.{}}}}}\", which the credential does not provide: \
                         choose a username / password pair or a token",
                        step.title(),
                        field.as_str()
                    )));
                }
            }
            known.extend(step.extractions.iter().map(|e| e.name.as_str()));
        }
        Ok(())
    }
}

/// « step 2 ("Login") », ou « step 2 » pour une étape sans nom.
fn title_of(index: usize, name: &str) -> String {
    if name.is_empty() {
        return format!("step {index}");
    }
    format!("step {index} (\"{name}\")")
}

fn config(message: impl Into<String>) -> ProbeError {
    ProbeError::Config(message.into())
}

/// Le texte d'une erreur de configuration, sans le préfixe de `ProbeError`.
fn message(error: ProbeError) -> String {
    match error {
        ProbeError::Config(detail) => detail,
        other => other.to_string(),
    }
}

fn parse_step(index: usize, spec: StepSpec, base: &Url) -> Result<Step, ProbeError> {
    let name = spec.name.trim().to_string();
    let title = title_of(index, &name);
    let fail = |detail: String| config(format!("{title}: {detail}"));

    let method = parse_method(Some(spec.method.trim()).filter(|m| !m.is_empty()))
        .map_err(|error| fail(message(error)))?;

    let url_text = spec.url.trim().to_string();
    if url_text.is_empty() {
        return Err(fail("the URL is empty".into()));
    }
    let url = Template::parse(&url_text).map_err(|detail| fail(format!("URL: {detail}")))?;
    if url.is_static() {
        // Vérifiée d'avance ; une URL qui dépend d'une variable l'est à l'exécution.
        resolve_url(base, &url_text).map_err(fail)?;
    }

    if spec.headers.len() > MAX_PER_STEP {
        return Err(fail(format!("at most {MAX_PER_STEP} headers")));
    }
    let mut headers = Vec::new();
    for header in spec.headers {
        let raw_name = header.name.trim();
        if raw_name.is_empty() && header.value.trim().is_empty() {
            continue;
        }
        let name = HeaderName::from_bytes(raw_name.as_bytes())
            .map_err(|_| fail(format!("invalid header name \"{raw_name}\"")))?;
        // La valeur n'est jamais reprise : un en-tête porte souvent un jeton.
        let value = Template::parse(header.value.trim())
            .map_err(|detail| fail(format!("header \"{name}\": {detail}")))?;
        if value.is_static() && HeaderValue::from_str(header.value.trim()).is_err() {
            return Err(fail(format!("invalid value for header \"{name}\"")));
        }
        headers.push((name, value));
    }

    let body = if spec.body.is_empty() {
        None
    } else {
        Some(Template::parse(&spec.body).map_err(|detail| fail(format!("body: {detail}")))?)
    };

    let accepted_status = match spec.expect_status.trim() {
        "" => StatusRanges::default(),
        raw => StatusRanges::parse(raw).map_err(|error| fail(message(error)))?,
    };

    if spec.assertions.len() > MAX_PER_STEP {
        return Err(fail(format!("at most {MAX_PER_STEP} checks")));
    }
    let assertions = spec
        .assertions
        .into_iter()
        .map(|assertion| parse_assertion(assertion).map_err(fail))
        .collect::<Result<Vec<_>, _>>()?;

    if spec.extract.len() > MAX_PER_STEP {
        return Err(fail(format!("at most {MAX_PER_STEP} extractions")));
    }
    let extractions = spec
        .extract
        .into_iter()
        .map(|extraction| parse_extraction(extraction).map_err(fail))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Step {
        index,
        name,
        method,
        url_text,
        url,
        headers,
        body,
        accepted_status,
        assertions,
        extractions,
    })
}

fn parse_assertion(spec: AssertionSpec) -> Result<Assertion, String> {
    let keyword = |inverted| {
        if spec.value.is_empty() {
            return Err("a text check needs the text to look for".to_string());
        }
        Ok(Assertion::Keyword(Keyword {
            needle: spec.value.clone(),
            inverted,
            case_sensitive: false,
        }))
    };
    match spec.kind.trim() {
        "contains" => keyword(false),
        "not_contains" => keyword(true),
        "json" => {
            let path = spec.path.trim();
            if path.is_empty() {
                return Err("a JSON check needs a path, for example \"$.status\"".into());
            }
            Ok(Assertion::Json(JsonExpectation {
                path: path.to_string(),
                expected: spec.value.trim().to_string(),
            }))
        }
        other => Err(format!(
            "unknown check type \"{other}\": use \"contains\", \"not_contains\" or \"json\""
        )),
    }
}

fn parse_extraction(spec: ExtractionSpec) -> Result<Extraction, String> {
    let name = spec.name.trim();
    if !valid_name(name) {
        return Err(format!(
            "\"{name}\" is not a valid variable name: letters, digits and \"_\", not starting \
             with a digit"
        ));
    }
    let expression = spec.expression.trim();
    if expression.is_empty() {
        return Err(format!("the extraction \"{name}\" has no expression"));
    }
    let source = match spec.from.trim() {
        "json" => Source::Json(expression.to_string()),
        "header" => Source::Header(
            HeaderName::from_bytes(expression.as_bytes())
                .map_err(|_| format!("invalid header name \"{expression}\""))?,
        ),
        "regex" => Source::Regex(
            RegexBuilder::new(expression)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|error| format!("invalid regular expression for \"{name}\": {error}"))?,
        ),
        other => {
            return Err(format!(
                "unknown extraction source \"{other}\": use \"json\", \"header\" or \"regex\""
            ));
        }
    };
    Ok(Extraction { name: name.to_string(), source })
}

/// Rattache l'URL d'une étape à l'adresse de la cible et la vérifie.
///
/// `Err` ne reprend pas l'URL rendue : elle peut contenir une valeur extraite.
pub fn resolve_url(base: &Url, text: &str) -> Result<Url, String> {
    let url = base.join(text.trim()).map_err(|error| format!("invalid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(format!(
            "scheme \"{}\" is not supported: a step expects \"http\" or \"https\"",
            url.scheme()
        ));
    }
    if url.host_str().is_none() {
        return Err("URL without a host".into());
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://app.example.com/").unwrap()
    }

    fn parse(raw: &str) -> Result<Scenario, ProbeError> {
        Scenario::parse(raw, &base())
    }

    #[test]
    fn une_etape_minimale_suffit() {
        let scenario = parse(r#"[{"url":"/health"}]"#).unwrap();
        let step = &scenario.steps[0];
        assert_eq!(step.index, 1);
        assert_eq!(step.method, Method::GET);
        assert!(step.accepted_status.accepts(204));
        assert!(!step.accepted_status.accepts(302));
        assert!(!step.needs_body());
        assert_eq!(step.title(), "step 1");
    }

    #[test]
    fn un_scenario_vide_ou_trop_long_est_refuse() {
        assert!(matches!(parse("[]").unwrap_err(), ProbeError::Config(_)));
        let onze: Vec<&str> = (0..=MAX_STEPS).map(|_| r#"{"url":"/"}"#).collect();
        let onze = format!("[{}]", onze.join(","));
        assert!(parse(&onze).unwrap_err().to_string().contains("at most"));
        assert!(parse("pas du json").is_err());
        assert!(parse(r#"[{"url":"/","inconnu":1}]"#).is_err(), "champ inconnu");
    }

    #[test]
    fn les_erreurs_nomment_letape_fautive() {
        let error = parse(r#"[{"url":"/"},{"name":"Login","method":"GET POST","url":"/"}]"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("step 2 (\"Login\")"), "{error}");

        for raw in [
            r#"[{"url":""}]"#,
            r#"[{"url":"ftp://x/"}]"#,
            r#"[{"url":"/","expect_status":"abc"}]"#,
            r#"[{"url":"/","headers":[{"name":"mauvais nom","value":"x"}]}]"#,
            r#"[{"url":"/","assertions":[{"type":"json","value":"ok"}]}]"#,
            r#"[{"url":"/","assertions":[{"type":"regex","value":"ok"}]}]"#,
            r#"[{"url":"/","extract":[{"name":"2x","from":"json","expression":"$.a"}]}]"#,
            r#"[{"url":"/","extract":[{"name":"x","from":"regex","expression":"("}]}]"#,
            r#"[{"url":"/","extract":[{"name":"x","from":"cookie","expression":"a"}]}]"#,
        ] {
            let error = parse(raw).expect_err(raw);
            assert!(matches!(error, ProbeError::Config(_)), "{raw}");
            assert!(!error.means_down(), "{raw}");
        }
    }

    #[test]
    fn une_variable_doit_etre_extraite_par_une_etape_precedente() {
        let ordre_juste = parse(
            r#"[{"url":"/login","extract":[{"name":"token","from":"json","expression":"$.t"}]},
                {"url":"/me","headers":[{"name":"Authorization","value":"Bearer {{token}}"}]}]"#,
        )
        .unwrap();
        assert!(ordre_juste.check(&Secrets::default()).is_ok());
        assert!(ordre_juste.steps[0].needs_body());

        let trop_tot = parse(
            r#"[{"url":"/me/{{token}}"},
                {"url":"/login","extract":[{"name":"token","from":"header","expression":"x-t"}]}]"#,
        )
        .unwrap();
        let error = trop_tot.check(&Secrets::default()).unwrap_err();
        assert!(matches!(error, ProbeError::Config(_)));
        assert!(error.to_string().contains("{{token}}"), "{error}");
        assert!(!trop_tot.steps[1].needs_body(), "un en-tête se lit sans le corps");
    }

    #[test]
    fn un_secret_absent_de_lidentifiant_est_une_erreur_de_configuration() {
        let scenario = parse(r#"[{"url":"/","body":"p={{secret.password|url}}"}]"#).unwrap();
        let error = scenario.check(&Secrets::default()).unwrap_err();
        assert!(error.to_string().contains("secret.password"), "{error}");
        let secrets = Secrets { username: Some("u"), password: Some("p"), token: None };
        assert!(scenario.check(&secrets).is_ok());
    }

    #[test]
    fn les_urls_relatives_se_rattachent_a_ladresse() {
        let base = Url::parse("https://app.example.com/root/").unwrap();
        assert_eq!(resolve_url(&base, "api").unwrap().as_str(), "https://app.example.com/root/api");
        assert_eq!(resolve_url(&base, "/api").unwrap().as_str(), "https://app.example.com/api");
        assert_eq!(
            resolve_url(&base, "http://autre.example.com/x").unwrap().as_str(),
            "http://autre.example.com/x"
        );
        assert!(resolve_url(&base, "mailto:x@y").is_err());
    }
}
