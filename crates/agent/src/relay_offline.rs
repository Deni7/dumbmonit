//! Relais hors ligne : le planning gardé sur disque, la planification locale
//! pendant une coupure, et le tampon des résultats à renvoyer.
//!
//! En temps normal, le serveur planifie et le relais exécute. Le relais garde
//! en plus une copie du planning (`GET /api/agent/relay/plan`, rafraîchie
//! toutes les quelques minutes). Quand le serveur ne répond plus depuis
//! [`OFFLINE_AFTER`], le relais planifie seul d'après cette copie, et chaque
//! résultat part dans un tampon disque ([`Spool`]). Au retour du serveur, la
//! planification lui est rendue et le tampon se vide par lots
//! (`POST /api/agent/relay/backlog`), sans saturer le lien.
//!
//! # Le planning chiffré, et contre quoi
//!
//! Le planning contient les identifiants des équipements du site (communautés
//! SNMP, jetons d'API…) : il est chiffré (AES-256-GCM) par une clé propre au
//! relais, tirée au hasard et rangée à part (`relay-plan.key`, lisible du seul
//! compte de l'agent, comme son secret de liaison).
//!
//! - **Protège** contre la fuite du fichier du planning seul : copie du
//!   répertoire de données sans la clé, sauvegarde partielle, journal de
//!   diagnostic joint à un ticket, disque d'un ancien relais récupéré sans le
//!   fichier de clé.
//! - **Ne protège pas** contre qui lit tout le répertoire de l'agent avec ses
//!   droits (ou ceux de l'administrateur) : il y trouve la clé, mais aussi le
//!   certificat client et le secret de liaison, qui lui ouvrent de toute façon
//!   le planning auprès du serveur. Contre cela, ce sont les permissions de la
//!   machine qui protègent, pas la cryptographie.
//! - Une clé perdue ou un fichier abîmé ne bloquent rien : le planning est
//!   oublié et redemandé au serveur à la reprise suivante.
//!
//! Le relais coupé depuis l'interface oublie planning, clé et tampon.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use dumbmonit_proto::{
    BacklogResult, MAX_BACKLOG_RESULTS, PlannedProbe, ProbeOutcome, RelayBacklog, RelayPlan,
    TargetId,
};
use serde::{Deserialize, Serialize};
use tokio::time::Instant;
use tracing::{debug, info, warn};

use crate::client::{PushClient, PushError};
use crate::shutdown::Shutdown;
use crate::spool::{self, Spool};

/// Silence du serveur au-delà duquel le relais planifie seul : le délai au
/// bout duquel le serveur, de son côté, compte les sondes comme échues.
pub const OFFLINE_AFTER: Duration = Duration::from_secs(35);

/// Période de rafraîchissement du planning tant que le serveur répond, et
/// délai avant un nouvel essai après un échec.
pub const PLAN_REFRESH: Duration = Duration::from_secs(300);
pub const PLAN_RETRY: Duration = Duration::from_secs(60);

/// Période d'un équipement, au moins : une valeur absurde dans le planning ne
/// doit pas faire tourner le relais en boucle.
const MIN_INTERVAL: Duration = Duration::from_secs(10);

/// Pause entre deux lots de rattrapage : le lien d'un petit site sert aussi
/// aux sondes en cours et aux mesures fraîches.
const BACKLOG_PAUSE: Duration = Duration::from_millis(500);

/// Charge compressée au plus par lot de rattrapage : de l'ordre de 5 Mo de
/// JSON une fois décompressée, sous la limite de corps du serveur (16 Mo).
const BACKLOG_BYTES: usize = 512 * 1024;

const PLAN_FILE: &str = "relay-plan.enc";
const KEY_FILE: &str = "relay-plan.key";

/// Marque et version du fichier chiffré, authentifiées avec lui.
const PLAN_MAGIC: &[u8; 4] = b"DMP1";

// ------------------------------------------------------------------ planning

/// Le planning chiffré sur le disque du relais.
#[derive(Debug, Clone)]
pub struct PlanStore {
    path: PathBuf,
    key_path: PathBuf,
}

impl PlanStore {
    /// À côté du secret de liaison : le seul répertoire où l'agent sait
    /// pouvoir écrire.
    pub fn beside(secret_path: &Path) -> Self {
        Self {
            path: secret_path.with_file_name(PLAN_FILE),
            key_path: secret_path.with_file_name(KEY_FILE),
        }
    }

    /// Vrai si un planning a été rangé : ce relais l'était au dernier arrêt.
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Chiffre et range le planning (fichier voisin, puis renommage).
    pub fn save(&self, plan: &RelayPlan) -> Result<()> {
        use base64::Engine as _;
        let key = self.key(true)?.context("no relay plan key")?;
        let plain = serde_json::to_vec(plan).context("serialising the relay plan")?;
        let sealed = seal(&key, &plain)?;
        let partial = self.path.with_extension("enc.partial");
        let text = base64::engine::general_purpose::STANDARD.encode(sealed);
        crate::binding::write_private(&partial, &text)?;
        std::fs::rename(&partial, &self.path)
            .with_context(|| format!("renaming {}", partial.display()))?;
        Ok(())
    }

    /// Relit le planning. Absent, illisible ou sans sa clé : `None`, et le
    /// fichier est oublié — le serveur le redonnera.
    pub fn load(&self) -> Option<RelayPlan> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return None,
            Err(error) => {
                warn!(path = %self.path.display(), %error, "relay plan unreadable");
                return None;
            }
        };
        match self.decrypt(&text) {
            Ok(plan) => Some(plan),
            Err(error) => {
                warn!(
                    path = %self.path.display(),
                    error = %format!("{error:#}"),
                    "stored relay plan discarded, it will be fetched again from the server"
                );
                let _ = std::fs::remove_file(&self.path);
                None
            }
        }
    }

    fn decrypt(&self, text: &str) -> Result<RelayPlan> {
        use base64::Engine as _;
        let key = self.key(false)?.context("the relay plan key is missing")?;
        let sealed = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .context("relay plan not in base64")?;
        let plain = open(&key, &sealed)?;
        serde_json::from_slice(&plain).context("relay plan not in JSON")
    }

    /// Oublie planning et clé (relais coupé).
    pub fn forget(&self) {
        for path in [&self.path, &self.key_path] {
            match std::fs::remove_file(path) {
                Ok(()) => debug!(path = %path.display(), "relay plan forgotten"),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => warn!(path = %path.display(), %error, "cannot remove a relay file"),
            }
        }
    }

    /// Clé du planning ; `create` en tire une neuve si elle manque.
    fn key(&self, create: bool) -> Result<Option<[u8; 32]>> {
        match std::fs::read_to_string(&self.key_path) {
            Ok(text) => {
                let bytes = hex::decode(text.trim()).context("relay plan key not in hex")?;
                let key: [u8; 32] =
                    bytes.try_into().map_err(|_| anyhow!("relay plan key of the wrong length"))?;
                Ok(Some(key))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound && create => {
                let mut key = [0u8; 32];
                random(&mut key)?;
                crate::binding::write_private(&self.key_path, &hex::encode(key))?;
                Ok(Some(key))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => {
                Err(error).with_context(|| format!("reading {}", self.key_path.display()))
            }
        }
    }
}

fn random(buffer: &mut [u8]) -> Result<()> {
    use aws_lc_rs::rand::SecureRandom as _;
    aws_lc_rs::rand::SystemRandom::new()
        .fill(buffer)
        .map_err(|_| anyhow!("no random source for the relay plan key"))
}

fn cipher(key: &[u8; 32]) -> Result<aws_lc_rs::aead::LessSafeKey> {
    let key = aws_lc_rs::aead::UnboundKey::new(&aws_lc_rs::aead::AES_256_GCM, key)
        .map_err(|_| anyhow!("invalid relay plan key"))?;
    Ok(aws_lc_rs::aead::LessSafeKey::new(key))
}

/// `DMP1 | nonce (12) | chiffré + étiquette (16)`. La marque est authentifiée.
fn seal(key: &[u8; 32], plain: &[u8]) -> Result<Vec<u8>> {
    use aws_lc_rs::aead::{Aad, NONCE_LEN, Nonce};
    let mut nonce = [0u8; NONCE_LEN];
    random(&mut nonce)?;
    let mut sealed = plain.to_vec();
    cipher(key)?
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(PLAN_MAGIC),
            &mut sealed,
        )
        .map_err(|_| anyhow!("cannot encrypt the relay plan"))?;
    let mut out = Vec::with_capacity(PLAN_MAGIC.len() + NONCE_LEN + sealed.len());
    out.extend_from_slice(PLAN_MAGIC);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&sealed);
    Ok(out)
}

fn open(key: &[u8; 32], sealed: &[u8]) -> Result<Vec<u8>> {
    use aws_lc_rs::aead::{Aad, NONCE_LEN, Nonce};
    let body = sealed.strip_prefix(PLAN_MAGIC.as_slice()).context("not a relay plan file")?;
    if body.len() < NONCE_LEN {
        return Err(anyhow!("relay plan file truncated"));
    }
    let (nonce, encrypted) = body.split_at(NONCE_LEN);
    let nonce = Nonce::try_assume_unique_for_key(nonce).map_err(|_| anyhow!("bad nonce"))?;
    let mut buffer = encrypted.to_vec();
    let plain = cipher(key)?
        .open_in_place(nonce, Aad::from(PLAN_MAGIC), &mut buffer)
        .map_err(|_| anyhow!("relay plan does not decrypt with this key"))?;
    Ok(plain.to_vec())
}

// ------------------------------------------------------- planification locale

/// Échéances des équipements pendant une coupure.
#[derive(Debug, Default)]
pub struct OfflineSchedule {
    due: HashMap<TargetId, Instant>,
}

impl OfflineSchedule {
    /// Sondes échues à `now` ; leur échéance suivante est posée. Au premier
    /// passage, les équipements sont étalés sur leur période, comme le fait le
    /// serveur à son démarrage.
    pub fn due<'a>(&mut self, plan: &'a RelayPlan, now: Instant) -> Vec<&'a PlannedProbe> {
        let live: HashSet<TargetId> = plan.probes.iter().map(|p| p.target.id).collect();
        self.due.retain(|id, _| live.contains(id));
        let mut out = Vec::new();
        for probe in &plan.probes {
            if !probe.target.enabled {
                continue;
            }
            let interval = probe.target.interval.max(MIN_INTERVAL);
            let at = self
                .due
                .entry(probe.target.id)
                .or_insert_with(|| now + initial_offset(probe.target.id, interval));
            if *at <= now {
                *at = now + interval;
                out.push(probe);
            }
        }
        out
    }

    /// Prochaine échéance.
    pub fn next_wake(&self) -> Option<Instant> {
        self.due.values().min().copied()
    }
}

/// Même décalage que le planificateur du serveur (`scheduler.rs`).
fn initial_offset(id: TargetId, interval: Duration) -> Duration {
    Duration::from_secs(id.unsigned_abs() % interval.as_secs().max(1))
}

// ------------------------------------------------------- tampon des résultats

/// Ce que le relais publie de son tampon dans ses propres mesures
/// (`agent_relay_buffered_results`, `agent_relay_dropped_results`).
#[derive(Debug, Default)]
pub struct RelayStats {
    pending: AtomicU64,
    dropped: AtomicU64,
}

impl RelayStats {
    pub fn pending(&self) -> u64 {
        self.pending.load(Ordering::Relaxed)
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Un résultat tel qu'il est rangé dans le tampon (sa séquence est celle de
/// l'enregistrement).
#[derive(Debug, Serialize, Deserialize)]
struct Entry {
    target_id: TargetId,
    started_at_ms: i64,
    outcome: ProbeOutcome,
}

/// Tampon des résultats du relais, partagé par les sondes et le rattrapage.
#[derive(Clone)]
pub struct Backlog {
    spool: Arc<Mutex<Spool>>,
    stats: Arc<RelayStats>,
}

impl Backlog {
    pub fn open(dir: &Path, limit_bytes: u64) -> io::Result<Self> {
        let backlog = Self {
            spool: Arc::new(Mutex::new(Spool::open(dir, limit_bytes)?)),
            stats: Arc::new(RelayStats::default()),
        };
        backlog.publish(&backlog.lock());
        Ok(backlog)
    }

    pub fn stats(&self) -> Arc<RelayStats> {
        self.stats.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Range un résultat. Rien n'est gardé d'une sonde sans échantillon (un
    /// équipement injoignable) : il n'y aurait rien à reporter dans les
    /// graphes, et le serveur l'a constaté de son côté.
    pub fn keep(&self, target_id: TargetId, started_at_ms: i64, outcome: ProbeOutcome) -> bool {
        if outcome.samples.is_empty() {
            return false;
        }
        let entry = Entry { target_id, started_at_ms, outcome };
        let payload = match spool::encode(&entry) {
            Ok(payload) => payload,
            Err(error) => {
                warn!(%error, "cannot encode a relayed result");
                return false;
            }
        };
        let mut spool = self.lock();
        let kept = match spool.append_weighted(&[(payload.as_slice(), 1)]) {
            Ok(()) => true,
            Err(error) => {
                warn!(target = target_id, %error, "cannot keep a relayed result on disk");
                false
            }
        };
        self.publish(&spool);
        kept
    }

    /// Oublie tout (relais coupé).
    pub fn clear(&self) {
        let mut spool = self.lock();
        if let Err(error) = spool.clear() {
            warn!(%error, "cannot clear the relay buffer");
        }
        self.publish(&spool);
    }

    fn lock(&self) -> MutexGuard<'_, Spool> {
        // Rien n'est tenu à travers un `await`.
        self.spool.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn publish(&self, spool: &Spool) {
        self.stats.pending.store(spool.pending(), Ordering::Relaxed);
        self.stats.dropped.store(spool.dropped(), Ordering::Relaxed);
    }

    /// Prochain lot à renvoyer : au plus `max` résultats, et la séquence qui
    /// clôt le lot (résultats illisibles compris, jetés en route).
    fn next_batch(&self, max: usize) -> io::Result<Option<(RelayBacklog, u64)>> {
        let mut spool = self.lock();
        let records = spool.peek(max, BACKLOG_BYTES)?;
        let Some(last) = records.last().map(|r| r.seq) else { return Ok(None) };
        let mut results = Vec::with_capacity(records.len());
        for record in records {
            match spool::decode::<Entry>(&record.payload) {
                Ok(entry) => results.push(BacklogResult {
                    seq: record.seq,
                    target_id: entry.target_id,
                    started_at_ms: entry.started_at_ms,
                    outcome: entry.outcome,
                }),
                Err(error) => warn!(seq = record.seq, %error, "unreadable relayed result, dropped"),
            }
        }
        let backlog =
            RelayBacklog { spool_id: spool.id().to_string(), results, dropped: spool.dropped() };
        Ok(Some((backlog, last)))
    }

    fn ack(&self, seq: u64) {
        let mut spool = self.lock();
        if let Err(error) = spool.ack(seq) {
            warn!(%error, "cannot acknowledge the relay buffer");
        }
        self.publish(&spool);
    }

    /// Renvoie au serveur tout ce qui attend, lot après lot, jusqu'à ce que le
    /// tampon soit vide ou que le serveur cesse de répondre (le rattrapage
    /// reprendra au retour suivant). Rend le nombre de résultats rangés.
    pub async fn drain(&self, client: &PushClient, key: &str, mut shutdown: Shutdown) -> usize {
        let mut max = MAX_BACKLOG_RESULTS;
        let mut accepted = 0usize;
        loop {
            let (backlog, last) = match self.next_batch(max) {
                Ok(Some(batch)) => batch,
                Ok(None) => break,
                Err(error) => {
                    warn!(%error, "cannot read the relay buffer");
                    break;
                }
            };
            if backlog.results.is_empty() {
                self.ack(last);
                continue;
            }
            let count = backlog.results.len();
            let sent = tokio::select! {
                biased;
                _ = shutdown.wait() => break,
                sent = client.send_backlog(key, &backlog) => sent,
            };
            match sent {
                Ok(ack) => {
                    accepted += ack.accepted;
                    debug!(
                        count,
                        accepted = ack.accepted,
                        duplicates = ack.duplicates,
                        ignored = ack.ignored,
                        "relayed results caught up"
                    );
                    self.ack(last);
                }
                Err(PushError::TooLarge(message)) if count > 1 => {
                    max = (count / 2).max(1);
                    warn!(
                        reason = message,
                        max, "relay catch-up batch too large, sending smaller ones"
                    );
                    continue;
                }
                Err(error) if !error.is_retryable() || matches!(error, PushError::TooLarge(_)) => {
                    // Compris et refusé : le renvoyer à l'identique n'y
                    // changerait rien, et bloquerait tout ce qui suit.
                    warn!(%error, dropped = count, "relay catch-up batch refused, dropped");
                    self.ack(last);
                }
                Err(error) => {
                    debug!(%error, "relay catch-up interrupted, it resumes when the server is back");
                    break;
                }
            }
            if self.is_empty() {
                break;
            }
            tokio::select! {
                biased;
                _ = shutdown.wait() => break,
                _ = tokio::time::sleep(BACKLOG_PAUSE) => {}
            }
        }
        if accepted > 0 {
            info!(accepted, "results probed while the server was unreachable sent to it");
        }
        accepted
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use dumbmonit_proto::{Credential, MetricKind, Sample, Target};

    use super::*;
    use crate::testsupport::fake_http;

    fn target(id: TargetId, interval_secs: u64) -> Target {
        Target {
            id,
            name: format!("équipement-{id}"),
            address: "10.0.0.1".into(),
            kind: "snmp".into(),
            profile_id: None,
            parent_id: None,
            interval: Duration::from_secs(interval_secs),
            enabled: true,
            tags: BTreeMap::new(),
            credential: Credential::ApiToken { token: "s3cr3t-communauté".into() },
            group_name: String::new(),
            position: 0,
        }
    }

    fn plan(targets: Vec<Target>) -> RelayPlan {
        RelayPlan {
            probes: targets
                .into_iter()
                .map(|target| PlannedProbe { target, timeout_secs: 10 })
                .collect(),
            issued_at_ms: 1_000,
        }
    }

    fn outcome(ts_ms: i64) -> ProbeOutcome {
        ProbeOutcome {
            duration_ms: 5,
            error: None,
            samples: vec![Sample::new("up", 1.0, MetricKind::Gauge, ts_ms)],
            profile_id: None,
            netconfig: None,
        }
    }

    #[test]
    fn the_stored_plan_is_encrypted_and_comes_back_with_its_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlanStore::beside(&dir.path().join("agent-secret"));
        assert!(!store.exists());
        assert!(store.load().is_none());

        store.save(&plan(vec![target(1, 60), target(2, 30)])).unwrap();
        assert!(store.exists());
        let on_disk = std::fs::read_to_string(dir.path().join(PLAN_FILE)).unwrap();
        assert!(!on_disk.contains("s3cr3t"), "jamais d'identifiant en clair sur le disque");
        let back = store.load().expect("planning relu");
        assert_eq!(back.probes.len(), 2);
        assert_eq!(
            back.probes[0].target.credential,
            Credential::ApiToken { token: "s3cr3t-communauté".into() }
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(KEY_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "la clé n'est lisible que par l'agent");
        }

        // Rangé de nouveau : la même clé sert, un nonce neuf est tiré.
        store.save(&plan(vec![target(3, 60)])).unwrap();
        let again = std::fs::read_to_string(dir.path().join(PLAN_FILE)).unwrap();
        assert_ne!(on_disk, again);
        assert_eq!(store.load().unwrap().probes[0].target.id, 3);
    }

    #[test]
    fn a_plan_without_its_key_or_tampered_with_is_dropped_not_trusted() {
        let dir = tempfile::tempdir().unwrap();
        let store = PlanStore::beside(&dir.path().join("agent-secret"));
        store.save(&plan(vec![target(1, 60)])).unwrap();

        // Un octet changé : l'étiquette d'authentification ne passe plus.
        use base64::Engine as _;
        let path = dir.path().join(PLAN_FILE);
        let text = std::fs::read_to_string(&path).unwrap();
        let mut sealed = base64::engine::general_purpose::STANDARD.decode(text.trim()).unwrap();
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        std::fs::write(&path, base64::engine::general_purpose::STANDARD.encode(&sealed)).unwrap();
        assert!(store.load().is_none());
        assert!(!store.exists(), "un fichier illisible est oublié");

        // Sans la clé : illisible, et oublié lui aussi.
        store.save(&plan(vec![target(1, 60)])).unwrap();
        std::fs::remove_file(dir.path().join(KEY_FILE)).unwrap();
        assert!(store.load().is_none());

        // Relais coupé : tout part.
        store.save(&plan(vec![target(1, 60)])).unwrap();
        store.forget();
        assert!(!store.exists());
        assert!(!dir.path().join(KEY_FILE).exists());
    }

    #[test]
    fn the_offline_schedule_follows_each_device_period() {
        let planning = plan(vec![target(60, 60), target(7, 30)]);
        let mut schedule = OfflineSchedule::default();
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);

        // L'équipement 60 (décalage 60 % 60 = 0) part tout de suite, le 7
        // attend son décalage (7 s).
        let ids = |due: Vec<&PlannedProbe>| due.iter().map(|p| p.target.id).collect::<Vec<_>>();
        assert_eq!(ids(schedule.due(&planning, start)), vec![60]);
        assert_eq!(schedule.next_wake(), Some(at(7)));
        assert!(schedule.due(&planning, at(1)).is_empty());
        assert_eq!(ids(schedule.due(&planning, at(7))), vec![7]);
        assert_eq!(ids(schedule.due(&planning, at(37))), vec![7]);
        assert_eq!(ids(schedule.due(&planning, at(60))), vec![60]);

        // Un équipement retiré du planning n'est plus suivi ; un équipement en
        // pause n'est pas sondé.
        let mut smaller = plan(vec![target(7, 30)]);
        smaller.probes[0].target.enabled = false;
        assert!(schedule.due(&smaller, at(600)).is_empty());
        assert_eq!(schedule.next_wake(), Some(at(67)));
    }

    #[test]
    fn an_absurd_period_is_raised_to_the_minimum() {
        let planning = plan(vec![target(10, 0)]);
        let mut schedule = OfflineSchedule::default();
        let start = Instant::now();
        assert_eq!(schedule.due(&planning, start).len(), 1);
        assert!(schedule.due(&planning, start + Duration::from_secs(5)).is_empty());
        assert_eq!(schedule.due(&planning, start + MIN_INTERVAL).len(), 1);
    }

    #[test]
    fn results_without_samples_are_not_kept() {
        let dir = tempfile::tempdir().unwrap();
        let backlog = Backlog::open(dir.path(), 1 << 20).unwrap();
        let mut failed = outcome(1);
        failed.samples.clear();
        failed.error = Some("Monitor unreachable: timeout".into());
        assert!(!backlog.keep(1, 1, failed));
        assert!(backlog.keep(1, 1, outcome(1)));
        assert_eq!(backlog.stats().pending(), 1);
    }

    #[tokio::test]
    async fn the_backlog_is_sent_in_order_and_forgotten_once_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let backlog = Backlog::open(dir.path(), 1 << 20).unwrap();
        for n in 0..5 {
            assert!(backlog.keep(n, 1_000 + n, outcome(1_000 + n)));
        }
        let (url, calls) = fake_http(|req| {
            let body: RelayBacklog = serde_json::from_str(&req.body).expect("lot");
            let last = body.results.last().map_or(0, |r| r.seq);
            let ack = dumbmonit_proto::BacklogAck {
                acked_seq: last,
                accepted: body.results.len(),
                duplicates: 0,
                ignored: 0,
            };
            (200, serde_json::to_string(&ack).unwrap())
        })
        .await;
        let client = PushClient::new(&url, "dmon_jeton", None, Duration::from_secs(5)).unwrap();
        let (_trigger, shutdown) = crate::shutdown::channel();
        assert_eq!(backlog.drain(&client, "machine", shutdown).await, 5);
        assert!(backlog.is_empty());
        assert_eq!(backlog.stats().pending(), 0);

        let bodies = calls.bodies("POST", dumbmonit_proto::RELAY_BACKLOG_PATH);
        assert_eq!(bodies.len(), 1, "cinq résultats tiennent dans un lot");
        let sent: RelayBacklog = serde_json::from_str(&bodies[0]).unwrap();
        let targets: Vec<TargetId> = sent.results.iter().map(|r| r.target_id).collect();
        assert_eq!(targets, vec![0, 1, 2, 3, 4]);
        // Horodatages d'origine, pas celui du renvoi.
        assert_eq!(sent.results[2].outcome.samples[0].ts_ms, 1_002);
        assert!(sent.results.windows(2).all(|w| w[0].seq < w[1].seq));
    }

    #[tokio::test]
    async fn an_unreachable_server_keeps_the_backlog_for_the_next_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let backlog = Backlog::open(dir.path(), 1 << 20).unwrap();
        backlog.keep(1, 1, outcome(1));
        let (url, _calls) = fake_http(|_| (503, r#"{"error":"busy"}"#.to_string())).await;
        let client = PushClient::new(&url, "dmon_jeton", None, Duration::from_secs(5)).unwrap();
        let (_trigger, shutdown) = crate::shutdown::channel();
        assert_eq!(backlog.drain(&client, "machine", shutdown).await, 0);
        assert!(!backlog.is_empty(), "rien n'est perdu");

        // Et le tampon survit au redémarrage du relais.
        drop(backlog);
        let backlog = Backlog::open(dir.path(), 1 << 20).unwrap();
        assert_eq!(backlog.stats().pending(), 1);
    }

    #[tokio::test]
    async fn a_refused_batch_is_dropped_rather_than_blocking_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let backlog = Backlog::open(dir.path(), 1 << 20).unwrap();
        backlog.keep(1, 1, outcome(1));
        let (url, _calls) =
            fake_http(|_| (422, r#"{"error":"unreadable backlog"}"#.to_string())).await;
        let client = PushClient::new(&url, "dmon_jeton", None, Duration::from_secs(5)).unwrap();
        let (_trigger, shutdown) = crate::shutdown::channel();
        backlog.drain(&client, "machine", shutdown).await;
        assert!(backlog.is_empty());
    }
}
