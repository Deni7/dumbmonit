//! Le collecteur d'un paquet : type `pack.<id>`, description tirée du manifeste.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use dumbmonit_collectors::api_options::Connection;
use dumbmonit_collectors::uptime::guard;
use dumbmonit_proto::{Collector, Credential, KindDescription, ProbeError, Sample, Target};
use reqwest::Url;

use crate::manifest::Scheme;
use crate::template::Context;
use crate::{MAX_REQUESTS, MAX_SERIES, Pack, http};

/// Délai par requête quand la cible ne le règle pas.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Requêtes réservées à la fermeture de session (une, plus une redirection),
/// hors du budget des sources : la session se referme même si elles l'ont épuisé.
const LOGOUT_REQUESTS: usize = 2;

/// Ce qu'une source facultative peut rencontrer sans faire échouer
/// l'interrogation : un refus, une erreur HTTP, un corps illisible. Une panne
/// (injoignable, délai) ou une erreur de configuration reste une erreur.
fn skippable(error: &ProbeError) -> bool {
    matches!(error, ProbeError::Auth(_) | ProbeError::Protocol(_))
}

pub struct PackCollector {
    pack: Arc<Pack>,
}

impl PackCollector {
    pub fn new(pack: Arc<Pack>) -> Self {
        Self { pack }
    }

    pub fn pack(&self) -> &Arc<Pack> {
        &self.pack
    }
}

#[async_trait]
impl Collector for PackCollector {
    fn kind(&self) -> &str {
        self.pack.kind()
    }

    fn description(&self) -> Option<KindDescription> {
        Some(self.pack.description())
    }

    async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        self.pack.probe(target).await
    }
}

impl Pack {
    /// Valeurs des options pour une cible : son étiquette, sinon le défaut déclaré.
    fn resolve_options(&self, target: &Target) -> Result<BTreeMap<String, String>, ProbeError> {
        let mut values = BTreeMap::new();
        for option in &self.manifest.options {
            let value = target
                .tags
                .get(&option.key)
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
                .unwrap_or(option.default.0.as_str());
            if option.required && value.is_empty() {
                return Err(ProbeError::Config(format!(
                    "The option \"{}\" ({}) is required",
                    option.key, option.label
                )));
            }
            values.insert(option.key.clone(), value.to_string());
        }
        Ok(values)
    }

    fn check_credential(&self, credential: &Credential) -> Result<(), ProbeError> {
        let kind = match credential {
            Credential::None => "none",
            Credential::ApiToken { .. } => "api_token",
            Credential::UsernamePassword { .. } => "username_password",
            _ => "other",
        };
        let accepted = if self.manifest.credentials.is_empty() {
            kind == "none"
        } else {
            self.manifest.credentials.iter().any(|spec| spec.kind() == kind)
        };
        if accepted {
            Ok(())
        } else {
            Err(ProbeError::Config(format!(
                "This monitor type does not accept a credential of type \"{kind}\""
            )))
        }
    }

    /// Interroge une cible : chaque source, dans l'ordre, puis l'extraction.
    pub async fn probe(&self, target: &Target) -> Result<Vec<Sample>, ProbeError> {
        self.check_credential(&target.credential)?;
        let options = self.resolve_options(target)?;
        let (http_port, https_port) = match (self.manifest.scheme, self.manifest.default_port) {
            (_, 0) => (80, 443),
            (Scheme::Http, port) => (port, 443),
            (Scheme::Https, port) => (80, port),
        };
        let connection = Connection::from_target(
            target,
            self.manifest.scheme.as_str(),
            http_port,
            https_port,
            DEFAULT_REQUEST_TIMEOUT,
        )?;
        let base = Url::parse(&connection.base_url).map_err(|error| {
            ProbeError::Config(format!("invalid address \"{}\": {error}", target.address))
        })?;
        if !matches!(base.scheme(), "http" | "https") || base.host_str().is_none() {
            return Err(ProbeError::Config(format!(
                "invalid address \"{}\": an http(s) host is expected",
                target.address
            )));
        }
        let allow_private = guard::allowed(target)?;
        let client = http::client(connection.insecure_tls, allow_private)?;
        let context = Context { options: &options, credential: &target.credential };
        let request = http::Request {
            client: &client,
            base: &base,
            context: &context,
            allow_private,
            timeout: connection.request_timeout,
        };

        let mut budget = MAX_REQUESTS;
        if let Some(login) = &self.session_login {
            http::fetch(&request, login, &mut budget).await?;
        }
        let result = self.read_sources(&request, &mut budget).await;
        // Fermée même après un échec : un équipement qui n'admet qu'une session
        // d'API à la fois resterait sinon fermé jusqu'à son délai d'inactivité.
        if let Some(logout) = &self.session_logout {
            let mut spare = LOGOUT_REQUESTS;
            if let Err(error) = http::fetch(&request, logout, &mut spare).await {
                tracing::debug!(pack = self.id(), %error, "fermeture de session impossible");
            }
        }
        let mut samples = result?;
        if samples.len() > MAX_SERIES {
            tracing::warn!(
                pack = self.id(),
                target_id = target.id,
                series = samples.len(),
                "trop de séries pour un paquet : les suivantes sont abandonnées"
            );
            samples.truncate(MAX_SERIES);
        }
        Ok(samples)
    }

    /// Chaque source, dans l'ordre ; une source facultative en échec est passée.
    async fn read_sources(
        &self,
        request: &http::Request<'_>,
        budget: &mut usize,
    ) -> Result<Vec<Sample>, ProbeError> {
        let ts_ms = chrono::Utc::now().timestamp_millis();
        let mut samples = Vec::new();
        for source in &self.sources {
            let read = match http::fetch(request, source, budget).await {
                Ok(bytes) => source.decode(&bytes).map_err(ProbeError::Protocol),
                Err(error) => Err(error),
            };
            let body = match read {
                Ok(body) => body,
                Err(error) if source.optional && skippable(&error) => {
                    tracing::debug!(
                        pack = self.id(),
                        source = %source.id,
                        %error,
                        "source facultative passée"
                    );
                    continue;
                }
                Err(error) => return Err(error),
            };
            source.extract(&body, ts_ms, &mut samples);
        }
        Ok(samples)
    }

    /// Extraction seule, à partir de réponses déjà lues (fixtures, tests).
    ///
    /// Une source absente de `bodies` ne produit rien.
    pub fn extract(
        &self,
        bodies: &BTreeMap<String, Vec<u8>>,
        ts_ms: i64,
    ) -> Result<Vec<Sample>, String> {
        let mut samples = Vec::new();
        for source in &self.sources {
            let Some(bytes) = bodies.get(&source.id) else { continue };
            let body = source.decode(bytes)?;
            source.extract(&body, ts_ms, &mut samples);
        }
        samples.truncate(MAX_SERIES);
        Ok(samples)
    }

    /// Identifiants des sources, dans l'ordre de déclaration.
    pub fn source_ids(&self) -> impl Iterator<Item = &str> {
        self.sources.iter().map(|source| source.id.as_str())
    }
}
