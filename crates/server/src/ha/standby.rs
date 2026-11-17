//! Le serveur en attente : il regarde le bail, recopie la réplique, et ne
//! fait rien d'autre — ni base ouverte, ni port, ni sonde.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, Connection, Row};
use tokio::time::Instant;
use tracing::{error, info, warn};

use super::shared::Shared;
use super::{
    HaConfig, Lease, Manifest, PROMOTE_FILE, PromoteRequest, REPLICA_FILE, Role, STATE_FILE,
    Status, now_ms, write_atomic,
};
use crate::crypto::Cipher;

/// Ce que le serveur en attente sait du détenteur du bail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Liveness {
    /// Le bail a bougé il y a peu : l'actif est là.
    Alive,
    /// Pas encore assez observé pour le dire.
    Unknown,
    /// Rien n'a bougé depuis ce temps (bail rendu, absent ou figé).
    Silent(Duration),
}

/// Suivi du bail d'un relevé à l'autre, sur l'horloge locale : celle de
/// l'actif n'entre jamais dans la décision.
pub(super) struct Watch {
    first: Instant,
    key: Option<(String, String, u64, u64, bool)>,
    changed_at: Instant,
    observed: bool,
    seen_change: bool,
}

impl Watch {
    pub(super) fn new(now: Instant) -> Self {
        Self { first: now, key: None, changed_at: now, observed: false, seen_change: false }
    }

    pub(super) fn observe(&mut self, lease: Option<&Lease>, now: Instant) {
        let key = lease.map(Lease::key);
        if !self.observed {
            // Le premier relevé n'est pas un changement : il n'y avait rien avant.
            self.observed = true;
            self.first = now;
            self.changed_at = now;
            self.key = key;
            return;
        }
        if key != self.key {
            self.seen_change = true;
            self.key = key;
            self.changed_at = now;
        }
    }

    /// Le bail vu en dernier a été rendu par un arrêt propre.
    pub(super) fn released(&self) -> bool {
        self.key.as_ref().is_some_and(|key| key.4)
    }

    pub(super) fn liveness(&self, now: Instant, live: Duration) -> Liveness {
        let still = now.saturating_duration_since(self.changed_at);
        match &self.key {
            None => Liveness::Silent(still),
            Some(key) if key.4 => Liveness::Silent(still),
            Some(_) if self.seen_change && still < live => Liveness::Alive,
            Some(_) if !self.seen_change && now.saturating_duration_since(self.first) < live => {
                Liveness::Unknown
            }
            Some(_) => Liveness::Silent(still),
        }
    }
}

/// Décision du serveur en attente, à chaque relevé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Verdict {
    Wait,
    Promote(&'static str),
    /// Demande de promotion refusée, avec la raison (en anglais).
    Refuse(String),
}

/// Ce qui décide d'une promotion.
#[derive(Debug, Clone)]
pub(super) struct Facts<'a> {
    pub role: Role,
    pub takeover_after: Option<Duration>,
    pub liveness: Liveness,
    pub holder: Option<&'a str>,
    /// Le bail a été rendu par un arrêt propre de son détenteur.
    pub released: bool,
    pub request: Option<PromoteRequest>,
    /// La dernière réplique ne se déchiffre pas avec le secret de ce serveur.
    pub secret_mismatch: bool,
}

pub(super) fn decide(facts: &Facts<'_>) -> Verdict {
    if facts.secret_mismatch {
        // Démarrer échouerait de toute façon : la base ne s'ouvrirait pas.
        return match facts.request {
            Some(_) => Verdict::Refuse(SECRET_MISMATCH.to_string()),
            None => Verdict::Wait,
        };
    }
    if let Some(request) = facts.request {
        return match facts.liveness {
            _ if request.force => Verdict::Promote("forced promotion"),
            Liveness::Silent(_) => Verdict::Promote("promotion requested"),
            Liveness::Unknown => Verdict::Wait,
            Liveness::Alive => Verdict::Refuse(format!(
                "the active server '{}' still renews its lease: stop it first (or use --force)",
                facts.holder.unwrap_or("?")
            )),
        };
    }
    match facts.liveness {
        // Bail rendu par un arrêt propre, et ce serveur est le primaire : il
        // reprend sa place sans attendre. Un bail seulement figé (serveur
        // tombé, ou coupé du stockage) attend le délai de reprise.
        Liveness::Silent(_) if facts.role == Role::Primary && facts.released => {
            Verdict::Promote("the primary takes back a released lease")
        }
        Liveness::Silent(still) if facts.takeover_after.is_some_and(|after| still >= after) => {
            Verdict::Promote("automatic takeover: no heartbeat from the active server")
        }
        _ => Verdict::Wait,
    }
}

const SECRET_MISMATCH: &str = "the replica cannot be decrypted with this server's instance \
     secret: copy /data/secret.key from the active server (or set the same DUMBMONIT_SECRET) \
     and restart this one";

/// Attend, en serveur passif, qu'il soit temps de devenir actif. Rend le
/// dernier bail vu (son époque fixe celle du suivant).
pub(super) async fn wait(
    ha: &HaConfig,
    shared: &Shared,
    work: &Path,
    secret: &str,
) -> Result<Option<Lease>> {
    info!(
        node = ha.node,
        "standby: waiting (no database, port or probe until this server is promoted)"
    );
    let mut watch = Watch::new(Instant::now());
    let mut replica: Option<Manifest> = None;
    let mut status = Status {
        node: ha.node.clone(),
        role: "standby".into(),
        takeover_after_s: ha.takeover_after.map(|after| after.as_secs()),
        ..Status::default()
    };
    let mut secret_mismatch = false;
    let mut storage_failing = false;
    let mut announced_holder: Option<String> = None;
    let mut first = true;
    loop {
        if !first {
            tokio::time::sleep(ha.timing.beat).await;
        }
        first = false;
        let now = Instant::now();
        let lease = match shared.read_lease().await {
            Ok(lease) => {
                storage_failing = false;
                lease
            }
            Err(cause) => {
                if !std::mem::replace(&mut storage_failing, true) {
                    let error = format!("{cause:#}");
                    warn!(error, "standby: the shared directory is unreadable");
                }
                status.replica_error = Some(format!("{cause:#}"));
                status.updated_at_ms = now_ms();
                write_status(work, &status).await;
                continue;
            }
        };
        watch.observe(lease.as_ref(), now);
        let liveness = watch.liveness(now, ha.timing.live);
        let holder = lease.as_ref().map(|lease| lease.holder.clone());
        if holder != announced_holder {
            if let Some(holder) = &holder {
                info!(holder, "standby: the lease is held by another server");
            }
            announced_holder = holder.clone();
        }

        // Réplique : recopiée dès qu'un nouvel instantané est publié.
        match shared.read_manifest().await {
            Ok(Some(manifest)) if replica.as_ref() != Some(&manifest) => {
                match pull(shared, &manifest, work, secret).await {
                    Ok(()) => {
                        if secret_mismatch {
                            info!("standby: the replica decrypts with this server's secret now");
                        }
                        secret_mismatch = false;
                        status.replica_error = version_note(&manifest);
                        status.replica_taken_at_ms = Some(manifest.taken_at_ms);
                        status.replica_from = Some(manifest.node.clone());
                        replica = Some(manifest);
                    }
                    Err(Pull::Secret) => {
                        if !secret_mismatch {
                            error!("standby: {SECRET_MISMATCH}");
                        }
                        secret_mismatch = true;
                        status.replica_error = Some(SECRET_MISMATCH.to_string());
                        // Retenté au prochain instantané, ou au redémarrage.
                        replica = Some(manifest);
                    }
                    Err(Pull::Failed(cause)) => {
                        warn!(error = %format!("{cause:#}"), "standby: replica not refreshed");
                        status.replica_error = Some(format!("{cause:#}"));
                    }
                }
            }
            Ok(_) => {}
            Err(cause) => status.replica_error = Some(format!("{cause:#}")),
        }

        let request = read_request(work).await;
        let verdict = decide(&Facts {
            role: ha.role,
            takeover_after: ha.takeover_after,
            liveness,
            holder: holder.as_deref(),
            released: watch.released(),
            request,
            secret_mismatch,
        });
        status.holder = holder;
        status.holder_alive = Some(matches!(liveness, Liveness::Alive));
        status.updated_at_ms = now_ms();
        match verdict {
            Verdict::Wait => {
                // Un refus reste affiché jusqu'à la demande suivante.
                if let Some(request) = request {
                    status.promotion = Some("pending".to_string());
                    status.promotion_for_ms = Some(request.requested_at_ms);
                }
                write_status(work, &status).await;
            }
            Verdict::Refuse(reason) => {
                warn!(reason, "standby: promotion refused");
                let _ = tokio::fs::remove_file(work.join(PROMOTE_FILE)).await;
                status.promotion = Some(format!("refused: {reason}"));
                status.promotion_for_ms = request.map(|request| request.requested_at_ms);
                write_status(work, &status).await;
            }
            Verdict::Promote(reason) => {
                info!(reason, "standby: becoming the active server");
                let _ = tokio::fs::remove_file(work.join(PROMOTE_FILE)).await;
                return Ok(lease);
            }
        }
    }
}

/// Une réplique écrite par une autre version : le dire, la promotion
/// échouerait si la base est plus récente que ce binaire.
fn version_note(manifest: &Manifest) -> Option<String> {
    let mine = env!("CARGO_PKG_VERSION");
    (manifest.version != mine).then(|| {
        format!(
            "the replica comes from version {} and this server runs {mine}: keep both servers on \
             the same version",
            manifest.version
        )
    })
}

enum Pull {
    Secret,
    Failed(anyhow::Error),
}

/// Recopie l'instantané et le vérifie avant de le garder comme réplique.
async fn pull(shared: &Shared, manifest: &Manifest, work: &Path, secret: &str) -> Result<(), Pull> {
    let fresh = work.join(format!("{REPLICA_FILE}.new"));
    shared.fetch(manifest, &fresh).await.map_err(Pull::Failed)?;
    match verify(&fresh, secret).await {
        Ok(()) => {}
        Err(Check::Secret) => {
            let _ = tokio::fs::remove_file(&fresh).await;
            return Err(Pull::Secret);
        }
        Err(Check::Failed(cause)) => {
            let _ = tokio::fs::remove_file(&fresh).await;
            return Err(Pull::Failed(cause));
        }
    }
    tokio::fs::rename(&fresh, work.join(REPLICA_FILE))
        .await
        .context("keeping the replica")
        .map_err(Pull::Failed)
}

pub(super) enum Check {
    Secret,
    Failed(anyhow::Error),
}

/// Une base complète, intègre, que le secret de ce serveur déchiffre. Ouverte
/// en lecture seule, sans rien exécuter de son schéma.
pub(super) async fn verify(path: &Path, secret: &str) -> Result<(), Check> {
    let failed = |cause: sqlx::Error| Check::Failed(anyhow::Error::from(cause));
    let mut conn = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .immutable(true)
        .create_if_missing(false)
        .pragma("trusted_schema", "OFF")
        .connect()
        .await
        .map_err(failed)?;
    let row = sqlx::query("PRAGMA quick_check").fetch_one(&mut conn).await.map_err(failed)?;
    let integrity: String = row.try_get(0).map_err(failed)?;
    if integrity != "ok" {
        let _ = conn.close().await;
        return Err(Check::Failed(anyhow::anyhow!("integrity check failed: {integrity}")));
    }
    let row = sqlx::query("SELECT salt, canary FROM instance WHERE id = 1")
        .fetch_optional(&mut conn)
        .await
        .map_err(failed)?;
    let _ = conn.close().await;
    let Some(row) = row else {
        return Err(Check::Failed(anyhow::anyhow!("the replica has no instance key check")));
    };
    let salt: Vec<u8> = row.try_get("salt").map_err(failed)?;
    let canary: Vec<u8> = row.try_get("canary").map_err(failed)?;
    let cipher = Cipher::derive(secret, &salt).map_err(Check::Failed)?;
    cipher.verify_canary(&canary).map_err(|_| Check::Secret)
}

/// Installe la dernière réplique à la place de la base locale, au moment de
/// devenir actif. La base remplacée est gardée à côté (`ha/previous.db`).
/// Sans instantané publié, la base locale reste (premier démarrage).
pub(super) async fn install_latest(
    shared: &Shared,
    work: &Path,
    database: &Path,
    secret: &str,
) -> Result<()> {
    let Some(manifest) = shared.read_manifest().await? else {
        warn!("no replica in the shared directory: starting from the local database");
        return Ok(());
    };
    match pull(shared, &manifest, work, secret).await {
        Ok(()) => {}
        Err(Pull::Secret) => anyhow::bail!("{SECRET_MISMATCH}"),
        Err(Pull::Failed(cause)) => return Err(cause),
    }
    if let Some(note) = version_note(&manifest) {
        warn!("{note}");
    }
    let replica = work.join(REPLICA_FILE);
    let previous = work.join("previous.db");
    // Le journal WAL d'une base appartient à elle seule : laissé là, SQLite
    // le rejouerait sur la réplique et la corromprait. Il suit la base mise
    // de côté.
    for suffix in ["", "-wal", "-shm"] {
        let _ = tokio::fs::remove_file(with_suffix(&previous, suffix)).await;
    }
    if tokio::fs::try_exists(database).await.unwrap_or(false) {
        for suffix in ["", "-wal", "-shm"] {
            let from = with_suffix(database, suffix);
            if tokio::fs::try_exists(&from).await.unwrap_or(false) {
                tokio::fs::rename(&from, with_suffix(&previous, suffix))
                    .await
                    .with_context(|| format!("setting {} aside", from.display()))?;
            }
        }
    }
    for suffix in ["-wal", "-shm"] {
        let _ = tokio::fs::remove_file(with_suffix(database, suffix)).await;
    }
    // Une copie plutôt qu'un renommage : `ha/` et la base peuvent être sur deux
    // systèmes de fichiers, et la réplique reste prête pour la suite.
    let staged = with_suffix(database, ".replica");
    tokio::fs::copy(&replica, &staged)
        .await
        .with_context(|| format!("copying the replica to {}", staged.display()))?;
    tokio::fs::rename(&staged, database)
        .await
        .with_context(|| format!("installing the replica as {}", database.display()))?;
    info!(
        from = manifest.node,
        taken_at_ms = manifest.taken_at_ms,
        "latest replica of the database installed"
    );
    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    std::path::PathBuf::from(name)
}

/// Demande de promotion déposée par `dumbmonit ha promote`, si elle est
/// lisible.
async fn read_request(work: &Path) -> Option<PromoteRequest> {
    let bytes = tokio::fs::read(work.join(PROMOTE_FILE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// État en cours de promotion, tel que `dumbmonit ha status` le montre.
pub(super) fn promoting_status(ha: &HaConfig) -> Status {
    Status {
        node: ha.node.clone(),
        role: "promoting".into(),
        updated_at_ms: now_ms(),
        takeover_after_s: ha.takeover_after.map(|after| after.as_secs()),
        ..Status::default()
    }
}

/// Écrit l'état de ce serveur (au mieux : un échec n'arrête rien).
pub(super) async fn write_status(work: &Path, status: &Status) {
    let Ok(bytes) = serde_json::to_vec_pretty(status) else { return };
    if let Err(cause) = write_atomic(&work.join(STATE_FILE), &bytes).await {
        tracing::debug!(error = %format!("{cause:#}"), "cannot write the HA state file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIVE: Duration = Duration::from_secs(30);

    fn lease(beat: u64, released: bool) -> Lease {
        Lease { beat, released, ..Lease::new("a", "i", 1) }
    }

    #[test]
    fn the_lease_is_alive_only_while_it_moves() {
        let start = Instant::now();
        let mut watch = Watch::new(start);
        watch.observe(Some(&lease(1, false)), start);
        // Tout juste arrivé : on ne sait pas encore.
        assert_eq!(watch.liveness(start + Duration::from_secs(10), LIVE), Liveness::Unknown);
        watch.observe(Some(&lease(2, false)), start + Duration::from_secs(10));
        assert_eq!(watch.liveness(start + Duration::from_secs(20), LIVE), Liveness::Alive);
        // Plus rien ne bouge : silencieux depuis le dernier battement.
        assert_eq!(
            watch.liveness(start + Duration::from_secs(70), LIVE),
            Liveness::Silent(Duration::from_secs(60))
        );
        // Un bail figé dès le départ est silencieux depuis le premier relevé.
        let mut frozen = Watch::new(start);
        frozen.observe(Some(&lease(5, false)), start);
        assert_eq!(
            frozen.liveness(start + Duration::from_secs(40), LIVE),
            Liveness::Silent(Duration::from_secs(40))
        );
        // Rendu : silencieux tout de suite.
        let mut released = Watch::new(start);
        released.observe(Some(&lease(5, true)), start);
        assert_eq!(released.liveness(start, LIVE), Liveness::Silent(Duration::ZERO));
        let mut absent = Watch::new(start);
        absent.observe(None, start);
        assert!(matches!(absent.liveness(start + LIVE, LIVE), Liveness::Silent(_)));
    }

    fn facts(liveness: Liveness) -> Facts<'static> {
        Facts {
            role: Role::Standby,
            takeover_after: None,
            liveness,
            holder: Some("a"),
            released: false,
            request: None,
            secret_mismatch: false,
        }
    }

    fn request(force: bool) -> Option<PromoteRequest> {
        Some(PromoteRequest { force, requested_at_ms: 0 })
    }

    #[test]
    fn a_promotion_request_waits_for_the_active_server_to_be_silent() {
        let silent = Liveness::Silent(Duration::from_secs(40));
        assert_eq!(decide(&facts(silent)), Verdict::Wait, "rien sans demande ni reprise");
        assert!(matches!(
            decide(&Facts { request: request(false), ..facts(silent) }),
            Verdict::Promote(_)
        ));
        assert!(matches!(
            decide(&Facts { request: request(false), ..facts(Liveness::Alive) }),
            Verdict::Refuse(_)
        ));
        assert_eq!(
            decide(&Facts { request: request(false), ..facts(Liveness::Unknown) }),
            Verdict::Wait
        );
        assert!(matches!(
            decide(&Facts { request: request(true), ..facts(Liveness::Alive) }),
            Verdict::Promote(_)
        ));
        // Une réplique que ce secret ne déchiffre pas : jamais, même forcé.
        assert!(matches!(
            decide(&Facts { request: request(true), secret_mismatch: true, ..facts(silent) }),
            Verdict::Refuse(_)
        ));
    }

    #[test]
    fn the_automatic_takeover_waits_for_the_whole_delay() {
        let after = Some(Duration::from_secs(300));
        let auto = |still: u64| Facts {
            takeover_after: after,
            ..facts(Liveness::Silent(Duration::from_secs(still)))
        };
        assert_eq!(decide(&auto(299)), Verdict::Wait);
        assert!(matches!(decide(&auto(300)), Verdict::Promote(_)));
        let alive = Facts { takeover_after: after, ..facts(Liveness::Alive) };
        assert_eq!(decide(&alive), Verdict::Wait);
        assert_eq!(decide(&Facts { secret_mismatch: true, ..auto(900) }), Verdict::Wait);
        // Le primaire retrouve un bail rendu : il reprend sa place aussitôt.
        let released = Facts {
            role: Role::Primary,
            released: true,
            ..facts(Liveness::Silent(Duration::ZERO))
        };
        assert!(matches!(decide(&released), Verdict::Promote(_)));
        // Un bail seulement figé (l'autre est tombé, ou coupé du stockage)
        // attend le délai de reprise, comme pour le secours.
        let still = Liveness::Silent(Duration::from_secs(60));
        let frozen = Facts { role: Role::Primary, ..facts(still) };
        assert_eq!(decide(&frozen), Verdict::Wait);
        assert_eq!(decide(&Facts { role: Role::Primary, ..facts(Liveness::Alive) }), Verdict::Wait);
    }
}
