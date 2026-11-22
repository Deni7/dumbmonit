//! Envoi d'une sauvegarde locale vers les destinations, élagage, essai.

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::SqlitePool;

use super::store::{self, Destination, LastResult};
use super::transfer::Failure;
use super::{Secrets, Settings, TEST_CONTENT, is_key_name, key_name, s3, sftp, webdav};
use crate::backup::local::{self, RunRecord};
use crate::crypto::Cipher;

/// Envois lancés et pas encore terminés : l'interface rafraîchit tant qu'il y
/// en a.
static PENDING: AtomicUsize = AtomicUsize::new(0);
/// Un seul envoi à la fois : une sauvegarde lente ne se fait pas doubler par
/// la suivante, qui attend son tour.
static LOCK: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// Vrai pendant qu'un envoi est en cours ou en attente.
pub fn uploading() -> bool {
    PENDING.load(Ordering::SeqCst) > 0
}

struct Pending;

impl Pending {
    fn start() -> Self {
        PENDING.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Nouvelles tentatives d'un envoi qui a échoué pour une raison passagère.
#[derive(Debug, Clone, Copy)]
pub struct Retry {
    pub attempts: u32,
    /// Attente avant la deuxième tentative, triplée à chaque fois.
    pub delay: Duration,
}

impl Default for Retry {
    fn default() -> Self {
        Self { attempts: 3, delay: Duration::from_secs(10) }
    }
}

impl Retry {
    /// Une seule tentative (tests, essai d'une destination).
    pub fn once() -> Self {
        Self { attempts: 1, delay: Duration::ZERO }
    }

    fn wait(&self, attempt: u32) -> Duration {
        self.delay.saturating_mul(3u32.saturating_pow(attempt.saturating_sub(1)))
    }
}

/// À appeler après chaque sauvegarde locale : lance l'envoi en tâche de
/// fond, sans attendre. Une sauvegarde ratée ne part pas.
pub async fn after_local_backup(
    state: &crate::state::AppState,
    settings: &local::Settings,
    record: &RunRecord,
) {
    if !record.ok || record.file.is_empty() {
        return;
    }
    match store::count_enabled(&state.pool).await {
        Ok(0) => return,
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(%error, "off-site destinations unreadable, nothing uploaded");
            return;
        }
    }
    // Compté avant de rendre la main : l'interface qui relit l'état juste
    // après « Sauvegarder maintenant » voit l'envoi en cours.
    let pending = Pending::start();
    let state = state.clone();
    let directory = settings.directory.clone();
    let record = record.clone();
    tokio::spawn(async move {
        let _pending = pending;
        push_all(&state.pool, &state.cipher, &directory, &record, Retry::default()).await;
    });
}

/// Envoie la sauvegarde `record` (fichiers de `directory`) vers chaque
/// destination active, élague, et consigne chaque résultat. Rend les
/// résultats par nom de destination.
pub async fn push_all(
    pool: &SqlitePool,
    cipher: &Cipher,
    directory: &Path,
    record: &RunRecord,
    retry: Retry,
) -> Vec<(String, LastResult)> {
    let _turn = LOCK.lock().await;
    let destinations = match store::list(pool).await {
        Ok(list) => list,
        Err(error) => {
            tracing::warn!(%error, "off-site destinations unreadable, nothing uploaded");
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    for destination in destinations.into_iter().filter(|destination| destination.enabled) {
        let result = push_one(pool, cipher, &destination, directory, record, retry).await;
        if result.ok {
            tracing::info!(
                destination = %destination.name,
                file = %result.file,
                bytes = result.bytes,
                "backup copied off-site"
            );
        } else {
            tracing::warn!(
                destination = %destination.name,
                error = result.error.as_deref().unwrap_or(""),
                "off-site backup copy failed"
            );
        }
        if let Err(error) = store::record_result(pool, destination.id, &result).await {
            tracing::warn!(%error, "could not record the off-site upload");
        }
        out.push((destination.name.clone(), result));
    }
    out
}

async fn push_one(
    pool: &SqlitePool,
    cipher: &Cipher,
    destination: &Destination,
    directory: &Path,
    record: &RunRecord,
    retry: Retry,
) -> LastResult {
    let started = Instant::now();
    let at = crate::db::now();
    let outcome = upload(pool, cipher, destination, directory, record, retry).await;
    let duration_ms = started.elapsed().as_millis() as i64;
    match outcome {
        Ok((bytes, note)) => LastResult {
            at,
            ok: true,
            file: record.file.clone(),
            bytes: bytes as i64,
            duration_ms,
            error: note,
        },
        Err(failure) => LastResult {
            at,
            ok: false,
            file: record.file.clone(),
            bytes: 0,
            duration_ms,
            error: Some(failure.message),
        },
    }
}

/// Envoie la base (et sa clé si demandé), puis élague. Rend les octets
/// envoyés et, si l'élagage a échoué, une remarque.
async fn upload(
    pool: &SqlitePool,
    cipher: &Cipher,
    destination: &Destination,
    directory: &Path,
    record: &RunRecord,
    retry: Retry,
) -> Result<(u64, Option<String>), Failure> {
    let secrets = store::secrets(pool, cipher, destination.id).await.map_err(|error| {
        Failure::fatal(format!("the saved credentials cannot be read: {error:#}"))
    })?;
    let mut files = vec![record.file.clone()];
    // `secret.key` ne part que sur demande expresse pour cette destination.
    if destination.include_key
        && record.with_secret
        && let Some(key) = key_name(&record.file)
    {
        files.push(key);
    }

    let mut remote = Remote::new(&destination.settings, &secrets)?;
    let mut total = 0;
    for name in &files {
        match put_with_retry(&mut remote, name, &directory.join(name), retry).await {
            Ok(bytes) => total += bytes,
            Err(failure) => {
                remote.close().await;
                return Err(failure);
            }
        }
    }
    let note = match prune(&mut remote, destination.keep as usize).await {
        Ok(_) => None,
        Err(failure) => {
            Some(format!("Uploaded, but the older copies could not be removed: {failure}"))
        }
    };
    remote.close().await;
    Ok((total, note))
}

async fn put_with_retry(
    remote: &mut Remote,
    name: &str,
    path: &Path,
    retry: Retry,
) -> Result<u64, Failure> {
    let mut attempt = 1;
    loop {
        match remote.put_file(name, path).await {
            Ok(bytes) => return Ok(bytes),
            Err(failure) if failure.retry && attempt < retry.attempts => {
                tracing::info!(%failure, attempt, "off-site upload failed, trying again");
                remote.reset();
                tokio::time::sleep(retry.wait(attempt)).await;
                attempt += 1;
            }
            Err(failure) => return Err(failure),
        }
    }
}

async fn prune(remote: &mut Remote, keep: usize) -> Result<usize, Failure> {
    let names = remote.list().await?;
    let doomed = plan_prune(&names, keep);
    for name in &doomed {
        remote.delete(name).await?;
    }
    Ok(doomed.len())
}

/// Ce que l'élagage supprime d'une liste distante : les bases au-delà des
/// `keep` plus récentes, et les clés qui n'accompagnent plus une base gardée.
/// Rien d'autre que nos noms exacts n'est jamais proposé.
pub fn plan_prune(names: &[String], keep: usize) -> Vec<String> {
    let mut databases: Vec<&str> =
        names.iter().map(String::as_str).filter(|name| local::is_backup_name(name)).collect();
    // L'horodatage est dans le nom : l'ordre alphabétique est chronologique.
    databases.sort_unstable_by(|a, b| b.cmp(a));
    databases.dedup();
    let keep = keep.max(1);
    let kept: BTreeSet<String> =
        databases.iter().take(keep).filter_map(|db| key_name(db)).collect();
    let mut doomed: Vec<String> = databases.iter().skip(keep).map(|db| db.to_string()).collect();
    let mut keys: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| is_key_name(name) && !kept.contains(*name))
        .collect();
    keys.sort_unstable();
    keys.dedup();
    doomed.extend(keys.into_iter().map(str::to_string));
    doomed
}

/// Résultat d'un essai de destination.
#[derive(Debug, Clone, Serialize)]
pub struct TestOutcome {
    pub ok: bool,
    pub message: String,
    pub duration_ms: i64,
    /// Empreinte de la clé d'hôte SFTP présentée quand elle n'est pas (ou
    /// plus) celle confirmée : à comparer, puis à confirmer.
    pub host_key: Option<String>,
    /// Vrai : une autre empreinte était confirmée (serveur réinstallé, ou
    /// imposteur).
    pub host_key_changed: bool,
}

/// Écrit un petit fichier, vérifie qu'il apparaît dans la liste (l'élagage
/// en dépend), puis le supprime.
pub async fn test_destination(settings: &Settings, secrets: &Secrets) -> TestOutcome {
    let started = Instant::now();
    let result = test_inner(settings, secrets).await;
    let duration_ms = started.elapsed().as_millis() as i64;
    match result {
        Ok(()) => TestOutcome {
            ok: true,
            message: "A test file was written, listed and deleted.".into(),
            duration_ms,
            host_key: None,
            host_key_changed: false,
        },
        Err(failure) => TestOutcome {
            ok: false,
            message: failure.message,
            duration_ms,
            host_key_changed: failure.host_key.as_ref().is_some_and(|key| key.changed),
            host_key: failure.host_key.map(|key| key.fingerprint),
        },
    }
}

async fn test_inner(settings: &Settings, secrets: &Secrets) -> Result<(), Failure> {
    let name = super::test_file_name();
    let mut remote = Remote::new(settings, secrets)?;
    if let Err(failure) = remote.put_bytes(&name, TEST_CONTENT.to_vec()).await {
        remote.close().await;
        return Err(failure);
    }
    let listed = remote.list().await;
    let deleted = remote.delete(&name).await;
    remote.close().await;
    if !listed?.contains(&name) {
        return Err(Failure::fatal(
            "The test file was written but is missing from the listing, so old copies could not \
             be pruned. Check the prefix or the folder.",
        ));
    }
    deleted
}

/// Une destination ouverte, quelle que soit sa sorte.
enum Remote {
    S3(s3::Client),
    WebDav(webdav::Client),
    Sftp(sftp::Client),
}

impl Remote {
    fn new(settings: &Settings, secrets: &Secrets) -> Result<Self, Failure> {
        Ok(match settings {
            Settings::S3(s) => {
                let secret = secrets.get("secret_access_key").cloned().unwrap_or_default();
                Self::S3(s3::Client::new(s.clone(), secret)?)
            }
            Settings::Webdav(s) => {
                Self::WebDav(webdav::Client::new(s.clone(), secrets.get("password").cloned())?)
            }
            Settings::Sftp(s) => {
                Self::Sftp(sftp::Client::new(s.clone(), sftp::Credentials::from_secrets(secrets)))
            }
        })
    }

    async fn put_file(&mut self, name: &str, path: &Path) -> Result<u64, Failure> {
        match self {
            Self::S3(client) => client.put_file(name, path).await,
            Self::WebDav(client) => client.put_file(name, path).await,
            Self::Sftp(client) => client.put_file(name, path).await,
        }
    }

    async fn put_bytes(&mut self, name: &str, data: Vec<u8>) -> Result<(), Failure> {
        match self {
            Self::S3(client) => client.put_bytes(name, data).await,
            Self::WebDav(client) => client.put_bytes(name, data).await,
            Self::Sftp(client) => client.put_bytes(name, data).await,
        }
    }

    async fn list(&mut self) -> Result<Vec<String>, Failure> {
        match self {
            Self::S3(client) => client.list().await,
            Self::WebDav(client) => client.list().await,
            Self::Sftp(client) => client.list().await,
        }
    }

    async fn delete(&mut self, name: &str) -> Result<(), Failure> {
        match self {
            Self::S3(client) => client.delete(name).await,
            Self::WebDav(client) => client.delete(name).await,
            Self::Sftp(client) => client.delete(name).await,
        }
    }

    fn reset(&mut self) {
        if let Self::Sftp(client) = self {
            client.reset();
        }
    }

    async fn close(self) {
        if let Self::Sftp(client) = self {
            client.close().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn pruning_keeps_the_newest_and_their_keys() {
        let remote = names(&[
            "dumbmonit-20260920-020000.db",
            "dumbmonit-20260920-020000.key",
            "dumbmonit-20260922-020000.db",
            "dumbmonit-20260922-020000.key",
            "dumbmonit-20260921-020000.db",
            "notes.txt",
            "dumbmonit-backup-20260101-000000.json",
            "dumbmonit-20260919-020000.key",
            "dumbmonit-20260919-020000.db.part",
            "secret.key",
        ]);
        assert_eq!(
            plan_prune(&remote, 2),
            names(&[
                "dumbmonit-20260920-020000.db",
                "dumbmonit-20260919-020000.key",
                "dumbmonit-20260920-020000.key",
            ])
        );
    }

    #[test]
    fn pruning_never_goes_below_one_copy_nor_touches_foreign_files() {
        let remote = names(&["dumbmonit-20260922-020000.db", "other.db", "x.key"]);
        assert!(plan_prune(&remote, 0).is_empty());
        assert!(plan_prune(&[], 3).is_empty());
        let many: Vec<String> =
            (10..20).map(|day| format!("dumbmonit-202609{day}-020000.db")).collect();
        let doomed = plan_prune(&many, 7);
        assert_eq!(
            doomed,
            names(&[
                "dumbmonit-20260912-020000.db",
                "dumbmonit-20260911-020000.db",
                "dumbmonit-20260910-020000.db",
            ])
        );
    }

    #[test]
    fn retries_wait_longer_each_time() {
        let retry = Retry { attempts: 3, delay: Duration::from_secs(10) };
        assert_eq!(retry.wait(1), Duration::from_secs(10));
        assert_eq!(retry.wait(2), Duration::from_secs(30));
        assert_eq!(Retry::once().wait(1), Duration::ZERO);
    }
}
