//! Jetons d'enregistrement : politique, noms, domaine AD, relais et révocation.

use super::*;

/// Durée de la fenêtre ouverte par « Allow re-enrolment », en minutes.
///
/// Assez pour réinstaller un agent sans se presser, trop court pour qu'une
/// fenêtre oubliée ouverte redevienne le trou qu'on vient de boucher.
pub const REBIND_WINDOW_MINUTES: i64 = 60;

/// Ce qu'un jeton a le droit d'enrôler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenPolicy {
    /// Nombre d'enrôlements permis. `None` : jeton de parc, sans limite.
    pub max_uses: Option<i64>,
    /// Nombre de jours avant que le jeton cesse d'enrôler. `None` : sans terme.
    pub expires_in_days: Option<i64>,
}

impl Default for TokenPolicy {
    /// Usage unique : une commande d'installation copiée dans une conversation
    /// ou retrouvée dans l'historique d'un terminal ne doit pas suffire à faire
    /// entrer une seconde machine.
    fn default() -> Self {
        Self { max_uses: Some(1), expires_in_days: None }
    }
}

/// Un jeton, tel qu'il peut être montré. Jamais le secret lui-même.
#[derive(Debug, Clone)]
pub struct TokenRecord {
    pub id: i64,
    pub name: String,
    pub prefix: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub revoked_at: Option<String>,
    /// Enrôlements permis, `None` pour un jeton de parc.
    pub max_uses: Option<i64>,
    /// Enrôlements déjà servis.
    pub uses: i64,
    /// Terme au-delà duquel le jeton n'enrôle plus. Les machines déjà enrôlées
    /// continuent de remonter leurs mesures.
    pub expires_at: Option<String>,
    /// Vrai pour un jeton créé par le parcours « site distant » : la machine
    /// qu'il enrôle devient relais dès son premier lot.
    pub relay: bool,
    /// Site donné à ce relais, s'il y en a un.
    pub relay_site: Option<String>,
    /// Jeton de flotte Active Directory : domaine DNS auquel il est limité.
    pub ad_domain: Option<String>,
}

/// Longueur maximale d'un nom de site : il s'affiche sur une carte et dans une
/// liste, il n'a pas à être un paragraphe.
pub const MAX_SITE_LEN: usize = 80;

/// Nom de site normalisé : sans blancs autour, `None` s'il est vide.
pub fn clean_site(site: Option<&str>) -> Option<String> {
    site.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Pourquoi un jeton valide n'enrôle pas cette machine-ci.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrolmentDenied {
    /// Tous les enrôlements permis ont été servis.
    Exhausted,
    /// Le terme d'enrôlement est passé.
    Expired,
    /// Jeton de flotte Active Directory : la machine ne se présente pas sous
    /// un nom complet du domaine auquel il est limité.
    OutsideDomain,
    /// Mode MSP : la machine est connue chez un autre client que celui du
    /// jeton. Un jeton n'enrôle, n'adopte ni ne relie que chez son client.
    OtherClient,
    /// Mode MSP : le client du jeton a atteint son quota d'équipements.
    QuotaReached,
    /// Jeton de flotte mis en veille faute d'enrôlement récent (voir
    /// [`super::enrolment`]). Les machines déjà enrôlées continuent de remonter.
    Dormant,
}

impl EnrolmentDenied {
    /// Message rendu tel quel à l'agent, donc dans les journaux de la machine.
    pub fn message(self) -> &'static str {
        match self {
            Self::Exhausted => {
                "This enrollment token has already enrolled all the machines it was \
                 allowed to. Create a new token, or a reusable one for a fleet."
            }
            Self::Expired => {
                "This enrollment token has passed its enrollment deadline. Create a new \
                 one; machines already enrolled keep reporting."
            }
            Self::OutsideDomain => {
                "This enrollment token is limited to an Active Directory domain, and this \
                 machine does not report a fully qualified name in it. Install the agent \
                 with -HostName <name>.<domain>, or use another token."
            }
            Self::OtherClient => {
                "This enrollment token belongs to another client than this machine. Use an \
                 enrollment token of the machine's client."
            }
            Self::QuotaReached => {
                "The client of this enrollment token has reached its target quota. Ask your \
                 MSP administrator to raise it."
            }
            Self::Dormant => {
                "This fleet enrollment token enrolled no machine for a long time and was put to \
                 sleep. An administrator can reactivate it in DumbMonit (Settings > Agents > \
                 Enrolment tokens); machines already enrolled keep reporting."
            }
        }
    }
}

/// Préfixe des noms de jeton d'enrôlement.
pub(super) const TOKEN_NAME_PREFIX: &str = "tok_";
/// Nombre de caractères aléatoires après le préfixe.
pub(super) const TOKEN_NAME_LEN: usize = 10;
/// Alphabet base32 minuscule (RFC 4648), 32 symboles : un octet aléatoire masqué
/// sur 5 bits le choisit sans biais.
pub(super) const TOKEN_NAME_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

/// Identifiant opaque d'un jeton d'enrôlement : `tok_` + 10 caractères base32
/// minuscules tirés du générateur du système. Il ne dit rien de la machine ni de
/// la date : on retrouve l'usage d'un jeton par ses données, pas par son nom.
pub fn generate_token_name() -> String {
    let bytes: [u8; TOKEN_NAME_LEN] = rand::random();
    let mut name = String::from(TOKEN_NAME_PREFIX);
    name.extend(bytes.iter().map(|b| TOKEN_NAME_ALPHABET[(b & 31) as usize] as char));
    name
}

/// Vrai si `name` a déjà la forme d'un identifiant opaque.
pub fn is_token_name(name: &str) -> bool {
    name.strip_prefix(TOKEN_NAME_PREFIX).is_some_and(|rest| {
        rest.len() == TOKEN_NAME_LEN && rest.bytes().all(|b| TOKEN_NAME_ALPHABET.contains(&b))
    })
}

/// Identifiant opaque qu'aucun jeton ne porte encore (nouveau tirage en cas de
/// collision).
pub async fn unused_token_name(pool: &SqlitePool) -> Result<String> {
    loop {
        let name = generate_token_name();
        let taken = sqlx::query("SELECT 1 FROM agent_tokens WHERE name = ?")
            .bind(&name)
            .fetch_optional(pool)
            .await
            .context("checking the enrollment token name")?;
        if taken.is_none() {
            return Ok(name);
        }
    }
}

/// Crée un jeton et renvoie sa forme en clair — la seule et unique fois.
pub async fn create_token(pool: &SqlitePool, policy: TokenPolicy) -> Result<(TokenRecord, String)> {
    let name = unused_token_name(pool).await?;
    let clear = token::generate();
    let expires_at = policy.expires_in_days.map(|days| {
        (chrono::Utc::now() + chrono::Duration::days(days)).format("%Y-%m-%dT%H:%M:%SZ").to_string()
    });
    let row = sqlx::query(
        "INSERT INTO agent_tokens (name, token_hash, prefix, max_uses, expires_at)
         VALUES (?, ?, ?, ?, ?)
         RETURNING id, name, prefix, created_at, last_used_at, revoked_at,
                   max_uses, uses, expires_at, relay, relay_site, ad_domain",
    )
    .bind(&name)
    .bind(token::fingerprint(&clear))
    .bind(token::display_prefix(&clear))
    .bind(policy.max_uses)
    .bind(expires_at)
    .fetch_one(pool)
    .await
    .context("creating the enrollment token")?;

    Ok((row_to_token(&row)?, clear))
}

pub async fn list_tokens(pool: &SqlitePool) -> Result<Vec<TokenRecord>> {
    let rows = sqlx::query(
        "SELECT id, name, prefix, created_at, last_used_at, revoked_at,
                max_uses, uses, expires_at, relay, relay_site, ad_domain
         FROM agent_tokens ORDER BY id DESC",
    )
    .fetch_all(pool)
    .await
    .context("listing enrollment tokens")?;
    rows.iter().map(row_to_token).collect()
}

/// Machine enrôlée par un jeton, telle que le flux d'ajout l'attend.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TokenMachine {
    pub id: TargetId,
    pub name: String,
    pub hostname: String,
    pub os: String,
    pub last_seen_at: Option<String>,
    /// Relais par lequel la machine joint le serveur, s'il y en a un.
    pub via_agent: Option<TargetId>,
}

/// Machines enrôlées par un jeton, la plus récente d'abord : c'est ce que
/// l'assistant d'ajout surveille pour constater la première connexion.
pub async fn token_machines(pool: &SqlitePool, token_id: i64) -> Result<Vec<TokenMachine>> {
    let rows = sqlx::query(
        "SELECT t.id, t.name, t.via_agent, h.hostname, h.os, h.last_seen_at
         FROM agent_hosts h JOIN targets t ON t.id = h.target_id
         WHERE h.token_id = ?
         ORDER BY t.id DESC
         LIMIT 50",
    )
    .bind(token_id)
    .fetch_all(pool)
    .await
    .context("listing the machines of a token")?;
    rows.iter()
        .map(|row| {
            Ok(TokenMachine {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                hostname: row.try_get("hostname")?,
                os: row.try_get("os")?,
                last_seen_at: row.try_get("last_seen_at")?,
                via_agent: row.try_get("via_agent")?,
            })
        })
        .collect()
}

/// Fait d'un jeton un jeton de relais : la machine qu'il enrôle (ou relie)
/// devient relais pour `site`, sans rien éditer sur elle.
pub async fn set_token_relay(pool: &SqlitePool, id: i64, site: Option<&str>) -> Result<()> {
    sqlx::query("UPDATE agent_tokens SET relay = 1, relay_site = ? WHERE id = ?")
        .bind(clean_site(site))
        .bind(id)
        .execute(pool)
        .await
        .context("marking the token as a relay token")?;
    Ok(())
}

/// Longueur maximale d'un nom de domaine DNS (RFC 1035).
pub const MAX_DOMAIN_LEN: usize = 253;

/// Domaine Active Directory normalisé : minuscules, sans point final ni
/// blancs. `None` s'il est vide ou n'a pas la forme d'un nom DNS
/// (`corp.example.com` : des étiquettes de lettres, chiffres et tirets).
///
/// Strict à dessein : ce nom finit dans le nom d'un dossier, dans une
/// étiquette, et surtout dans un script PowerShell exécuté en administrateur
/// du domaine. Rien d'autre que ces caractères n'a à y entrer.
pub fn clean_ad_domain(domain: &str) -> Option<String> {
    let domain = domain.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.is_empty() || domain.len() > MAX_DOMAIN_LEN {
        return None;
    }
    let valid_label = |label: &str| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    };
    domain.split('.').all(valid_label).then_some(domain)
}

/// Vrai si `hostname` est un nom complet du domaine `domain` (déjà normalisé),
/// sous-domaines compris : `srv01.corp.example.com` et
/// `srv02.paris.corp.example.com` sont dans `corp.example.com`, `srv01` ou
/// `srv01.evilcorp.example.com` non.
pub fn hostname_in_domain(hostname: &str, domain: &str) -> bool {
    let hostname = hostname.trim().trim_end_matches('.').to_ascii_lowercase();
    hostname.strip_suffix(domain).is_some_and(|head| head.len() > 1 && head.ends_with('.'))
}

/// Dossier dans lequel sont rangées les machines d'un domaine.
pub fn ad_folder(domain: &str) -> String {
    format!("Active Directory {domain}")
}

/// Limite un jeton à un domaine Active Directory (voir [`clean_ad_domain`]).
pub async fn set_token_ad_domain(pool: &SqlitePool, id: i64, domain: &str) -> Result<()> {
    sqlx::query("UPDATE agent_tokens SET ad_domain = ? WHERE id = ?")
        .bind(domain)
        .bind(id)
        .execute(pool)
        .await
        .context("limiting the token to an Active Directory domain")?;
    Ok(())
}

/// Clôt les inscriptions d'un jeton sans couper les machines qu'il a fait
/// entrer : son terme est ramené à maintenant. C'est l'arrêt qui convient à un
/// jeton de flotte une fois le parc déployé — le révoquer, lui, ferait taire
/// toutes les machines qui l'utilisent. Faux si le jeton est inconnu ou
/// révoqué.
pub async fn close_enrolment(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_tokens
         SET expires_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now', '-1 second')
         WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("closing the token enrolments")?;
    Ok(result.rows_affected() > 0)
}

/// Domaine Active Directory auquel un jeton est limité, s'il l'est.
pub(super) async fn token_ad_domain(pool: &SqlitePool, token_id: i64) -> Result<Option<String>> {
    let row = sqlx::query("SELECT ad_domain FROM agent_tokens WHERE id = ?")
        .bind(token_id)
        .fetch_optional(pool)
        .await
        .context("reading the token domain")?;
    Ok(row.map(|row| row.try_get::<Option<String>, _>("ad_domain")).transpose()?.flatten())
}

/// Ce jeton peut-il faire entrer *cette* machine ? Ses limites
/// ([`enrolment_allowed`]), puis son domaine s'il en a un.
pub(super) async fn enrolment_allowed_for(
    pool: &SqlitePool,
    token_id: i64,
    identity: &AgentIdentity,
) -> Result<Result<(), EnrolmentDenied>> {
    if let Err(denied) = enrolment_allowed(pool, token_id).await? {
        return Ok(Err(denied));
    }
    if let Some(domain) = token_ad_domain(pool, token_id).await?
        && !hostname_in_domain(&identity.hostname, &domain)
    {
        tracing::warn!(
            hote = identity.hostname,
            domaine = domain,
            "inscription refusée : machine hors du domaine du jeton"
        );
        return Ok(Err(EnrolmentDenied::OutsideDomain));
    }
    Ok(Ok(()))
}

/// Reporte sur la machine le mode relais porté par son jeton d'enrôlement.
/// Sans effet pour un jeton ordinaire : un réglage fait dans l'interface reste.
pub(super) async fn apply_token_relay(pool: &SqlitePool, target_id: TargetId, token_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE agent_hosts
         SET relay_wanted = 1,
             relay_wanted_site = COALESCE(
                 (SELECT relay_site FROM agent_tokens WHERE id = ?), relay_wanted_site)
         WHERE target_id = ?
           AND EXISTS (SELECT 1 FROM agent_tokens WHERE id = ? AND relay = 1)",
    )
    .bind(token_id)
    .bind(target_id)
    .bind(token_id)
    .execute(pool)
    .await
    .context("applying the relay mode of the token")?;
    Ok(())
}

/// Règle depuis l'interface le mode relais d'une machine. Faux si aucune
/// machine à agent ne porte cette cible.
pub async fn set_relay_wanted(
    pool: &SqlitePool,
    target_id: TargetId,
    enabled: bool,
    site: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_hosts SET relay_wanted = ?, relay_wanted_site = ? WHERE target_id = ?",
    )
    .bind(i64::from(enabled))
    .bind(clean_site(site))
    .bind(target_id)
    .execute(pool)
    .await
    .context("saving the relay mode")?;
    Ok(result.rows_affected() > 0)
}

/// Mode relais à transmettre à l'agent dans son accusé de réception.
///
/// La passerelle d'un relais est ouverte d'office (port par défaut) tant que
/// personne ne l'a fermée : un relais tout juste installé accepte les agents
/// de son site dès son premier accusé de réception.
pub async fn relay_setting(pool: &SqlitePool, target_id: TargetId) -> Result<Option<RelaySetting>> {
    let row = sqlx::query(
        "SELECT relay_local, relay_wanted, relay_wanted_site, relay_gateway_wanted, relay_gateway_host
         FROM agent_hosts WHERE target_id = ?",
    )
    .bind(target_id)
    .fetch_optional(pool)
    .await
    .context("reading the relay mode")?;
    row.map(|row| {
        let enabled = row.try_get::<i64, _>("relay_wanted")? != 0;
        // Relais demandé depuis l'interface, ou posé dans l'agent.yaml.
        let relay = enabled || row.try_get::<i64, _>("relay_local")? != 0;
        Ok(RelaySetting {
            enabled,
            site: row.try_get("relay_wanted_site")?,
            gateway_port: gateway_wanted_of(row.try_get("relay_gateway_wanted")?, relay),
            gateway_host: row.try_get("relay_gateway_host")?,
        })
    })
    .transpose()
}

/// Port lu en base, écarté s'il n'en est pas un.
pub(super) fn port_of(raw: Option<i64>) -> Option<u16> {
    raw.and_then(|port| u16::try_from(port).ok()).filter(|port| *port > 0)
}

/// Port de passerelle demandé, tel que rangé : `NULL`, jamais réglé (un relais
/// l'ouvre sur le port par défaut) ; `0`, fermée depuis l'interface ; sinon le
/// port. `None` : pas de passerelle.
pub fn gateway_wanted_of(raw: Option<i64>, relay: bool) -> Option<u16> {
    match raw {
        None if relay => Some(dumbmonit_proto::DEFAULT_RELAY_GATEWAY_PORT),
        None => None,
        Some(port) => port_of(Some(port)),
    }
}

/// Longueur maximale de l'adresse de passerelle d'un relais.
pub const MAX_GATEWAY_HOST_LEN: usize = 253;

/// Adresse de passerelle nettoyée : un nom d'hôte ou une IP, sans schéma, sans
/// port, sans espace. `None` si vide ; `Err` si ce n'est pas une adresse.
pub fn clean_gateway_host(raw: Option<&str>) -> Result<Option<String>, &'static str> {
    let Some(host) = raw.map(str::trim).filter(|host| !host.is_empty()) else {
        return Ok(None);
    };
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    let valid = host.len() <= MAX_GATEWAY_HOST_LEN
        && (bare.parse::<std::net::IpAddr>().is_ok()
            || bare.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-')));
    if !valid {
        return Err(
            "The relay address must be a host name or an IP address, without scheme or port.",
        );
    }
    // Une IPv6 s'écrit entre crochets dans une URL.
    Ok(Some(if bare.contains(':') { format!("[{bare}]") } else { bare.to_string() }))
}

/// Règle depuis l'interface la passerelle d'un relais : port d'écoute (`None` :
/// fermée, rangée `0` pour la distinguer d'une passerelle jamais réglée, qui
/// s'ouvre d'office) et adresse par laquelle les machines du site le joignent.
/// Faux si aucune machine à agent ne porte cette cible.
pub async fn set_relay_gateway(
    pool: &SqlitePool,
    target_id: TargetId,
    port: Option<u16>,
    host: Option<&str>,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_hosts SET relay_gateway_wanted = ?, relay_gateway_host = ?
         WHERE target_id = ?",
    )
    .bind(port.map_or(0, i64::from))
    .bind(host)
    .bind(target_id)
    .execute(pool)
    .await
    .context("saving the relay gateway")?;
    Ok(result.rows_affected() > 0)
}

/// Révoque un jeton. Idempotent : révoquer deux fois n'est pas une erreur, mais
/// seule la première révocation renvoie « vrai ».
pub async fn revoke_token(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE agent_tokens
         SET revoked_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
         WHERE id = ? AND revoked_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("revoking the token")?;
    Ok(result.rows_affected() > 0)
}

/// Retrouve un jeton actif à partir de son empreinte.
///
/// La recherche porte sur l'empreinte, jamais sur le jeton : c'est ce qui permet
/// de s'appuyer sur un index — donc une comparaison en temps constant du point de
/// vue de l'attaquant, qui ne peut de toute façon pas deviner l'empreinte d'un
/// secret qu'il ne possède pas.
///
/// « Actif » veut dire « non révoqué », et rien de plus : un jeton à usage unique
/// épuisé, ou dont le terme d'enrôlement est passé, laisse toujours remonter les
/// mesures de la machine qu'il a fait entrer. C'est [`enrolment_allowed`] qui
/// décide, séparément, s'il peut en faire entrer une de plus.
pub async fn find_active_token(pool: &SqlitePool, fingerprint: &str) -> Result<Option<i64>> {
    let row =
        sqlx::query("SELECT id FROM agent_tokens WHERE token_hash = ? AND revoked_at IS NULL")
            .bind(fingerprint)
            .fetch_optional(pool)
            .await
            .context("checking the enrollment token")?;
    row.map(|row| row.try_get("id")).transpose().context("token id")
}

/// Ce jeton peut-il faire entrer une machine de plus ?
pub async fn enrolment_allowed(
    pool: &SqlitePool,
    token_id: i64,
) -> Result<Result<(), EnrolmentDenied>> {
    let row = sqlx::query(
        "SELECT max_uses, uses,
                (expires_at IS NOT NULL
                 AND expires_at <= strftime('%Y-%m-%dT%H:%M:%SZ', 'now')) AS expired
         FROM agent_tokens WHERE id = ?",
    )
    .bind(token_id)
    .fetch_optional(pool)
    .await
    .context("reading the token limits")?;
    let Some(row) = row else {
        return Ok(Err(EnrolmentDenied::Expired));
    };
    if row.try_get::<i64, _>("expired")? != 0 {
        return Ok(Err(EnrolmentDenied::Expired));
    }
    let max: Option<i64> = row.try_get("max_uses")?;
    let used: i64 = row.try_get("uses")?;
    match max {
        Some(max) if used >= max => Ok(Err(EnrolmentDenied::Exhausted)),
        _ => Ok(Ok(())),
    }
}

/// Décompte un enrôlement servi par ce jeton.
pub(super) async fn consume_enrolment(pool: &SqlitePool, token_id: i64) -> Result<()> {
    sqlx::query("UPDATE agent_tokens SET uses = uses + 1 WHERE id = ?")
        .bind(token_id)
        .execute(pool)
        .await
        .context("counting the enrollment")?;
    Ok(())
}

pub async fn touch_token(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE agent_tokens SET last_used_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await
    .context("updating the token timestamp")?;
    Ok(())
}
