//! Persistance de l'agent : jetons d'enregistrement et machines enregistrées.

use anyhow::{Context, Result};
use dumbmonit_proto::{
    AgentIdentity, Credential, GatewayCheck, GatewayState, MAX_SERVER_LINK_TEXT, ServerLink,
    TargetId,
};
use sqlx::{Row, SqlitePool};

use crate::crypto::Cipher;
use crate::db;

use super::token;

mod hosts;
mod tokens;

pub use hosts::*;
pub use tokens::*;

/// Période d'interrogation attribuée à une machine qui s'enregistre seule.
///
/// Elle sert aussi de période d'échantillonnage renvoyée à l'agent : ajuster la
/// cible dans l'interface pilote donc directement la finesse de la collecte.
const DEFAULT_INTERVAL_SECS: u64 = 30;

/// Résultat d'un enregistrement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub target_id: TargetId,
    /// Vrai si la cible vient d'être créée par ce lot.
    pub created: bool,
    /// Secret de liaison attribué à l'instant, à remettre à l'agent une fois.
    pub issued_secret: Option<String>,
    /// Vrai si la machine est désormais liée à un secret.
    pub bound: bool,
}

/// Pourquoi un lot correctement authentifié n'est pas accepté.
#[derive(Debug)]
pub enum RegisterError {
    /// La machine existe et est liée à un autre secret que celui présenté.
    /// C'est exactement le scénario qu'on ferme : une machine du parc qui se
    /// réclame de l'identité d'une autre.
    BindingMismatch,
    /// Le jeton est valide mais ne peut plus faire entrer de machine.
    EnrolmentDenied(EnrolmentDenied),
    /// La machine est connue mais n'a jamais été liée (enrôlée avant la
    /// liaison) : elle est refusée tant qu'un administrateur n'a pas ouvert la
    /// fenêtre de reliaison.
    NotBound,
    /// Le binaire ne sait pas recevoir de secret de liaison : la machine ne
    /// pourrait jamais être liée, donc elle n'entre pas.
    AgentTooOld,
    /// Garde de l'enrôlement : la machine attend sa validation, a été refusée,
    /// ou le jeton a dépassé son débit (voir [`super::enrolment`]).
    Held(super::enrolment::Refusal),
    Internal(anyhow::Error),
}

impl From<anyhow::Error> for RegisterError {
    fn from(error: anyhow::Error) -> Self {
        Self::Internal(error)
    }
}

impl From<sqlx::Error> for RegisterError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

/// La machine telle que la base la connaît, du point de vue de la liaison.
struct HostRow {
    target_id: TargetId,
    secret_hash: Option<String>,
    /// Jeton d'enrôlement qui a fait entrer (ou a lié) la machine.
    token_id: Option<i64>,
    /// Vrai si une fenêtre de reliaison est ouverte et encore valide.
    rebind_open: bool,
}

/// Retrouve la cible d'un agent, ou l'enregistre si elle n'existe pas encore.
///
/// C'est ce qui rend l'installation en une commande possible : la machine se
/// présente avec un jeton valide, et devient une cible sans que personne n'ait
/// rien saisi dans l'interface.
///
/// La clé d'identité (`/etc/machine-id`, ou le nom d'hôte) n'est un secret pour
/// personne : elle dit *qui prétend parler*, jamais *qui parle*. C'est le secret
/// de liaison, attribué à la première présentation et connu de cette machine
/// seule, qui fait la différence. Les trois cas :
///
/// - machine liée : le secret doit correspondre, sinon rien ne passe ;
/// - machine inconnue : le jeton doit pouvoir enrôler, l'agent doit savoir
///   recevoir un secret, et le secret est attribué ;
/// - machine connue mais non liée (agent installé avant la liaison) : refusée.
///   La période de transition est close : elle ne se lie plus que par une
///   fenêtre de reliaison ouverte par un administrateur.
#[cfg(test)]
pub async fn register(
    pool: &SqlitePool,
    cipher: &Cipher,
    identity: &AgentIdentity,
    token_id: i64,
    presented_secret: Option<&str>,
) -> Result<Registration, RegisterError> {
    let origin = super::enrolment::EnrolOrigin::Internal;
    register_with(pool, cipher, identity, token_id, presented_secret, &origin).await
}

/// [`register`], avec la garde de l'enrôlement appliquée à une machine
/// inconnue venue du réseau : validation, débit du jeton, mise en veille.
/// Tant que la garde la retient, rien n'est créé pour elle.
pub async fn register_with(
    pool: &SqlitePool,
    cipher: &Cipher,
    identity: &AgentIdentity,
    token_id: i64,
    presented_secret: Option<&str>,
    origin: &super::enrolment::EnrolOrigin,
) -> Result<Registration, RegisterError> {
    let key = identity.key();

    if let Some(host) = find_host(pool, &key).await? {
        return claim(pool, host, identity, token_id, presented_secret).await;
    }

    // Un binaire qui ne sait pas recevoir de secret donnerait une machine non
    // liée, que le lot suivant refuserait : autant le dire tout de suite.
    if !identity.binding_supported {
        tracing::warn!(hote = identity.hostname, "agent trop ancien pour être lié : refusé");
        return Err(RegisterError::AgentTooOld);
    }

    // Une machine inconnue est un enrôlement : c'est là, et seulement là, que la
    // portée du jeton compte.
    if let Err(denied) = enrolment_allowed_for(pool, token_id, identity).await? {
        return Err(RegisterError::EnrolmentDenied(denied));
    }

    // Garde de l'enrôlement, avant que quoi que ce soit n'existe pour elle.
    let now = chrono::Utc::now();
    match super::enrolment::admit(pool, token_id, identity, &key, origin, now).await? {
        Ok(_) => {}
        Err(super::enrolment::Refusal::Dormant) => {
            return Err(RegisterError::EnrolmentDenied(EnrolmentDenied::Dormant));
        }
        Err(refusal) => return Err(RegisterError::Held(refusal)),
    }

    // Mode MSP : la machine entre chez le client du jeton, et nulle part ailleurs.
    let client = crate::auth::clients::token_client(pool, token_id).await?;

    // Une cible « agent » a pu être créée à la main dans l'interface avant
    // l'installation : on l'adopte plutôt que de buter sur `UNIQUE (kind, address)`
    // et de laisser la machine dehors — chez le même client seulement.
    let existing =
        sqlx::query("SELECT id, client_id FROM targets WHERE kind = 'agent' AND address = ?")
            .bind(&key)
            .fetch_optional(pool)
            .await
            .context("looking up an existing agent target")?;

    let (target_id, created) = match existing {
        Some(row) => {
            if row.try_get::<i64, _>("client_id")? != client {
                tracing::warn!(
                    hote = identity.hostname,
                    "enrôlement refusé : machine d'un autre client"
                );
                return Err(RegisterError::EnrolmentDenied(EnrolmentDenied::OtherClient));
            }
            (row.try_get::<TargetId, _>("id")?, false)
        }
        None => {
            // Jeton de flotte Active Directory : la machine est rangée d'office
            // dans le dossier de son domaine, avec l'étiquette `ad`. L'OU
            // d'origine, quand le script de déploiement la connaît, arrive
            // dans les étiquettes de l'agent (`ad_ou`).
            let ad_domain = token_ad_domain(pool, token_id).await?;
            let mut tags = identity.tags.clone();
            let mut group_name = String::new();
            if let Some(domain) = &ad_domain {
                tags.insert("ad".to_string(), domain.clone());
                group_name = ad_folder(domain);
            }
            let input = db::targets::TargetInput {
                name: identity.hostname.clone(),
                // L'adresse d'une cible « agent » n'est pas une adresse de contact :
                // c'est l'agent qui se connecte. On y range donc son identité, ce
                // qui donne au passage l'unicité recherchée par le schéma.
                address: key.clone(),
                kind: "agent".to_string(),
                profile_id: None,
                parent_id: None,
                via_agent: None,
                interval: std::time::Duration::from_secs(DEFAULT_INTERVAL_SECS),
                enabled: true,
                tags,
                // Aucun secret à conserver de ce côté : c'est l'agent qui se
                // connecte, le serveur n'a jamais à joindre la machine.
                credential: Some(Credential::None),
                group_name,
            };
            match db::targets::create_in(pool, cipher, &input, client).await {
                Ok(id) => (id, true),
                Err(error) if error.downcast_ref::<db::targets::QuotaReached>().is_some() => {
                    return Err(RegisterError::EnrolmentDenied(EnrolmentDenied::QuotaReached));
                }
                Err(error) => return Err(error.into()),
            }
        }
    };

    let secret = token::generate_secret();
    insert_host(pool, target_id, &key, identity, token_id, Some(&secret)).await?;
    apply_token_relay(pool, target_id, token_id).await?;
    consume_enrolment(pool, token_id).await?;
    super::enrolment::note_enrolled(pool, token_id, &key, &identity.hostname, now).await?;
    Ok(Registration { target_id, created, bound: true, issued_secret: Some(secret) })
}

/// Décide si l'agent qui se présente est bien celui de cette machine.
async fn claim(
    pool: &SqlitePool,
    host: HostRow,
    identity: &AgentIdentity,
    token_id: i64,
    presented_secret: Option<&str>,
) -> Result<Registration, RegisterError> {
    let target_id = host.target_id;

    match host.secret_hash.as_deref() {
        // Machine liée : le secret décide, et lui seul.
        Some(stored) => {
            let matches = presented_secret
                .map(token::fingerprint)
                .is_some_and(|presented| presented == stored);
            if matches {
                update_host(pool, target_id, identity).await?;
                return Ok(Registration {
                    target_id,
                    created: false,
                    issued_secret: None,
                    bound: true,
                });
            }
            // Réinstallation : quelqu'un a explicitement ouvert la fenêtre dans
            // l'interface, et le jeton doit encore pouvoir enrôler.
            if host.rebind_open && identity.binding_supported {
                if let Err(denied) = enrolment_allowed_for(pool, token_id, identity).await? {
                    return Err(RegisterError::EnrolmentDenied(denied));
                }
                same_client(pool, target_id, token_id).await?;
                let secret = token::generate_secret();
                bind_host(pool, target_id, &secret, token_id).await?;
                update_host(pool, target_id, identity).await?;
                tracing::info!(cible = target_id, "machine reliée pendant sa fenêtre de reliaison");
                return Ok(Registration {
                    target_id,
                    created: false,
                    issued_secret: Some(secret),
                    bound: true,
                });
            }
            tracing::warn!(
                cible = target_id,
                hote = identity.hostname,
                "identité d'agent refusée : secret de liaison absent ou incorrect"
            );
            Err(RegisterError::BindingMismatch)
        }
        // Machine connue mais jamais liée : un agent installé avant la liaison,
        // accepté pendant la période de transition. Elle est close : la clé
        // d'identité (`/etc/machine-id`, nom d'hôte) se devine, et un jeton de
        // flotte est partagé par tout un parc, donc rien ne prouve que celui qui
        // se présente est la machine d'origine. Seule une fenêtre de reliaison,
        // ouverte par un administrateur, la fait entrer — avec un secret.
        None => {
            if !host.rebind_open {
                tracing::warn!(
                    cible = target_id,
                    hote = identity.hostname,
                    "machine jamais liée à son agent : refusée, réenrôlement nécessaire"
                );
                return Err(RegisterError::NotBound);
            }
            if !identity.binding_supported {
                return Err(RegisterError::AgentTooOld);
            }
            same_client(pool, target_id, token_id).await?;
            // Le jeton qui a enrôlé la machine l'a déjà comptée ; un autre doit
            // encore pouvoir enrôler.
            if host.token_id != Some(token_id)
                && let Err(denied) = enrolment_allowed_for(pool, token_id, identity).await?
            {
                return Err(RegisterError::EnrolmentDenied(denied));
            }
            let secret = token::generate_secret();
            bind_host(pool, target_id, &secret, token_id).await?;
            update_host(pool, target_id, identity).await?;
            tracing::info!(
                cible = target_id,
                hote = identity.hostname,
                "machine liée à son agent pendant sa fenêtre de reliaison"
            );
            Ok(Registration { target_id, created: false, bound: true, issued_secret: Some(secret) })
        }
    }
}

/// Mode MSP : une machine ne se relie qu'avec un jeton de son client. Une
/// machine déjà liée, elle, est reconnue à son secret, quel que soit le jeton.
async fn same_client(
    pool: &SqlitePool,
    target_id: TargetId,
    token_id: i64,
) -> Result<(), RegisterError> {
    let target = crate::auth::clients::target_client(pool, target_id).await?;
    let token = crate::auth::clients::token_client(pool, token_id).await?;
    if target != Some(token) {
        tracing::warn!(cible = target_id, "reliaison refusée : jeton d'un autre client");
        return Err(RegisterError::EnrolmentDenied(EnrolmentDenied::OtherClient));
    }
    Ok(())
}

async fn find_host(pool: &SqlitePool, key: &str) -> Result<Option<HostRow>> {
    let row = sqlx::query(
        "SELECT target_id, secret_hash, token_id,
                (rebind_until IS NOT NULL
                 AND rebind_until > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) AS rebind_open
         FROM agent_hosts WHERE agent_key = ?",
    )
    .bind(key)
    .fetch_optional(pool)
    .await
    .context("looking up the machine")?;
    row.map(|row| {
        Ok(HostRow {
            target_id: row.try_get("target_id")?,
            secret_hash: row.try_get("secret_hash")?,
            token_id: row.try_get("token_id")?,
            rebind_open: row.try_get::<i64, _>("rebind_open")? != 0,
        })
    })
    .transpose()
}

/// Enregistre l'empreinte du secret d'une machine, et referme la fenêtre de
/// reliaison : elle a servi.
async fn bind_host(
    pool: &SqlitePool,
    target_id: TargetId,
    secret: &str,
    token_id: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE agent_hosts
         SET secret_hash = ?, binding_supported = 1, rebind_until = NULL, token_id = ?,
             bound_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE target_id = ?",
    )
    .bind(token::fingerprint(secret))
    .bind(token_id)
    .bind(target_id)
    .execute(pool)
    .await
    .context("binding the machine to its agent")?;
    // Réinstallée avec un jeton de relais : elle le devient.
    apply_token_relay(pool, target_id, token_id).await
}

/// Ouvre une fenêtre de reliaison : la machine pourra se relier au prochain lot
/// d'un agent porteur d'un jeton valide. Renvoie l'instant de fermeture.
///
/// C'est la seule issue prévue quand un secret est perdu — réinstallation,
/// disque remplacé, conteneur d'agent recréé sans son volume. Elle est explicite
/// et datée, parce qu'une machine liée doit le rester tant que personne n'a
/// décidé le contraire.
pub async fn allow_rebind(pool: &SqlitePool, target_id: TargetId) -> Result<Option<String>> {
    let until = (chrono::Utc::now() + chrono::Duration::minutes(REBIND_WINDOW_MINUTES))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    let result = sqlx::query("UPDATE agent_hosts SET rebind_until = ? WHERE target_id = ?")
        .bind(&until)
        .bind(target_id)
        .execute(pool)
        .await
        .context("opening the re-enrolment window")?;
    Ok((result.rows_affected() > 0).then_some(until))
}

/// Machine qui porte une clé d'identité, si elle est connue.
pub async fn target_of_key(pool: &SqlitePool, key: &str) -> Result<Option<TargetId>> {
    Ok(find_host(pool, key).await?.map(|host| host.target_id))
}

/// Verdict rendu à une requête qui se présente avec une clé d'identité.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAuth {
    /// Aucune machine ne porte cette clé.
    Unknown,
    /// La clé existe, mais le secret présenté n'est pas le sien.
    Denied,
    /// La machine n'a jamais été liée : refusée jusqu'à son réenrôlement.
    NotBound,
    /// Requête acceptée : la machine est liée et le secret est le sien.
    Allowed { target_id: TargetId },
}

/// Autorise une requête du canal de commandes ou du relais.
///
/// Même règle que pour l'ingestion, et pour la même raison : sans elle, une
/// machine du parc viendrait chercher — et consommerait — les commandes Docker
/// destinées à une autre.
pub async fn authorise_key(
    pool: &SqlitePool,
    key: &str,
    presented_secret: Option<&str>,
) -> Result<KeyAuth> {
    let Some(host) = find_host(pool, key).await? else {
        return Ok(KeyAuth::Unknown);
    };
    match host.secret_hash.as_deref() {
        Some(stored) => {
            let matches = presented_secret
                .map(token::fingerprint)
                .is_some_and(|presented| presented == stored);
            if matches {
                Ok(KeyAuth::Allowed { target_id: host.target_id })
            } else {
                Ok(KeyAuth::Denied)
            }
        }
        // Machine d'avant la liaison : plus de commandes ni de relais tant
        // qu'elle n'est pas liée. Rien ne distingue son agent d'un voisin qui
        // aurait deviné sa clé.
        None => Ok(KeyAuth::NotBound),
    }
}

async fn insert_host(
    pool: &SqlitePool,
    target_id: TargetId,
    key: &str,
    identity: &AgentIdentity,
    token_id: i64,
    secret: Option<&str>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO agent_hosts
             (target_id, agent_key, hostname, os, os_version, kernel_version, arch,
              agent_version, commands_enabled, relay, site, relay_control, relay_local,
              token_id, secret_hash, binding_supported, bound_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                 CASE WHEN ? IS NULL THEN NULL
                      ELSE strftime('%Y-%m-%dT%H:%M:%SZ', 'now') END)",
    )
    .bind(target_id)
    .bind(key)
    .bind(&identity.hostname)
    .bind(&identity.os)
    .bind(&identity.os_version)
    .bind(&identity.kernel_version)
    .bind(&identity.arch)
    .bind(&identity.agent_version)
    .bind(identity.commands_enabled)
    .bind(i64::from(identity.relay))
    .bind(site_of(identity))
    .bind(i64::from(identity.relay_control))
    .bind(i64::from(identity.relay_local))
    .bind(token_id)
    .bind(secret.map(token::fingerprint))
    .bind(i64::from(identity.binding_supported))
    .bind(secret)
    .execute(pool)
    .await
    .context("registering the machine")?;
    store_remediation(pool, target_id, identity).await
}

/// Rafraîchit la description de la machine à chaque lot.
///
/// Une mise à jour du système ou de l'agent doit se voir dans l'interface sans
/// qu'on ait à désinstaller quoi que ce soit.
async fn update_host(
    pool: &SqlitePool,
    target_id: TargetId,
    identity: &AgentIdentity,
) -> Result<()> {
    sqlx::query(
        "UPDATE agent_hosts
         SET hostname = ?, os = ?, os_version = ?, kernel_version = ?, arch = ?,
             agent_version = ?, commands_enabled = ?, relay = ?, site = ?,
             relay_control = ?, relay_local = ?, binding_supported = ?, relay_gateway_port = ?,
             relay_gateway_check = ?, relay_gateway_state = ?, server_link = ?
         WHERE target_id = ?",
    )
    .bind(&identity.hostname)
    .bind(&identity.os)
    .bind(&identity.os_version)
    .bind(&identity.kernel_version)
    .bind(&identity.arch)
    .bind(&identity.agent_version)
    .bind(identity.commands_enabled)
    .bind(i64::from(identity.relay))
    .bind(site_of(identity))
    .bind(i64::from(identity.relay_control))
    .bind(i64::from(identity.relay_local))
    .bind(i64::from(identity.binding_supported))
    .bind(identity.relay_gateway.filter(|_| identity.relay).map(i64::from))
    .bind(gateway_check_json(identity))
    .bind(gateway_state_json(identity))
    .bind(server_link_json(identity))
    .bind(target_id)
    .execute(pool)
    .await
    .context("updating the machine")?;
    store_remediation(pool, target_id, identity).await
}

/// Range ce que l'agent déclare de ses actions de remédiation, borné et
/// assaini : la déclaration vient d'une machine que le serveur ne contrôle pas.
async fn store_remediation(
    pool: &SqlitePool,
    target_id: TargetId,
    identity: &AgentIdentity,
) -> Result<()> {
    let declared = identity
        .remediation
        .clone()
        .map(|info| serde_json::to_string(&info.sanitized()))
        .transpose()
        .context("encoding the remediation declaration")?;
    sqlx::query("UPDATE agent_hosts SET remediation = ? WHERE target_id = ?")
        .bind(declared)
        .bind(target_id)
        .execute(pool)
        .await
        .context("recording the remediation declaration")?;
    Ok(())
}

/// Serveur en service que l'agent rapporte (`server_urls`), borné, en JSON.
/// Rien pour un agent qui ne connaît qu'une adresse : la colonne s'efface
/// quand on retire ses secours.
fn server_link_json(identity: &AgentIdentity) -> Option<String> {
    let link = identity.server_link.as_ref()?;
    let servers = link.servers.clamp(1, 64);
    let clip = |text: &str| text.chars().take(MAX_SERVER_LINK_TEXT).collect::<String>();
    let bounded = ServerLink {
        active: link.active.min(servers - 1),
        servers,
        url: clip(&link.url),
        since_ms: link.since_ms,
        primary_error: link.primary_error.as_deref().map(clip),
    };
    serde_json::to_string(&bounded).ok()
}

/// État de la passerelle que le relais rapporte (adresse liée ou cause de
/// l'échec), borné, en JSON. Rien pour un agent qui n'est pas relais.
fn gateway_state_json(identity: &AgentIdentity) -> Option<String> {
    let state = identity.relay_gateway_state.as_ref().filter(|_| identity.relay)?;
    let clip = |text: &str, max: usize| text.chars().take(max).collect::<String>();
    // `[ipv6]:port` au plus, comme l'adresse d'un essai.
    let address = MAX_GATEWAY_HOST_LEN + 8;
    let bounded = GatewayState {
        wanted: clip(&state.wanted, address),
        listen: state.listen.as_deref().map(|listen| clip(listen, address)),
        error: state
            .error
            .as_deref()
            .map(|error| clip(error, dumbmonit_proto::MAX_GATEWAY_CHECK_ERROR)),
        local: state.local,
        encrypted: state.encrypted,
    };
    serde_json::to_string(&bounded).ok()
}

/// Essai de passerelle que le relais rapporte, borné, en JSON. Rien pour un
/// agent qui n'est pas relais, ou sans passerelle.
fn gateway_check_json(identity: &AgentIdentity) -> Option<String> {
    let check = identity.relay_gateway_check.as_ref().filter(|_| identity.relay)?;
    identity.relay_gateway.filter(|port| *port > 0)?;
    let clip = |text: &str, max: usize| text.chars().take(max).collect::<String>();
    let bounded = GatewayCheck {
        address: clip(check.address.as_str(), MAX_GATEWAY_HOST_LEN + 8),
        ok: check.ok,
        error: check
            .error
            .as_deref()
            .map(|error| clip(error, dumbmonit_proto::MAX_GATEWAY_CHECK_ERROR)),
        checked_at_ms: check.checked_at_ms,
    };
    serde_json::to_string(&bounded).ok()
}

/// Site déclaré par l'agent, vidé de ses blancs ; `None` s'il n'en dit rien.
fn site_of(identity: &AgentIdentity) -> Option<String> {
    identity.site.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Note la réception d'un lot. C'est cette trace que le collecteur relit pour
/// juger de la fraîcheur d'une machine.
pub async fn record_batch(
    pool: &SqlitePool,
    target_id: TargetId,
    received_at_ms: i64,
    samples: usize,
) -> Result<()> {
    sqlx::query(
        "UPDATE agent_hosts
         SET last_seen_ms = ?,
             last_seen_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
             last_samples = ?
         WHERE target_id = ?",
    )
    .bind(received_at_ms)
    .bind(samples as i64)
    .bind(target_id)
    .execute(pool)
    .await
    .context("recording the batch receipt")?;
    Ok(())
}

#[cfg(test)]
mod tests;
