//! Réception des mesures poussées par les agents, et gestion de leurs jetons.
//!
//! **Cette route ne s'authentifie pas par la session de l'interface.** Un agent
//! est un programme installé sur une machine distante : il présente un jeton
//! d'enregistrement dans `Authorization: Bearer …`, et rien d'autre. Si un
//! contrôle de session est appliqué à `/api/**`, `/api/ingest` doit en être
//! explicitement exempté, sans quoi plus aucune machine ne pourra remonter quoi
//! que ce soit.
//!
//! Les routes de jetons, elles, sont des routes d'administration ordinaires : la
//! session de l'interface doit les protéger comme les autres.

use std::sync::LazyLock;

use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use dumbmonit_collectors::log_gate::LogGate;
use dumbmonit_proto::{AGENT_SECRET_HEADER, PushBatch, TargetId};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::agent_tls::AgentCert;
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::clients;
use crate::auth::middleware::{AdminIdentity, Authenticated};
use crate::collectors::agent;
use crate::state::AppState;

/// Refus d'un lot dont l'identité n'est pas celle du certificat client.
pub const CERT_OF_ANOTHER_MACHINE: &str =
    "This agent certificate belongs to another machine: the batch is refused.";

/// Refus d'agents déjà journalisés : une ligne par client et par cause, pas une
/// par tentative — un agent au jeton révoqué réessaie toutes les 30 s.
static REFUSALS: LazyLock<LogGate> = LazyLock::new(LogGate::new);

/// Journalise un refus opposé à un agent, une fois par client et par cause.
/// `who` : adresse du client, suivie du nom de la machine quand on le connaît.
fn note_refusal(who: &str, status: StatusCode, reason: &str) {
    if REFUSALS.report(who, &format!("{} {reason}", status.as_u16())).is_some() {
        tracing::info!(client = who, status = status.as_u16(), reason, "agent report refused");
    }
}

/// Adresse lisible d'un client, pour les journaux.
fn client_label(ip: ClientIp) -> String {
    ip.0.map_or_else(|| "unknown".to_string(), |ip| ip.to_string())
}

/// Réponse d'erreur de l'ingestion.
///
/// `ApiError` ne sait pas dire « 401 » : jusqu'ici, l'API n'était appelée que par
/// l'interface, où un refus d'authentification n'existait pas. Plutôt que d'y
/// toucher — le fichier est partagé — cette route porte sa propre traduction, avec
/// exactement le même corps `{"error": …}` que le reste de l'API.
pub struct IngestRejection {
    status: StatusCode,
    message: String,
}

impl IntoResponse for IngestRejection {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<agent::IngestError> for IngestRejection {
    fn from(error: agent::IngestError) -> Self {
        match error {
            agent::IngestError::Unauthorized => Self {
                status: StatusCode::UNAUTHORIZED,
                message: "Enrollment token missing, unknown or revoked.".to_string(),
            },
            agent::IngestError::Forbidden(why) => {
                Self { status: StatusCode::FORBIDDEN, message: why }
            }
            agent::IngestError::BadRequest(why) => {
                Self { status: StatusCode::BAD_REQUEST, message: why }
            }
            agent::IngestError::Pending(why) => {
                Self { status: StatusCode::FORBIDDEN, message: why }
            }
            agent::IngestError::TooMany(why) => {
                Self { status: StatusCode::TOO_MANY_REQUESTS, message: why }
            }
            agent::IngestError::Internal(error) => {
                // Même politique que `ApiError` : la panne est journalisée en
                // entier, mais son détail ne part pas sur le réseau.
                tracing::error!(?error, "internal error during ingestion");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    message: "Internal server error.".to_string(),
                }
            }
        }
    }
}

/// Refuse une requête d'agent sans jeton valide *avant* d'en lire le corps.
///
/// Les routes d'agent acceptent des corps de 16 Mio ; sans cette garde, le
/// corps serait lu et désérialisé par l'extracteur `Json` avant que le
/// gestionnaire ne regarde le jeton — n'importe qui joignant le port pourrait
/// faire travailler le serveur à vide. Le gestionnaire revérifie le jeton : ce
/// contrôle-ci ne fait que trier tôt.
pub async fn require_agent_token(
    State(state): State<AppState>,
    ip: ClientIp,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let bearer = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    match agent::authenticate_token(&state.pool, bearer).await {
        Ok(Some(_)) => {
            // Sur le port de l'interface (pas de certificat client) : l'ancien
            // canal, refusé quand l'administrateur exige mTLS. Le refus
            // annonce le canal chiffré, comme toute réponse faite à un agent
            // authentifié : un agent à jour y migre de lui-même.
            let legacy = request.extensions().get::<crate::agent_tls::AgentCert>().is_none();
            let mut response = if legacy {
                match crate::agent_tls::mtls_required(&state).await {
                    Ok(true) => {
                        let reason = "plain agent channel refused (mTLS required)";
                        note_refusal(&client_label(ip), StatusCode::FORBIDDEN, reason);
                        IngestRejection {
                            status: StatusCode::FORBIDDEN,
                            message: dumbmonit_proto::MTLS_REQUIRED.to_string(),
                        }
                        .into_response()
                    }
                    Ok(false) => next.run(request).await,
                    Err(error) => {
                        return IngestRejection::from(agent::IngestError::Internal(error))
                            .into_response();
                    }
                }
            } else {
                next.run(request).await
            };
            if let Some(offer) = crate::agent_tls::offer(&state)
                && let Ok(value) = HeaderValue::from_str(&offer.to_header())
            {
                response.headers_mut().insert(dumbmonit_proto::AGENT_TLS_HEADER, value);
            }
            response
        }
        Ok(None) => {
            let reason = "enrolment token missing, unknown or revoked";
            note_refusal(&client_label(ip), StatusCode::UNAUTHORIZED, reason);
            IngestRejection::from(agent::IngestError::Unauthorized).into_response()
        }
        Err(error) => IngestRejection::from(agent::IngestError::Internal(error)).into_response(),
    }
}

/// Réception d'un lot de mesures.
///
/// `POST /api/ingest`, corps [`PushBatch`], réponse [`dumbmonit_proto::PushAck`].
pub async fn receive(
    State(state): State<AppState>,
    ip: ClientIp,
    headers: HeaderMap,
    cert: Option<Extension<AgentCert>>,
    batch: Result<Json<PushBatch>, JsonRejection>,
) -> Response {
    let client = client_label(ip);
    let batch = match batch {
        Ok(Json(batch)) => batch,
        Err(rejection) => {
            // Corps trop gros (413), illisible ou mal typé : l'agent ne le saura
            // que par son propre journal, le serveur le dit aussi.
            note_refusal(&client, rejection.status(), &rejection.body_text());
            return rejection.into_response();
        }
    };
    let bearer =
        headers.get(axum::http::header::AUTHORIZATION).and_then(|value| value.to_str().ok());
    // Le secret de liaison voyage dans son propre en-tête : il dit *quelle*
    // machine parle, là où le jeton dit seulement qu'elle a le droit de parler.
    let secret = headers.get(AGENT_SECRET_HEADER).and_then(|value| value.to_str().ok());
    let who = format!("{client} {}", batch.identity.hostname);
    let version = batch.identity.agent_version.clone();
    let identity = batch.identity.clone();
    let cert = cert.map(|Extension(cert)| cert);
    let declared = batch.declared_monitors.clone();
    let hostname = batch.identity.hostname.clone();

    // Sur le canal chiffré, le certificat dit quelle machine parle : un lot au
    // nom d'une autre est refusé avant d'avoir rien écrit.
    if let Some(cert) = &cert {
        match agent::target_of_key(&state.pool, &batch.identity.key()).await {
            Ok(Some(target)) if target == cert.target_id => {}
            Ok(_) => {
                note_refusal(&who, StatusCode::FORBIDDEN, "certificate of another machine");
                return IngestRejection {
                    status: StatusCode::FORBIDDEN,
                    message: CERT_OF_ANOTHER_MACHINE.to_string(),
                }
                .into_response();
            }
            Err(error) => {
                return IngestRejection::from(agent::IngestError::Internal(error)).into_response();
            }
        }
    }

    let origin = agent::EnrolOrigin::network(ip.0);
    match agent::ingest(&state.pool, &state.cipher, &state.sink, bearer, secret, batch, &origin)
        .await
    {
        Ok(mut ack) => {
            if let Err(error) =
                crate::agent_tls::note_channel(&state.pool, ack.target_id, cert.as_ref()).await
            {
                tracing::debug!(%error, "agent channel not recorded");
            }
            // Moniteurs déclarés par les étiquettes Docker : sans effet si le module
            // Docker est décoché pour cette machine.
            if let Some(declared) = declared
                && !ack.disabled_modules.iter().any(|module| module == "docker")
            {
                super::label_monitors::on_batch(&state, ack.target_id, &hostname, declared).await;
            }
            // Mise à jour automatique de l'agent : ce qu'il dit de son binaire,
            // puis l'offre du binaire de ce build si elle est de mise. Une
            // panne ici ne coûte jamais le lot.
            let target_id = ack.target_id;
            if let Err(error) = agent::selfupdate::record(&state.pool, target_id, &identity).await {
                tracing::warn!(error = %format!("{error:#}"), "agent update state not recorded");
            }
            match agent::selfupdate::offer_for(&state, target_id).await {
                Ok(offer) => ack.agent_update = offer,
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "agent update offer skipped");
                }
            }
            // Mode sentinelle : le réglage et son canal secret ne partent que
            // vers la machine désignée, et que sur le canal chiffré.
            match agent::sentinel::load(&state.pool, &state.cipher, target_id).await {
                Ok(Some(stored)) => {
                    ack.sentinel = Some(agent::sentinel::for_agent(stored, cert.is_some()));
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(error = %format!("{error:#}"), "sentinel setting skipped");
                }
            }
            if REFUSALS.clear(&who) {
                tracing::info!(client = who, "agent reports accepted again");
            }
            REFUSALS.clear(&client);
            Json(ack).into_response()
        }
        Err(error) => {
            let rejection = IngestRejection::from(error);
            // Une panne interne est déjà journalisée en erreur, en entier.
            if rejection.status != StatusCode::INTERNAL_SERVER_ERROR {
                let reason = format!("{} (agent {version})", rejection.message);
                note_refusal(&who, rejection.status, &reason);
            }
            rejection.into_response()
        }
    }
}

/// Un jeton tel que l'interface peut l'afficher : jamais le secret.
#[derive(Debug, Serialize)]
pub struct TokenView {
    pub id: i64,
    pub name: String,
    /// Début du jeton, pour l'identifier dans une liste.
    pub prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    /// Machines que ce jeton peut encore enrôler en tout. `null` : un jeton de
    /// parc, sans limite de nombre.
    pub max_uses: Option<i64>,
    /// Machines déjà enrôlées avec ce jeton.
    pub uses: i64,
    /// Terme au-delà duquel le jeton n'enrôle plus. Les machines déjà enrôlées
    /// continuent de remonter leurs mesures.
    pub expires_at: Option<String>,
    /// Jeton de relais : la machine enrôlée devient relais de son site.
    pub relay: bool,
    pub relay_site: Option<String>,
    /// Jeton de flotte Active Directory : domaine auquel il est limité.
    pub ad_domain: Option<String>,
    /// Mode MSP : le client chez lequel ce jeton enrôle.
    pub client_id: i64,
    /// Jeton de flotte en veille faute d'enrôlement récent : il n'enrôle plus
    /// tant qu'on ne le remet pas en service. `null` : en service.
    pub dormant_at: Option<String>,
}

impl From<agent::TokenRecord> for TokenView {
    fn from(record: agent::TokenRecord) -> Self {
        Self {
            id: record.id,
            name: record.name,
            prefix: record.prefix,
            created_at: record.created_at,
            last_used_at: record.last_used_at,
            revoked_at: record.revoked_at,
            max_uses: record.max_uses,
            uses: record.uses,
            expires_at: record.expires_at,
            relay: record.relay,
            relay_site: record.relay_site,
            ad_domain: record.ad_domain,
            client_id: clients::DEFAULT_CLIENT,
            dormant_at: None,
        }
    }
}

/// Réponse à la création d'un jeton.
///
/// C'est la seule et unique occasion de voir le jeton en clair : la base n'en
/// garde que l'empreinte. L'interface doit donc l'afficher immédiatement, avec la
/// commande d'installation toute faite.
#[derive(Debug, Serialize)]
pub struct CreatedToken {
    #[serde(flatten)]
    pub token: TokenView,
    /// Le jeton, en clair, pour cette réponse seulement.
    pub secret: String,
    pub install_linux: String,
    pub install_windows: String,
    /// Origine de l'empreinte `--pinned-key` (voir [`InstallParams::pin_source`]).
    pub pin_source: Option<&'static str>,
    /// Transport imposé par les commandes (voir [`InstallParams::transport`]).
    pub transport: Option<&'static str>,
}

#[derive(Debug, Deserialize)]
pub struct TokenPayload {
    /// Accepté par compatibilité, mais ignoré : le nom d'un jeton est toujours
    /// l'identifiant opaque généré par le serveur (voir
    /// [`agent::generate_token_name`]).
    #[serde(default)]
    #[allow(dead_code)]
    pub name: String,
    /// URL par laquelle les agents joindront ce serveur.
    ///
    /// Fournie par l'interface, qui est la seule à la connaître : le serveur, lui,
    /// ne voit que son adresse d'écoute, souvent `0.0.0.0` derrière un proxy.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Jeton réutilisable pour tout un parc. Absent : usage unique.
    ///
    /// Un jeton de parc est parfaitement légitime — c'est ce qu'on colle dans un
    /// playbook Ansible. Mais c'en est un qui ouvre autant de machines qu'on
    /// veut : ce doit être une décision, pas le comportement par défaut.
    #[serde(default)]
    pub reusable: bool,
    /// Nombre d'enrôlements permis pour un jeton réutilisable. Absent : sans
    /// limite. Ignoré pour un jeton à usage unique.
    #[serde(default)]
    pub max_uses: Option<i64>,
    /// Jours avant que le jeton cesse d'enrôler. Absent : sans terme.
    #[serde(default)]
    pub expires_in_days: Option<i64>,
    /// Jeton de relais : la machine enrôlée relaie les sondes de son site dès
    /// son premier lot, sans rien éditer sur elle.
    #[serde(default)]
    pub relay: bool,
    /// Site de ce relais (« Agence de Lyon »). Ignoré sans `relay`.
    #[serde(default)]
    pub site: Option<String>,
    /// Mode MSP : le client chez lequel le jeton enrôle. Absent : le client par
    /// défaut.
    #[serde(default)]
    pub client_id: Option<i64>,
    /// Installation à travers un agent relais (site derrière un pare-feu) : les
    /// commandes visent sa passerelle, que le serveur ouvre au besoin.
    #[serde(default)]
    pub relay_id: Option<TargetId>,
    /// Adresse par laquelle les machines du site joignent ce relais. Absente :
    /// celle déjà réglée, à défaut son nom d'hôte.
    #[serde(default)]
    pub relay_host: Option<String>,
    /// Commandes qui n'utilisent que le port des agents (scripts, binaires et
    /// canal chiffré sur 8443) : une machine d'un site distant n'a alors besoin
    /// que de ce port. Toujours vrai pour un jeton de relais.
    #[serde(default)]
    pub agent_port_only: bool,
    /// Transport du canal chiffré dans les commandes : `websocket` (par
    /// l'adresse de l'interface), `direct`, ou `auto` (défaut : la WebSocket
    /// derrière un intermédiaire qui termine TLS).
    #[serde(default)]
    pub transport: TransportWish,
}

impl TokenPayload {
    /// Portée effective du jeton, telle que le magasin l'attend.
    fn policy(&self) -> Result<agent::TokenPolicy, ApiError> {
        let max_uses = match (self.reusable, self.max_uses) {
            (false, _) => Some(1),
            (true, None) => None,
            (true, Some(count)) if count >= 1 => Some(count),
            (true, Some(_)) => {
                return Err(ApiError::BadRequest(
                    "A reusable token must allow at least one enrolment.".into(),
                ));
            }
        };
        if self.expires_in_days.is_some_and(|days| days < 1) {
            return Err(ApiError::BadRequest(
                "The enrolment deadline must be at least one day away.".into(),
            ));
        }
        Ok(agent::TokenPolicy { max_uses, expires_in_days: self.expires_in_days })
    }
}

/// Réservé aux administrateurs, comme la liste des jetons d'API.
pub async fn list_tokens(
    _: AdminIdentity,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<TokenView>>> {
    let tokens = agent::list_tokens(&state.pool).await?;
    let owners = clients::owners(&state.pool, clients::Kind::AgentToken).await?;
    let dormant = agent::enrolment::dormant_tokens(&state.pool).await?;
    Ok(Json(
        tokens
            .into_iter()
            .map(|record| {
                let client = owners.get(&record.id).copied();
                let asleep = dormant.get(&record.id).cloned();
                let mut view = TokenView::from(record);
                view.client_id = client.unwrap_or(clients::DEFAULT_CLIENT);
                view.dormant_at = asleep;
                view
            })
            .collect(),
    ))
}

pub async fn create_token(
    State(state): State<AppState>,
    Authenticated(me): Authenticated,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    Json(payload): Json<TokenPayload>,
) -> ApiResult<(StatusCode, Json<CreatedToken>)> {
    let site = agent::clean_site(payload.site.as_deref());
    let max = agent::MAX_SITE_LEN;
    if site.as_ref().is_some_and(|s| s.chars().count() > max) {
        return Err(ApiError::BadRequest(format!("Site name is limited to {max} characters.")));
    }
    let client = payload.client_id.unwrap_or(clients::DEFAULT_CLIENT);
    if !clients::exists(&state.pool, client).await? {
        return Err(ApiError::BadRequest("This client does not exist.".into()));
    }
    if payload.relay && payload.relay_id.is_some() {
        return Err(ApiError::BadRequest(
            "A relay connects to the server directly: it cannot be installed through another relay."
                .into(),
        ));
    }
    let route = match payload.relay_id {
        Some(relay) => {
            Some(relay_route(&state, relay, payload.relay_host.as_deref(), Some(client)).await?)
        }
        None => None,
    };
    let policy = payload.policy()?;
    if let Some(route) = &route {
        // Choisir ce relais pour une installation, c'est lui demander d'ouvrir
        // sa passerelle à cette adresse : il en reçoit le réglage aussitôt
        // (fin de son attente longue), l'ouvre, l'essaie et le rapporte.
        agent::set_relay_gateway(&state.pool, route.relay_id, Some(route.port), Some(&route.host))
            .await?;
        state.relay().nudge(route.relay_id);
    }
    let (mut record, secret) = agent::create_token(&state.pool, policy).await?;
    clients::assign(&state.pool, clients::Kind::AgentToken, &[record.id], Some(client))
        .await
        .map_err(super::clients::api_error)?;
    if payload.relay {
        agent::set_token_relay(&state.pool, record.id, site.as_deref()).await?;
        record.relay = true;
        record.relay_site = site;
    }
    audit::record(&state.pool, Some(&me.username), "agent_token.created", Some(&record.name), ip)
        .await;
    // Un relais est posé sur un site distant : sa commande n'utilise que le
    // port des agents, le seul qu'un site distant ait à joindre.
    let agent_port_only = payload.relay || payload.agent_port_only;
    let mut params = install_parameters_for(
        &state,
        payload.base_url.as_deref(),
        route,
        Some(&headers),
        agent_port_only,
        payload.transport,
    )
    .await;

    // Un relais ouvre sa passerelle d'office : l'installateur ouvre aussi le
    // port sur son pare-feu, au seul sous-réseau local.
    params.open_gateway = payload.relay;

    let mut token = TokenView::from(record);
    token.client_id = client;
    Ok((
        StatusCode::CREATED,
        Json(CreatedToken {
            install_linux: params.linux_command(&secret),
            install_windows: params.windows_command(&secret),
            token,
            secret,
            pin_source: params.pin_source,
            transport: params.transport,
        }),
    ))
}

mod install;

pub use install::*;

#[cfg(test)]
mod tests;
