//! Le serveur actif : il renouvelle son bail, publie la réplique de sa base,
//! et s'arrête de lui-même dès qu'il n'est plus sûr d'être le seul actif.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use sqlx::SqlitePool;
use tokio::time::Instant;
use tracing::{debug, error, info, warn};

use super::shared::Shared;
use super::{EXIT_FENCED, HaConfig, Lease, Manifest, Status, now_ms, standby};

/// Délai laissé au dernier instantané, à l'arrêt.
const FINAL_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(10);

/// Ce qui dit qu'une base a changé : date et taille de la base et de son
/// journal WAL.
type Stamp = Vec<Option<(SystemTime, u64)>>;

/// Ce serveur, actif : son bail, sa réplication.
#[derive(Clone)]
pub struct Active(Arc<Inner>);

struct Inner {
    ha: HaConfig,
    shared: Shared,
    work: PathBuf,
    database: PathBuf,
    /// Bail tenu. Verrouillé pendant tout un renouvellement, et pendant sa
    /// restitution à l'arrêt : les deux ne se croisent jamais.
    lease: tokio::sync::Mutex<Lease>,
    status: Mutex<Status>,
    /// Arrêt en cours : plus de renouvellement, plus d'instantané périodique.
    stopping: AtomicBool,
    /// Un instantané à la fois ; garde l'empreinte de la base au dernier.
    snapshot: tokio::sync::Mutex<Option<Stamp>>,
}

/// Issue d'un renouvellement du bail.
#[derive(Debug)]
pub(super) enum Renewal {
    Renewed,
    /// Le bail est à un autre : s'arrêter tout de suite.
    Lost(String),
    /// Stockage partagé injoignable ou trop lent : réessayer.
    Failed(anyhow::Error),
}

impl Active {
    /// Prend son service : le battement part tout de suite, avant même
    /// l'ouverture de la base.
    pub(super) fn start(
        ha: HaConfig,
        shared: Shared,
        lease: Lease,
        work: PathBuf,
        database: PathBuf,
    ) -> Self {
        let active = Self::new(ha, shared, lease, work, database);
        let beating = active.clone();
        tokio::spawn(async move { beating.heartbeat().await });
        active
    }

    pub(super) fn new(
        ha: HaConfig,
        shared: Shared,
        lease: Lease,
        work: PathBuf,
        database: PathBuf,
    ) -> Self {
        let status = Status {
            node: ha.node.clone(),
            role: "active".into(),
            updated_at_ms: now_ms(),
            epoch: Some(lease.epoch),
            holder: Some(ha.node.clone()),
            takeover_after_s: ha.takeover_after.map(|after| after.as_secs()),
            ..Status::default()
        };
        Self(Arc::new(Inner {
            ha,
            shared,
            work,
            database,
            lease: tokio::sync::Mutex::new(lease),
            status: Mutex::new(status),
            stopping: AtomicBool::new(false),
            snapshot: tokio::sync::Mutex::new(None),
        }))
    }

    async fn heartbeat(self) {
        let mut last_renewed = Instant::now();
        let mut failing = false;
        loop {
            tokio::time::sleep(self.0.ha.timing.beat).await;
            if self.0.stopping.load(Ordering::SeqCst) {
                return;
            }
            match self.renew().await {
                Renewal::Renewed => {
                    if std::mem::take(&mut failing) {
                        info!("high availability: lease renewed again");
                    }
                    last_renewed = Instant::now();
                }
                Renewal::Lost(reason) => fence(&reason),
                Renewal::Failed(cause) => {
                    let silent = last_renewed.elapsed();
                    if !std::mem::replace(&mut failing, true) {
                        let error = format!("{cause:#}");
                        warn!(error, "high availability: cannot renew the lease");
                    }
                    if let Some(limit) = self.0.ha.fence_after()
                        && silent >= limit
                    {
                        fence(&format!(
                            "the lease could not be renewed for {} s: the standby may take over",
                            silent.as_secs()
                        ));
                    }
                }
            }
            self.write_status().await;
        }
    }

    /// Relit le bail, puis le renouvelle s'il est toujours le nôtre.
    pub(super) async fn renew(&self) -> Renewal {
        let mut held = self.0.lease.lock().await;
        if self.0.stopping.load(Ordering::SeqCst) {
            return Renewal::Renewed;
        }
        let started = Instant::now();
        let current = match self.0.shared.read_lease().await {
            Ok(current) => current,
            Err(cause) => return Renewal::Failed(cause),
        };
        if let Some(current) = &current
            && !same_hold(current, &held)
        {
            return Renewal::Lost(format!(
                "the lease is now held by '{}' (epoch {})",
                current.holder, current.epoch
            ));
        }
        // Une relecture trop lente laisse à un autre le temps de prendre le
        // bail entre-temps : on ne l'écrase pas sur la foi d'une lecture
        // périmée.
        if started.elapsed() > self.0.ha.timing.beat / 2 {
            return Renewal::Failed(anyhow::anyhow!("the shared directory answers too slowly"));
        }
        let next = Lease { beat: held.beat + 1, renewed_at_ms: now_ms(), ..held.clone() };
        match self.0.shared.write_lease(&next).await {
            Ok(()) => {
                *held = next;
                Renewal::Renewed
            }
            Err(cause) => Renewal::Failed(cause),
        }
    }

    /// Publie la réplique de la base, toutes les `snapshot_every`, quand elle
    /// a changé. Le premier instantané part tout de suite.
    pub fn spawn_replication(&self, pool: SqlitePool) {
        let active = self.clone();
        tokio::spawn(async move {
            let mut failing = false;
            loop {
                if active.0.stopping.load(Ordering::SeqCst) {
                    return;
                }
                match active.snapshot(&pool).await {
                    Ok(Some(manifest)) => {
                        if std::mem::take(&mut failing) {
                            info!("high availability: replication works again");
                        }
                        debug!(file = manifest.file, size = manifest.size, "replica published");
                    }
                    Ok(None) => {}
                    Err(cause) => {
                        let error = format!("{cause:#}");
                        if !std::mem::replace(&mut failing, true) {
                            warn!(error, "high availability: the replica could not be published");
                        }
                        active.status(|status| status.replica_error = Some(error));
                    }
                }
                tokio::time::sleep(active.0.ha.snapshot_every).await;
            }
        });
    }

    /// Un instantané, s'il y a du nouveau : `None` sinon.
    pub(super) async fn snapshot(&self, pool: &SqlitePool) -> Result<Option<Manifest>> {
        let mut last = self.0.snapshot.lock().await;
        let stamp = stamp(&self.0.database).await;
        if last.as_ref() == Some(&stamp) {
            return Ok(None);
        }
        // Sous son propre bail seulement : un actif destitué n'écrase pas la
        // réplique de celui qui l'a remplacé.
        let held = self.0.lease.lock().await.clone();
        let current = self.0.shared.read_lease().await?;
        if !current.as_ref().is_some_and(|current| same_hold(current, &held)) {
            anyhow::bail!("this server no longer holds the lease: no replica published");
        }
        let outgoing = self.0.work.join("outgoing.db");
        let _ = tokio::fs::remove_file(&outgoing).await;
        // `VACUUM INTO` : une copie complète et cohérente, la base restant
        // servie. Le chemin part en paramètre, jamais dans le SQL.
        let target = outgoing.to_string_lossy().to_string();
        sqlx::query("VACUUM INTO ?")
            .bind(&target)
            .execute(pool)
            .await
            .with_context(|| format!("writing the database snapshot to {target}"))?;
        let published = self.0.shared.publish(&outgoing, held.epoch).await;
        let _ = tokio::fs::remove_file(&outgoing).await;
        let manifest = published?;
        *last = Some(stamp);
        self.status(|status| {
            status.replica_taken_at_ms = Some(manifest.taken_at_ms);
            status.replica_from = Some(manifest.node.clone());
            status.replica_error = None;
        });
        Ok(Some(manifest))
    }

    /// Arrêt propre : dernier instantané, puis bail rendu — le serveur de rôle
    /// `primary` le reprend aussitôt, l'autre attend son délai.
    pub async fn shutdown(&self, pool: &SqlitePool) {
        match tokio::time::timeout(FINAL_SNAPSHOT_TIMEOUT, self.snapshot(pool)).await {
            Ok(Ok(_)) => {}
            Ok(Err(cause)) => {
                let error = format!("{cause:#}");
                warn!(error, "high availability: last replica not published");
            }
            Err(_) => warn!("high availability: last replica timed out"),
        }
        let held = self.0.lease.lock().await;
        self.0.stopping.store(true, Ordering::SeqCst);
        // Rendu seulement s'il est encore le nôtre.
        if let Ok(Some(current)) = self.0.shared.read_lease().await
            && same_hold(&current, &held)
        {
            let released = Lease { released: true, renewed_at_ms: now_ms(), ..held.clone() };
            match self.0.shared.write_lease(&released).await {
                Ok(()) => info!("high availability: lease released"),
                Err(cause) => {
                    let error = format!("{cause:#}");
                    warn!(error, "high availability: lease not released");
                }
            }
        }
    }

    fn status(&self, change: impl FnOnce(&mut Status)) {
        let mut status = self.0.status.lock().unwrap_or_else(PoisonError::into_inner);
        change(&mut status);
    }

    async fn write_status(&self) {
        let status = {
            let mut status = self.0.status.lock().unwrap_or_else(PoisonError::into_inner);
            status.updated_at_ms = now_ms();
            status.clone()
        };
        standby::write_status(&self.0.work, &status).await;
    }
}

/// Même détenteur, même processus, même époque.
fn same_hold(current: &Lease, held: &Lease) -> bool {
    current.holder == held.holder
        && current.instance == held.instance
        && current.epoch == held.epoch
}

/// Ce serveur n'est plus sûr d'être le seul actif : il s'arrête net. Le
/// conteneur redémarre et le serveur repart en attente.
fn fence(reason: &str) -> ! {
    error!(
        reason,
        "high availability: this server is no longer the active one, stopping now (it restarts \
         as the standby)"
    );
    std::process::exit(EXIT_FENCED);
}

async fn stamp(database: &Path) -> Stamp {
    let wal = {
        let mut name = database.as_os_str().to_owned();
        name.push("-wal");
        PathBuf::from(name)
    };
    let mut stamp = Vec::with_capacity(2);
    for path in [database.to_path_buf(), wal] {
        let meta = tokio::fs::metadata(&path).await.ok();
        stamp.push(meta.and_then(|meta| Some((meta.modified().ok()?, meta.len()))));
    }
    stamp
}

#[cfg(test)]
mod tests {
    use super::super::{Role, TEST_SECRET as SECRET, test_ha as ha};
    use super::*;

    #[tokio::test]
    async fn the_old_active_sees_the_lease_taken_and_publishes_nothing_more() {
        let root = tempfile::tempdir().unwrap();
        let shared_dir = root.path().join("shared");
        let config = ha(&shared_dir, "a", Role::Primary);
        let shared = Shared::open(&config).await.unwrap();
        let lease = Lease::new("a", "process-a", 1);
        shared.write_lease(&lease).await.unwrap();
        let work = root.path().join("a");
        std::fs::create_dir_all(&work).unwrap();
        let database = work.join("dumbmonit.db");
        let pool = crate::db::open_with(&database, 2).await.unwrap();
        crate::db::init_cipher(&pool, SECRET).await.unwrap();
        let active = Active::new(config, shared, lease.clone(), work.clone(), database);

        assert!(matches!(active.renew().await, Renewal::Renewed));
        assert!(matches!(active.renew().await, Renewal::Renewed));
        let renewed = Shared::at(&shared_dir, "b").read_lease().await.unwrap().unwrap();
        assert_eq!(renewed.beat, 2, "le bail bouge à chaque battement");

        let first = active.snapshot(&pool).await.unwrap().expect("premier instantané");
        assert_eq!((first.node.as_str(), first.epoch), ("a", 1));
        assert!(active.snapshot(&pool).await.unwrap().is_none(), "rien de neuf, rien de publié");

        // Le secours prend le bail (époque suivante) : l'ancien actif le voit
        // au battement suivant, et ne publie plus rien.
        let taken = Lease::new("b", "process-b", 2);
        Shared::at(&shared_dir, "b").write_lease(&taken).await.unwrap();
        assert!(matches!(active.renew().await, Renewal::Lost(_)));
        sqlx::query("INSERT INTO settings (key, value) VALUES ('ha-test', '1')")
            .execute(&pool)
            .await
            .unwrap();
        assert!(active.snapshot(&pool).await.is_err());
        let manifest = Shared::at(&shared_dir, "b").read_manifest().await.unwrap().unwrap();
        assert_eq!(manifest, first);

        // Le même nom sous un autre processus : perdu aussi.
        let impostor = Lease::new("a", "process-c", 1);
        Shared::at(&shared_dir, "a").write_lease(&impostor).await.unwrap();
        assert!(matches!(active.renew().await, Renewal::Lost(_)));
    }

    #[tokio::test]
    async fn a_clean_stop_publishes_a_last_replica_and_releases_the_lease() {
        let root = tempfile::tempdir().unwrap();
        let shared_dir = root.path().join("shared");
        let config = ha(&shared_dir, "a", Role::Primary);
        let shared = Shared::open(&config).await.unwrap();
        let lease = Lease::new("a", "process-a", 4);
        shared.write_lease(&lease).await.unwrap();
        let work = root.path().join("a");
        std::fs::create_dir_all(&work).unwrap();
        let database = work.join("dumbmonit.db");
        let pool = crate::db::open_with(&database, 2).await.unwrap();
        crate::db::init_cipher(&pool, SECRET).await.unwrap();
        let active = Active::new(config, shared, lease, work, database);

        active.shutdown(&pool).await;
        let reader = Shared::at(&shared_dir, "b");
        let released = reader.read_lease().await.unwrap().unwrap();
        assert!(released.released);
        assert_eq!((released.holder.as_str(), released.epoch), ("a", 4));
        let manifest = reader.read_manifest().await.unwrap().expect("dernier instantané");
        assert_eq!(manifest.epoch, 4);
        // Arrêté : plus aucun renouvellement ne rend le bail « tenu ».
        assert!(matches!(active.renew().await, Renewal::Renewed));
        assert!(reader.read_lease().await.unwrap().unwrap().released);
    }
}
