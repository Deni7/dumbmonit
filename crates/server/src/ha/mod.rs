//! Serveur de secours : un serveur actif, un second en attente.
//!
//! Rien ne change sans `DUMBMONIT_HA_DIR`. Avec lui, deux serveurs partagent
//! un répertoire (NFS, SMB, volume répliqué…) et un VictoriaMetrics externe
//! (`DUMBMONIT_VM_URL`) ; un seul est actif à la fois, toujours.
//!
//! - **Le bail** (`lease.json` dans le répertoire partagé) dit qui est actif.
//!   L'actif le renouvelle toutes les dix secondes ([`Timing::beat`]). Il le
//!   relit avant chaque renouvellement : un autre nom, un autre processus ou
//!   une autre époque, et il s'arrête aussitôt (code de sortie
//!   [`EXIT_FENCED`]) — le redémarrage du conteneur le ramène en attente. En
//!   bascule automatique, l'actif qui ne parvient plus à renouveler son bail
//!   pendant la moitié du délai de reprise s'arrête aussi : le secours ne
//!   prend la main qu'après le délai entier.
//! - **La réplique** : l'actif écrit un instantané cohérent de sa base
//!   (`VACUUM INTO`, comme les sauvegardes locales) dans le répertoire partagé
//!   toutes les [`HaConfig::snapshot_every`] (30 s par défaut) quand elle a
//!   changé, sous son bail seulement. Le serveur en attente le recopie en
//!   continu, vérifie son empreinte, son intégrité et qu'il se déchiffre avec
//!   son propre secret d'instance.
//! - **En attente**, le serveur n'ouvre ni sa base, ni VictoriaMetrics, ni
//!   aucun port, et ne sonde rien : il ne peut rien écrire. Il devient actif
//!   sur `dumbmonit ha promote` (refusé tant que l'actif renouvelle son bail,
//!   sauf `--force`), ou de lui-même après `DUMBMONIT_HA_TAKEOVER_MINUTES`
//!   sans battement, ou tout de suite quand le serveur de rôle `primary`
//!   retrouve un bail rendu par un arrêt propre. Il prend alors le bail,
//!   attend ([`Timing::settle`]) puis le relit — deux prétendants, un seul
//!   gagnant —, installe la dernière réplique à la place de sa base et
//!   démarre normalement.
//!
//! La clé de l'instance (`/data/secret.key` ou `DUMBMONIT_SECRET`) doit être
//! la même sur les deux serveurs. L'autorité des agents, le certificat HTTPS
//! autosigné et tous les identifiants sont dans la base, chiffrés par elle :
//! ils suivent la réplique, et les agents se reconnectent au secours sans
//! réenrôlement (`server_urls` dans leur configuration).

pub mod cli;

mod active;
mod shared;
mod standby;

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

pub use active::Active;
use shared::Shared;

use crate::config::Config;

/// Code de sortie du serveur qui a perdu son bail. Le conteneur redémarre
/// (`restart: unless-stopped`) et le serveur repart en attente.
pub const EXIT_FENCED: i32 = 75;

/// Délai de reprise automatique minimal : en deçà, une coupure du stockage
/// partagé ou un redémarrage lent ferait basculer pour rien.
pub const MIN_TAKEOVER: Duration = Duration::from_secs(120);

/// Période par défaut de la réplication de la base, en secondes.
pub const DEFAULT_SNAPSHOT_SECS: u64 = 30;

/// Répertoire de travail local, sous `/data`.
const WORK_DIR: &str = "ha";
/// État du serveur, lu par `dumbmonit ha status` et par la sonde de santé.
const STATE_FILE: &str = "state.json";
/// Demande de promotion déposée par `dumbmonit ha promote`.
const PROMOTE_FILE: &str = "promote.json";
/// Dernière réplique vérifiée, prête à servir.
const REPLICA_FILE: &str = "replica.db";

/// Rôle voulu de ce serveur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Prend le bail au premier démarrage, et le reprend dès qu'il est rendu.
    Primary,
    /// Attend : ne devient actif que sur promotion.
    Standby,
}

/// Cadence du bail. Fixe en service ; les tests la resserrent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    /// Renouvellement du bail par l'actif, et relève du serveur en attente.
    pub beat: Duration,
    /// Un bail qui n'a pas bougé depuis plus longtemps n'est plus tenu par un
    /// serveur vivant (trois battements manqués).
    pub live: Duration,
    /// Attente entre la prise du bail et sa relecture.
    pub settle: Duration,
}

impl Timing {
    pub const SERVICE: Self = Self {
        beat: Duration::from_secs(10),
        live: Duration::from_secs(30),
        settle: Duration::from_secs(15),
    };
}

/// Réglages de la haute disponibilité (`DUMBMONIT_HA_*`).
#[derive(Clone, PartialEq, Eq)]
pub struct HaConfig {
    /// Répertoire partagé par les deux serveurs (`DUMBMONIT_HA_DIR`).
    pub dir: PathBuf,
    /// Nom de ce serveur, différent sur chacun (`DUMBMONIT_HA_NODE`).
    pub node: String,
    pub role: Role,
    /// Reprise automatique après ce délai sans battement de l'actif
    /// (`DUMBMONIT_HA_TAKEOVER_MINUTES`). `None` : promotion manuelle seule.
    pub takeover_after: Option<Duration>,
    /// Période de la réplication de la base (`DUMBMONIT_HA_SNAPSHOT_SECS`).
    pub snapshot_every: Duration,
    pub timing: Timing,
}

impl fmt::Debug for HaConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HaConfig")
            .field("dir", &self.dir)
            .field("node", &self.node)
            .field("role", &self.role)
            .field("takeover_after", &self.takeover_after)
            .field("snapshot_every", &self.snapshot_every)
            .finish_non_exhaustive()
    }
}

impl HaConfig {
    /// Lit l'environnement ; `None` sans `DUMBMONIT_HA_DIR`.
    pub fn from_env() -> Result<Option<Self>> {
        Self::parse(crate::config::env_var)
    }

    fn parse(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>> {
        let get = |key: &str| get(key).map(|value| value.trim().to_string());
        let get = |key: &str| get(key).filter(|value| !value.is_empty());
        let Some(dir) = get("DUMBMONIT_HA_DIR") else {
            return Ok(None);
        };
        let dir = PathBuf::from(dir);
        if !dir.is_absolute() {
            bail!("DUMBMONIT_HA_DIR must be an absolute path (got '{}')", dir.display());
        }
        let node = get("DUMBMONIT_HA_NODE").context(
            "DUMBMONIT_HA_NODE is required with DUMBMONIT_HA_DIR: a short name for this server, \
             different on each (for example a and b)",
        )?;
        if !is_node_name(&node) {
            bail!(
                "DUMBMONIT_HA_NODE: 1 to 32 lowercase letters, digits, '-' or '_' (got '{node}')"
            );
        }
        let role = match get("DUMBMONIT_HA_ROLE").map(|raw| raw.to_ascii_lowercase()).as_deref() {
            Some("primary") => Role::Primary,
            Some("standby") => Role::Standby,
            Some(other) => bail!("DUMBMONIT_HA_ROLE: primary or standby (got '{other}')"),
            None => bail!(
                "DUMBMONIT_HA_ROLE is required with DUMBMONIT_HA_DIR: primary on the server that \
                 normally runs, standby on the other"
            ),
        };
        let takeover_after = match get("DUMBMONIT_HA_TAKEOVER_MINUTES") {
            None => None,
            Some(raw) => {
                let minutes = raw.parse::<u64>().with_context(|| {
                    format!("DUMBMONIT_HA_TAKEOVER_MINUTES: a number of minutes (got '{raw}')")
                })?;
                (minutes > 0).then(|| Duration::from_secs(minutes * 60).max(MIN_TAKEOVER))
            }
        };
        let snapshot_secs = match get("DUMBMONIT_HA_SNAPSHOT_SECS") {
            None => DEFAULT_SNAPSHOT_SECS,
            Some(raw) => raw.parse::<u64>().with_context(|| {
                format!("DUMBMONIT_HA_SNAPSHOT_SECS: a number of seconds (got '{raw}')")
            })?,
        };
        Ok(Some(Self {
            dir,
            node,
            role,
            takeover_after,
            snapshot_every: Duration::from_secs(snapshot_secs.clamp(10, 3600)),
            timing: Timing::SERVICE,
        }))
    }

    /// Temps sans renouvellement réussi au bout duquel l'actif s'arrête de
    /// lui-même : la moitié du délai de reprise, pour qu'il soit parti bien
    /// avant que le secours ne prenne la main. Sans reprise automatique,
    /// personne ne prend la main tout seul : l'actif continue.
    pub fn fence_after(&self) -> Option<Duration> {
        self.takeover_after.map(|after| after / 2)
    }
}

/// Nom de serveur : court, sans rien qui sorte d'un nom de fichier.
fn is_node_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
}

/// Le bail, tel que le répertoire partagé le garde (`lease.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    /// Serveur qui le tient (`DUMBMONIT_HA_NODE`).
    pub holder: String,
    /// Processus qui le tient : deux serveurs réglés sous le même nom se
    /// trahissent ainsi.
    pub instance: String,
    /// Augmente à chaque prise du bail.
    pub epoch: u64,
    /// Augmente à chaque renouvellement : le serveur en attente le regarde
    /// bouger, sans se fier à l'horloge de l'autre.
    pub beat: u64,
    /// Rendu par un arrêt propre de son détenteur.
    #[serde(default)]
    pub released: bool,
    /// Dernier renouvellement selon l'horloge du détenteur (information).
    pub renewed_at_ms: i64,
    /// Version du serveur qui le tient.
    #[serde(default)]
    pub version: String,
}

impl Lease {
    fn new(node: &str, instance: &str, epoch: u64) -> Self {
        Self {
            holder: node.to_string(),
            instance: instance.to_string(),
            epoch,
            beat: 0,
            released: false,
            renewed_at_ms: now_ms(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Ce qui change quand le bail est tenu : de quoi voir qu'il bouge.
    fn key(&self) -> (String, String, u64, u64, bool) {
        (self.holder.clone(), self.instance.clone(), self.epoch, self.beat, self.released)
    }
}

/// Dernier instantané de la base publié par l'actif (`snapshot.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Nom du fichier dans le répertoire partagé.
    pub file: String,
    /// Serveur qui l'a écrit, et sous quelle époque du bail.
    pub node: String,
    pub epoch: u64,
    pub taken_at_ms: i64,
    pub size: u64,
    /// Empreinte SHA-256 du fichier, en hexadécimal.
    pub sha256: String,
    /// Version du serveur qui l'a écrit : les deux serveurs doivent en avoir
    /// la même, une base plus récente que le binaire ne démarre pas.
    pub version: String,
}

/// Demande de promotion (`dumbmonit ha promote`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromoteRequest {
    /// Prendre la main même si l'actif renouvelle encore son bail : il
    /// s'arrêtera de lui-même au renouvellement suivant.
    #[serde(default)]
    pub force: bool,
    pub requested_at_ms: i64,
}

/// État de ce serveur, écrit à chaque battement dans `/data/ha/state.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    pub node: String,
    /// `standby`, `promoting` ou `active`.
    pub role: String,
    pub updated_at_ms: i64,
    /// Époque du bail tenu (actif).
    #[serde(default)]
    pub epoch: Option<u64>,
    /// Détenteur du bail vu par le serveur en attente, et s'il le renouvelle.
    #[serde(default)]
    pub holder: Option<String>,
    #[serde(default)]
    pub holder_alive: Option<bool>,
    /// Dernière réplique vérifiée (serveur en attente) ou publiée (actif).
    #[serde(default)]
    pub replica_taken_at_ms: Option<i64>,
    #[serde(default)]
    pub replica_from: Option<String>,
    /// Dernier problème de réplication, en anglais.
    #[serde(default)]
    pub replica_error: Option<String>,
    /// Demande de promotion en attente, ou la raison de son refus.
    #[serde(default)]
    pub promotion: Option<String>,
    /// Instant de la demande dont `promotion` parle.
    #[serde(default)]
    pub promotion_for_ms: Option<i64>,
    #[serde(default)]
    pub takeover_after_s: Option<u64>,
}

/// Que faire au démarrage, d'après le bail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Start {
    /// Le bail est à ce serveur (redémarrage) : sa base locale est la plus
    /// fraîche.
    Resume,
    /// Pas de bail du tout, rôle `primary` : premier démarrage de la paire.
    Bootstrap,
    /// Bail rendu par l'autre, rôle `primary` : reprise, depuis la réplique.
    Reclaim,
    /// Attendre, en serveur passif.
    Wait,
}

fn decide_start(node: &str, role: Role, lease: Option<&Lease>) -> Start {
    match lease {
        None if role == Role::Primary => Start::Bootstrap,
        None => Start::Wait,
        Some(lease) if lease.holder == node => Start::Resume,
        Some(lease) if lease.released && role == Role::Primary => Start::Reclaim,
        Some(_) => Start::Wait,
    }
}

/// Démarrage en haute disponibilité : rend la main quand ce serveur tient le
/// bail et que sa base est prête — tout de suite pour l'actif, après la
/// promotion pour le serveur en attente. `None` sans `DUMBMONIT_HA_DIR`.
///
/// À appeler avant d'ouvrir la base : en attente, rien d'autre ne tourne.
pub async fn start(config: &Config, secret: &str) -> Result<Option<Active>> {
    let Some(ha) = config.ha.clone() else { return Ok(None) };
    if config.demo {
        bail!("DUMBMONIT_HA_DIR cannot be used with DUMBMONIT_DEMO");
    }
    if config.vm_embedded() {
        warn!(
            "high availability with the embedded VictoriaMetrics: each server keeps its own \
             metrics, the history does not follow a switch-over (set DUMBMONIT_VM_URL on both)"
        );
    }
    let work = config.data_dir.join(WORK_DIR);
    tokio::fs::create_dir_all(&work)
        .await
        .with_context(|| format!("creating {}", work.display()))?;
    let shared = Shared::open(&ha).await?;
    let database = config.database_path();
    let instance = crate::crypto::generate_secret();
    info!(
        node = ha.node,
        role = ?ha.role,
        dir = %ha.dir.display(),
        takeover_min = ha.takeover_after.map(|after| after.as_secs() / 60),
        "high availability on"
    );
    loop {
        let lease = match shared.read_lease().await {
            Ok(lease) => lease,
            Err(error) => {
                // Stockage partagé injoignable : rien n'est sûr, on attend.
                warn!(error = %format!("{error:#}"), "the shared directory is unreadable, waiting");
                tokio::time::sleep(ha.timing.beat).await;
                continue;
            }
        };
        // Base à remplacer par la réplique, et dernier bail vu (son époque).
        let (restore, previous) = match decide_start(&ha.node, ha.role, lease.as_ref()) {
            Start::Resume => {
                if let Some(lease) = &lease
                    && !lease.released
                    && !quiet(&shared, lease, &ha).await?
                {
                    // Le bail a bougé sans nous : un autre l'a pris pendant
                    // l'attente, on repart du début (en attente).
                    continue;
                }
                (false, lease)
            }
            Start::Bootstrap => (!tokio::fs::try_exists(&database).await.unwrap_or(false), lease),
            Start::Reclaim => (true, lease),
            Start::Wait => (true, standby::wait(&ha, &shared, &work, secret).await?),
        };
        standby::write_status(&work, &standby::promoting_status(&ha)).await;
        let Some(lease) = acquire(&ha, &shared, previous.as_ref(), &instance).await? else {
            warn!("another server took the lease at the same time, waiting again");
            continue;
        };
        if restore {
            let installed = standby::install_latest(&shared, &work, &database, secret).await;
            if let Err(error) = installed {
                // Bail rendu : l'autre serveur (ou celui-ci, au redémarrage)
                // pourra le reprendre.
                let released = Lease { released: true, ..lease };
                let _ = shared.write_lease(&released).await;
                return Err(error.context("installing the latest replica of the database"));
            }
        }
        info!(epoch = lease.epoch, restored = restore, "this server is now the active one");
        return Ok(Some(Active::start(ha, shared, lease, work, database)));
    }
}

/// Le bail à notre nom n'est-il plus renouvelé par personne ? Deux
/// battements d'observation : un autre processus sous le même nom serait une
/// erreur de configuration, refusée.
async fn quiet(shared: &Shared, seen: &Lease, ha: &HaConfig) -> Result<bool> {
    tokio::time::sleep(ha.timing.beat * 2).await;
    let now = shared.read_lease().await?;
    match now {
        Some(lease) if lease == *seen => Ok(true),
        Some(lease) if lease.holder == ha.node => bail!(
            "another server renews the lease under the name '{}': give each server its own \
             DUMBMONIT_HA_NODE",
            ha.node
        ),
        _ => Ok(false),
    }
}

/// Prend le bail, attend, le relit : `None` si un autre l'a pris entre-temps.
async fn acquire(
    ha: &HaConfig,
    shared: &Shared,
    previous: Option<&Lease>,
    instance: &str,
) -> Result<Option<Lease>> {
    let epoch = previous.map_or(0, |lease| lease.epoch) + 1;
    let lease = Lease::new(&ha.node, instance, epoch);
    shared.write_lease(&lease).await.context("writing the lease")?;
    tokio::time::sleep(ha.timing.settle).await;
    let read = shared.read_lease().await.context("reading the lease back")?;
    Ok((read.as_ref() == Some(&lease)).then_some(lease))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Écrit un fichier d'un coup : fichier temporaire, synchronisé, puis
/// renommé — un lecteur voit l'ancien ou le nouveau, jamais un morceau.
async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt as _;
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or("file");
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .with_context(|| format!("creating {}", tmp.display()))?;
    file.write_all(bytes).await.with_context(|| format!("writing {}", tmp.display()))?;
    file.sync_all().await.with_context(|| format!("syncing {}", tmp.display()))?;
    drop(file);
    tokio::fs::rename(&tmp, path)
        .await
        .with_context(|| format!("renaming {} to {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Secret d'instance des tests.
#[cfg(test)]
const TEST_SECRET: &str = "secret-de-test-suffisamment-long";

/// Réglages de test : cadence resserrée, promotion manuelle.
#[cfg(test)]
fn test_ha(dir: &Path, node: &str, role: Role) -> HaConfig {
    HaConfig {
        dir: dir.to_path_buf(),
        node: node.into(),
        role,
        takeover_after: None,
        snapshot_every: Duration::from_secs(10),
        timing: Timing {
            beat: Duration::from_millis(20),
            live: Duration::from_millis(120),
            settle: Duration::from_millis(30),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn parse(pairs: &[(&str, &str)]) -> Result<Option<HaConfig>> {
        let map: HashMap<String, String> =
            pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        HaConfig::parse(|key| map.get(key).cloned())
    }

    #[test]
    fn nothing_changes_without_the_shared_directory() {
        assert_eq!(parse(&[]).unwrap(), None);
        assert_eq!(parse(&[("DUMBMONIT_HA_NODE", "a")]).unwrap(), None);
        assert_eq!(parse(&[("DUMBMONIT_HA_DIR", "  ")]).unwrap(), None);
    }

    #[test]
    fn the_shared_directory_needs_a_name_and_a_role() {
        let dir = ("DUMBMONIT_HA_DIR", "/ha");
        let primary = ("DUMBMONIT_HA_ROLE", "primary");
        assert!(parse(&[dir]).is_err(), "nom manquant");
        assert!(parse(&[dir, ("DUMBMONIT_HA_NODE", "a")]).is_err(), "rôle manquant");
        assert!(parse(&[dir, ("DUMBMONIT_HA_NODE", "A/b"), primary]).is_err());
        let unknown_role = ("DUMBMONIT_HA_ROLE", "leader");
        assert!(parse(&[dir, ("DUMBMONIT_HA_NODE", "a"), unknown_role]).is_err());
        let relative = ("DUMBMONIT_HA_DIR", "ha");
        assert!(parse(&[relative, ("DUMBMONIT_HA_NODE", "a"), primary]).is_err());

        let standby = ("DUMBMONIT_HA_ROLE", "Standby");
        let config = parse(&[dir, ("DUMBMONIT_HA_NODE", "b"), standby]).unwrap().expect("réglages");
        assert_eq!(config.role, Role::Standby);
        assert_eq!(config.takeover_after, None, "promotion manuelle par défaut");
        assert_eq!(config.fence_after(), None);
        assert_eq!(config.snapshot_every, Duration::from_secs(DEFAULT_SNAPSHOT_SECS));
    }

    #[test]
    fn the_takeover_delay_has_a_floor_and_the_active_fences_at_half_of_it() {
        let base = [
            ("DUMBMONIT_HA_DIR", "/ha"),
            ("DUMBMONIT_HA_NODE", "a"),
            ("DUMBMONIT_HA_ROLE", "primary"),
        ];
        let with = |extra: (&'static str, &'static str)| {
            let mut pairs = base.to_vec();
            pairs.push(extra);
            parse(&pairs)
        };
        let config = with(("DUMBMONIT_HA_TAKEOVER_MINUTES", "5")).unwrap().unwrap();
        assert_eq!(config.takeover_after, Some(Duration::from_secs(300)));
        assert_eq!(config.fence_after(), Some(Duration::from_secs(150)));
        let config = with(("DUMBMONIT_HA_TAKEOVER_MINUTES", "1")).unwrap().unwrap();
        assert_eq!(config.takeover_after, Some(MIN_TAKEOVER));
        let config = with(("DUMBMONIT_HA_TAKEOVER_MINUTES", "0")).unwrap().unwrap();
        assert_eq!(config.takeover_after, None);
        assert!(with(("DUMBMONIT_HA_TAKEOVER_MINUTES", "soon")).is_err());
        let config = with(("DUMBMONIT_HA_SNAPSHOT_SECS", "1")).unwrap().unwrap();
        assert_eq!(config.snapshot_every, Duration::from_secs(10));
    }

    fn lease(holder: &str, released: bool) -> Lease {
        Lease { released, ..Lease::new(holder, "i", 3) }
    }

    #[test]
    fn at_startup_only_the_holder_or_the_primary_role_becomes_active() {
        use Role::{Primary, Standby};
        assert_eq!(decide_start("a", Primary, None), Start::Bootstrap);
        assert_eq!(decide_start("b", Standby, None), Start::Wait);
        assert_eq!(decide_start("a", Primary, Some(&lease("a", false))), Start::Resume);
        assert_eq!(decide_start("b", Standby, Some(&lease("b", true))), Start::Resume);
        // Après une bascule, l'ancien primaire revient en attente.
        assert_eq!(decide_start("a", Primary, Some(&lease("b", false))), Start::Wait);
        assert_eq!(decide_start("a", Primary, Some(&lease("b", true))), Start::Reclaim);
        assert_eq!(decide_start("b", Standby, Some(&lease("a", true))), Start::Wait);
    }

    // --------------------------------------------- démarrages de bout en bout

    use super::active::Renewal;

    /// Le primaire « a », actif : sa base (une ligne témoin), son bail, sa
    /// première réplique.
    async fn start_primary(root: &Path) -> (Active, sqlx::SqlitePool) {
        let config = test_ha(&root.join("shared"), "a", Role::Primary);
        let shared = Shared::open(&config).await.unwrap();
        let lease = Lease::new("a", "process-a", 1);
        shared.write_lease(&lease).await.unwrap();
        let work = root.join("a");
        std::fs::create_dir_all(&work).unwrap();
        let database = work.join(crate::db::DATABASE_FILE);
        let pool = crate::db::open_with(&database, 2).await.unwrap();
        crate::db::init_cipher(&pool, TEST_SECRET).await.unwrap();
        sqlx::query("INSERT INTO settings (key, value) VALUES ('ha-test', 'from-a')")
            .execute(&pool)
            .await
            .unwrap();
        let active = Active::new(config, shared, lease, work, database);
        active.snapshot(&pool).await.unwrap().expect("réplique");
        (active, pool)
    }

    /// Réglages du serveur « b » sous `root`.
    fn server_b(root: &Path, role: Role) -> Config {
        let mut config = Config::from_env().expect("configuration");
        config.data_dir = root.join("b");
        config.victoria_url = Some("http://127.0.0.1:9".into());
        config.ha = Some(test_ha(&root.join("shared"), "b", role));
        std::fs::create_dir_all(&config.data_dir).unwrap();
        config
    }

    fn ask_promotion(config: &Config) {
        let request = PromoteRequest { force: false, requested_at_ms: now_ms() };
        let path = config.data_dir.join(WORK_DIR).join(PROMOTE_FILE);
        std::fs::write(path, serde_json::to_vec(&request).unwrap()).unwrap();
    }

    fn state_of(config: &Config) -> Status {
        let path = config.data_dir.join(WORK_DIR).join(STATE_FILE);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    /// Le primaire bat `count` fois, au rythme du test.
    async fn beat(primary: &Active, count: usize) {
        for _ in 0..count {
            assert!(matches!(primary.renew().await, Renewal::Renewed));
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_standby_stays_passive_then_takes_over_with_the_replica() {
        let root = tempfile::tempdir().unwrap();
        let (primary, _pool_a) = start_primary(root.path()).await;
        let config = server_b(root.path(), Role::Standby);
        let standby = tokio::spawn({
            let config = config.clone();
            async move { start(&config, TEST_SECRET).await }
        });

        // Le primaire renouvelle son bail : le secours attend, sans base
        // ouverte ni rien écrit hors de son état.
        beat(&primary, 15).await;
        assert!(!standby.is_finished(), "passif tant que le primaire bat");
        assert!(!config.database_path().exists(), "aucune base ouverte en attente");
        assert_eq!(state_of(&config).role, "standby");
        assert_eq!(state_of(&config).replica_from.as_deref(), Some("a"));

        // Une promotion demandée pendant qu'il bat est refusée.
        ask_promotion(&config);
        beat(&primary, 10).await;
        let refused = state_of(&config).promotion.expect("refus");
        assert!(refused.starts_with("refused"), "{refused}");
        assert!(!standby.is_finished());

        // Le primaire s'est tu : la demande suivante aboutit.
        tokio::time::sleep(Duration::from_millis(300)).await;
        ask_promotion(&config);
        let active = tokio::time::timeout(Duration::from_secs(5), standby)
            .await
            .expect("promu à temps")
            .unwrap()
            .unwrap();
        assert!(active.is_some());

        // La base installée est la réplique de « a », que le secret déchiffre.
        assert!(standby::verify(&config.database_path(), TEST_SECRET).await.is_ok());
        let pool = crate::db::open_with(&config.database_path(), 2).await.unwrap();
        let value: String =
            sqlx::query_scalar("SELECT CAST(value AS TEXT) FROM settings WHERE key = 'ha-test'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(value, "from-a");
        let lease = Shared::at(&root.path().join("shared"), "x").read_lease().await.unwrap();
        let lease = lease.expect("bail");
        assert_eq!((lease.holder.as_str(), lease.epoch, lease.released), ("b", 2, false));

        // L'ancien actif, s'il revient à lui, se sait destitué.
        assert!(matches!(primary.renew().await, Renewal::Lost(_)));
    }

    #[tokio::test]
    async fn without_heartbeat_the_standby_takes_over_after_the_delay() {
        let root = tempfile::tempdir().unwrap();
        // Un bail tenu par « a », figé : le primaire est tombé.
        let shared_dir = root.path().join("shared");
        let frozen = Lease::new("a", "process-a", 7);
        let shared = Shared::open(&test_ha(&shared_dir, "a", Role::Primary)).await.unwrap();
        shared.write_lease(&frozen).await.unwrap();
        let mut config = server_b(root.path(), Role::Standby);
        if let Some(ha) = config.ha.as_mut() {
            ha.takeover_after = Some(Duration::from_millis(250));
        }
        let started = std::time::Instant::now();
        let active = tokio::time::timeout(Duration::from_secs(5), start(&config, TEST_SECRET))
            .await
            .expect("reprise à temps")
            .unwrap();
        assert!(active.is_some());
        assert!(started.elapsed() >= Duration::from_millis(250), "pas avant le délai");
        let lease = shared.read_lease().await.unwrap().expect("bail");
        assert_eq!((lease.holder.as_str(), lease.epoch), ("b", 8));
    }

    #[tokio::test]
    async fn the_primary_role_starts_at_once_on_an_empty_directory() {
        let root = tempfile::tempdir().unwrap();
        let mut config = server_b(root.path(), Role::Primary);
        if let Some(ha) = config.ha.as_mut() {
            ha.node = "a".into();
        }
        let active = tokio::time::timeout(Duration::from_secs(5), start(&config, TEST_SECRET))
            .await
            .expect("démarrage à temps")
            .unwrap();
        assert!(active.is_some());
        let lease = Shared::at(&root.path().join("shared"), "x").read_lease().await.unwrap();
        assert_eq!(lease.map(|lease| (lease.holder, lease.epoch)), Some(("a".to_string(), 1)));
        // Un serveur sans haute disponibilité ne touche à rien.
        let mut plain = server_b(root.path(), Role::Standby);
        plain.ha = None;
        assert!(start(&plain, TEST_SECRET).await.unwrap().is_none());
    }
}
