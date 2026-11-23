//! Lecture des valeurs de configuration : listes, drapeaux, dépôts de sauvegarde.

use super::*;

/// Les dépôts restic puis Borg, validés : un emplacement chacun, des noms
/// uniques par outil, et aucun secret dans le nom par défaut.
pub(super) fn backup_repo_list(
    restic: Option<Vec<RepoFile>>,
    borg: Option<Vec<RepoFile>>,
) -> Result<Vec<RepoConfig>> {
    let clean =
        |value: Option<String>| value.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let mut repos: Vec<RepoConfig> = Vec::new();
    let entries = restic
        .unwrap_or_default()
        .into_iter()
        .map(|entry| (Tool::Restic, entry))
        .chain(borg.unwrap_or_default().into_iter().map(|entry| (Tool::Borg, entry)));
    for (tool, entry) in entries {
        let key = format!("{}_repos", tool.as_str());
        let Some(repository) = clean(entry.repository) else {
            bail!("{key}: every entry needs a 'repository'");
        };
        let name = clean(entry.name).unwrap_or_else(|| without_credentials(&repository));
        if repos.iter().any(|r| r.tool == tool && r.name == name) {
            bail!("{key}: two repositories are named '{name}'");
        }
        repos.push(RepoConfig {
            tool,
            name,
            repository,
            password_file: clean(entry.password_file).map(PathBuf::from),
            password_command: clean(entry.password_command),
            env: entry.env,
            args: entry.args,
            host: clean(entry.host),
            glob_archives: clean(entry.glob_archives),
        });
    }
    if repos.len() > backup_repos::MAX_REPOS {
        bail!("at most {} restic/Borg repositories can be watched", backup_repos::MAX_REPOS);
    }
    Ok(repos)
}

/// L'emplacement d'un dépôt, sans les identifiants qu'une URL peut porter
/// (`rest:https://user:secret@host/`) : c'est lui qui sert d'étiquette quand
/// aucun nom n'est donné, et une étiquette part au serveur.
pub(super) fn without_credentials(repository: &str) -> String {
    let Some(scheme_end) = repository.find("://") else { return repository.to_string() };
    let authority_start = scheme_end + 3;
    let rest = &repository[authority_start..];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{}{}", &repository[..authority_start], &rest[at + 1..]),
        None => repository.to_string(),
    }
}

/// La liste `modules`, validée : `None` sans liste (tout se détecte seul),
/// l'ensemble des modules nommés sinon. `none` seul vaut liste vide — la
/// collecte système de base, rien d'autre. Un nom inconnu est refusé au
/// démarrage plutôt qu'ignoré : une faute de frappe couperait un module en
/// silence.
pub(super) fn parse_modules(
    from_env: Option<Vec<String>>,
    from_file: Option<Vec<String>>,
) -> Result<Option<BTreeSet<String>>> {
    // Une variable vide vaut absence : un `${X}` non défini dans un fichier
    // compose ne doit pas couper tous les modules.
    let Some(items) = from_env.filter(|items| !items.is_empty()).or(from_file) else {
        return Ok(None);
    };
    let mut modules = BTreeSet::new();
    for item in items {
        let name = item.trim().to_ascii_lowercase().replace('-', "_");
        if name.is_empty() || name == "none" {
            continue;
        }
        if !MODULES.contains(&name.as_str()) {
            bail!("modules: unknown module '{item}' (known: {}, or none)", MODULES.join(", "));
        }
        modules.insert(name);
    }
    Ok(Some(modules))
}

pub(super) fn split_list(raw: &str) -> Vec<String> {
    raw.split(',').map(str::trim).filter(|item| !item.is_empty()).map(str::to_string).collect()
}

/// Nom d'une commande externe : celui de la configuration, ou celui par défaut.
/// Une valeur vide ne désigne rien et vaut donc absence.
pub(super) fn command_name(configured: Option<String>, default: &str) -> String {
    configured
        .map(|bin| bin.trim().to_string())
        .filter(|bin| !bin.is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Période d'une lecture de fond, avec son plancher.
///
/// Le plancher n'est pas un détail de confort : interroger `smartctl` toutes les
/// secondes réveillerait des disques en veille à longueur de journée.
pub(super) fn read_interval(
    env: &EnvSource,
    variable: &str,
    from_file: Option<u64>,
    default: u64,
    minimum: u64,
    what: &str,
) -> Result<u64> {
    let seconds = match env.get(variable) {
        Some(raw) => raw
            .trim()
            .parse::<u64>()
            .with_context(|| format!("{variable}: invalid value '{raw}'"))?,
        None => from_file.unwrap_or(default),
    };
    if seconds < minimum {
        bail!("{what} must be at least {minimum} seconds");
    }
    Ok(seconds)
}

/// Réglages de la passerelle du relais : l'environnement prime sur le fichier.
pub(super) fn gateway_config(
    env: &EnvSource,
    listen: Option<String>,
    max_connections: Option<usize>,
    rate_kib: Option<u32>,
) -> Result<crate::gateway::GatewayConfig> {
    use crate::gateway::{
        DEFAULT_MAX_CONNECTIONS, DEFAULT_RATE_KIB, GatewayConfig, MAX_CONNECTIONS,
    };

    let listen = match env
        .get("DUMBMONIT_AGENT_GATEWAY_LISTEN")
        .or(listen)
        .map(|raw| raw.trim().to_string())
        .filter(|raw| !raw.is_empty())
    {
        Some(raw) => Some(raw.parse::<std::net::SocketAddr>().with_context(|| {
            format!("gateway_listen: invalid address '{raw}' (expected for example 0.0.0.0:8443)")
        })?),
        None => None,
    };
    let max_connections = match env.get("DUMBMONIT_AGENT_GATEWAY_MAX_CONNECTIONS") {
        Some(raw) => raw.trim().parse::<usize>().with_context(|| {
            format!("DUMBMONIT_AGENT_GATEWAY_MAX_CONNECTIONS: invalid value '{raw}'")
        })?,
        None => max_connections.unwrap_or(DEFAULT_MAX_CONNECTIONS),
    };
    let rate_kib = match env.get("DUMBMONIT_AGENT_GATEWAY_RATE_KIB") {
        Some(raw) => raw
            .trim()
            .parse::<u32>()
            .with_context(|| format!("DUMBMONIT_AGENT_GATEWAY_RATE_KIB: invalid value '{raw}'"))?,
        None => rate_kib.unwrap_or(DEFAULT_RATE_KIB),
    };
    if max_connections == 0 {
        bail!("gateway_max_connections must be at least 1");
    }
    if max_connections > MAX_CONNECTIONS {
        bail!("gateway_max_connections must be at most {MAX_CONNECTIONS}");
    }
    if rate_kib == 0 {
        bail!("gateway_rate_kib must be at least 1");
    }
    Ok(GatewayConfig { listen, max_connections, rate_kib })
}

pub(super) fn parse_tags(raw: &str) -> Result<BTreeMap<String, String>> {
    let mut tags = BTreeMap::new();
    for pair in raw.split(',').map(str::trim).filter(|pair| !pair.is_empty()) {
        let (key, value) = pair
            .split_once('=')
            .with_context(|| format!("invalid tag '{pair}', expected 'key=value'"))?;
        let key = key.trim();
        if key.is_empty() {
            bail!("tag without a name in '{raw}'");
        }
        tags.insert(key.to_string(), value.trim().to_string());
    }
    Ok(tags)
}

pub(super) fn parse_bool(raw: &str) -> Result<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "oui" | "yes" | "on" => Ok(true),
        "0" | "false" | "non" | "no" | "off" => Ok(false),
        other => bail!("invalid boolean value '{other}'"),
    }
}

pub(super) fn parse_level(raw: &str) -> Result<tracing::Level> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "trace" => Ok(tracing::Level::TRACE),
        "debug" => Ok(tracing::Level::DEBUG),
        "info" => Ok(tracing::Level::INFO),
        "warn" | "warning" => Ok(tracing::Level::WARN),
        "error" => Ok(tracing::Level::ERROR),
        other => bail!("unknown log level '{other}'"),
    }
}

/// `http://` vers la boucle locale seulement (serveur de test, tunnel SSH) :
/// rien n'y traverse le réseau.
pub(super) fn is_loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else { return false };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = match authority.strip_prefix('[') {
        Some(inner) => inner.split(']').next().unwrap_or_default(),
        None => authority.split(':').next().unwrap_or_default(),
    };
    host.eq_ignore_ascii_case("localhost")
        || host.parse::<std::net::IpAddr>().is_ok_and(|ip| ip.is_loopback())
}
