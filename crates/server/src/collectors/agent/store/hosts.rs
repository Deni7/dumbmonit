//! Lecture des machines enregistrées, telles que leur agent les décrit.

use super::*;

/// La machine telle que son agent la décrit, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub hostname: String,
    pub os: String,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub agent_version: String,
    /// `None` : l'agent n'a rien déclaré (binaire antérieur au canal de
    /// commandes). Il ne viendra pas chercher de commande, pas plus qu'avec
    /// `Some(false)`.
    pub commands_enabled: Option<bool>,
    /// Vrai si l'agent a déclaré relayer des sondes (`relay: true`).
    pub relay: bool,
    /// Site déclaré par l'agent, s'il en a un.
    pub site: Option<String>,
    /// Mode relais demandé depuis l'interface (ou par le jeton d'enrôlement).
    pub relay_wanted: bool,
    /// Site demandé depuis l'interface.
    pub relay_wanted_site: Option<String>,
    /// Vrai si l'agent sait suivre ce réglage ; faux : binaire trop ancien.
    pub relay_control: bool,
    /// Vrai si `relay: true` est posé dans sa configuration locale, qui prime.
    pub relay_local: bool,
    /// Port de passerelle demandé depuis l'interface (`None` : fermée).
    pub relay_gateway_wanted: Option<u16>,
    /// Port sur lequel la passerelle écoute, selon le dernier lot du relais.
    pub relay_gateway_port: Option<u16>,
    /// Adresse par laquelle les machines du site joignent le relais.
    pub relay_gateway_host: Option<String>,
    /// Dernier essai de sa passerelle par le relais.
    pub relay_gateway_check: Option<GatewayCheck>,
    /// Passerelle telle que le relais l'a vraiment ouverte (adresse liée, ou
    /// cause de l'échec). `None` : rien de demandé, ou relais antérieur.
    pub relay_gateway_state: Option<GatewayState>,
    /// Serveur en service, quand l'agent en connaît plusieurs (`server_urls`).
    pub server_link: Option<ServerLink>,
    /// Vrai si son dernier lot chiffré est arrivé par la WebSocket de
    /// l'interface (Cloudflare Tunnel, mandataire) plutôt qu'en direct.
    pub via_websocket: bool,
    pub last_seen_at: Option<String>,
    /// Vrai si la machine est liée à un secret connu d'elle seule.
    pub bound: bool,
    /// Moment de la liaison, s'il y en a une.
    pub bound_at: Option<String>,
    /// Vrai si le binaire installé sait recevoir un secret de liaison. Faux et
    /// non lié : l'agent est antérieur à la liaison et doit être réinstallé.
    pub binding_supported: bool,
    /// Fin de la fenêtre de reliaison ouverte à la main, si elle court encore.
    pub rebind_until: Option<String>,
}

impl HostInfo {
    /// Vrai seulement si l'agent a dit qu'il exécute les commandes : dans le
    /// doute, l'interface ne propose pas une action qui n'aboutirait jamais.
    pub fn commands_supported(&self) -> bool {
        self.commands_enabled == Some(true)
    }

    /// Ce que l'interface affiche sous « Binding ».
    ///
    /// Deux états : liée, ou refusée jusqu'à son réenrôlement (fenêtre de
    /// reliaison, puis agent à jour s'il est antérieur à la liaison).
    pub fn binding_state(&self) -> &'static str {
        if self.bound { "bound" } else { "unbound" }
    }
}

/// Description de la machine rattachée à une cible, si un agent s'y est présenté.
pub async fn host(pool: &SqlitePool, target_id: TargetId) -> Result<Option<HostInfo>> {
    let row = sqlx::query(
        "SELECT hostname, os, os_version, arch, agent_version, commands_enabled, relay, site,
                relay_wanted, relay_wanted_site, relay_control, relay_local, last_seen_at, secret_hash, bound_at, binding_supported, rebind_until,
                relay_gateway_wanted, relay_gateway_port, relay_gateway_host, relay_gateway_check,
                relay_gateway_state, server_link, tls_websocket
         FROM agent_hosts WHERE target_id = ?",
    )
    .bind(target_id)
    .fetch_optional(pool)
    .await
    .context("reading the machine")?;
    row.map(|row| row_to_host(&row)).transpose()
}

/// Toutes les machines à agent, avec le nom de leur cible : c'est ce que le
/// formulaire d'équipement propose comme relais possibles.
pub async fn list_hosts(pool: &SqlitePool) -> Result<Vec<(TargetId, String, HostInfo)>> {
    let rows = sqlx::query(
        "SELECT h.target_id, t.name, h.hostname, h.os, h.os_version, h.arch, h.agent_version,
                h.commands_enabled, h.relay, h.site, h.relay_wanted, h.relay_wanted_site,
                h.relay_control, h.relay_local, h.last_seen_at, h.secret_hash, h.bound_at,
                h.binding_supported, h.rebind_until, h.relay_gateway_wanted, h.relay_gateway_port,
                h.relay_gateway_host, h.relay_gateway_check, h.relay_gateway_state, h.server_link, h.tls_websocket
         FROM agent_hosts h JOIN targets t ON t.id = h.target_id
         ORDER BY t.name",
    )
    .fetch_all(pool)
    .await
    .context("listing the machines")?;
    rows.iter()
        .map(|row| Ok((row.try_get("target_id")?, row.try_get("name")?, row_to_host(row)?)))
        .collect()
}

pub(super) fn row_to_host(row: &sqlx::sqlite::SqliteRow) -> Result<HostInfo> {
    let relay_wanted = row.try_get::<i64, _>("relay_wanted")? != 0;
    let relay_local = row.try_get::<i64, _>("relay_local")? != 0;
    let gateway_check = row
        .try_get::<Option<String>, _>("relay_gateway_check")?
        .and_then(|raw| serde_json::from_str::<GatewayCheck>(&raw).ok());
    let gateway_state = row
        .try_get::<Option<String>, _>("relay_gateway_state")?
        .and_then(|raw| serde_json::from_str::<GatewayState>(&raw).ok());
    let server_link = row
        .try_get::<Option<String>, _>("server_link")?
        .and_then(|raw| serde_json::from_str::<ServerLink>(&raw).ok());
    Ok(HostInfo {
        hostname: row.try_get("hostname")?,
        os: row.try_get("os")?,
        os_version: row.try_get("os_version")?,
        arch: row.try_get("arch")?,
        agent_version: row.try_get("agent_version")?,
        commands_enabled: row.try_get::<Option<i64>, _>("commands_enabled")?.map(|v| v != 0),
        relay: row.try_get::<i64, _>("relay")? != 0,
        site: row.try_get("site")?,
        relay_wanted,
        relay_wanted_site: row.try_get("relay_wanted_site")?,
        relay_control: row.try_get::<i64, _>("relay_control")? != 0,
        relay_local,
        relay_gateway_wanted: gateway_wanted_of(
            row.try_get("relay_gateway_wanted")?,
            relay_wanted || relay_local,
        ),
        relay_gateway_port: port_of(row.try_get("relay_gateway_port")?),
        relay_gateway_host: row.try_get("relay_gateway_host")?,
        relay_gateway_check: gateway_check,
        relay_gateway_state: gateway_state,
        server_link,
        via_websocket: row.try_get::<i64, _>("tls_websocket")? != 0,
        last_seen_at: row.try_get("last_seen_at")?,
        bound: row.try_get::<Option<String>, _>("secret_hash")?.is_some(),
        bound_at: row.try_get("bound_at")?,
        binding_supported: row.try_get::<i64, _>("binding_supported")? != 0,
        rebind_until: row.try_get("rebind_until")?,
    })
}

/// Instant du dernier lot reçu, en millisecondes depuis l'époque Unix.
pub async fn last_seen_ms(pool: &SqlitePool, target_id: TargetId) -> Result<Option<i64>> {
    let row = sqlx::query("SELECT last_seen_ms FROM agent_hosts WHERE target_id = ?")
        .bind(target_id)
        .fetch_optional(pool)
        .await
        .context("reading the last receipt")?;
    match row {
        Some(row) => row.try_get("last_seen_ms").context("unreadable last receipt"),
        None => Ok(None),
    }
}

/// Dernière activité du planificateur avant `before_ms` : la date la plus
/// récente d'interrogation d'une cible, quelle qu'elle soit. C'est à peu près
/// l'heure à laquelle le serveur s'est arrêté avant son démarrage en cours —
/// indépendante des agents eux-mêmes, dont un seul muet fausserait l'estimation.
pub async fn last_activity_before(pool: &SqlitePool, before_ms: i64) -> Result<Option<i64>> {
    const FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";
    let Some(before) = chrono::DateTime::from_timestamp_millis(before_ms) else {
        return Ok(None);
    };
    let latest: Option<String> =
        sqlx::query_scalar("SELECT MAX(last_probe_at) FROM targets WHERE last_probe_at < ?")
            .bind(before.format(FORMAT).to_string())
            .fetch_one(pool)
            .await
            .context("reading the last activity before startup")?;
    Ok(latest
        .and_then(|text| chrono::NaiveDateTime::parse_from_str(&text, FORMAT).ok())
        .map(|at| at.and_utc().timestamp_millis()))
}
