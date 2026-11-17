//! Le répertoire partagé : bail, instantanés de la base et leur manifeste.
//!
//! Tout y est écrit d'un coup (fichier temporaire synchronisé, puis renommé) :
//! un lecteur voit l'ancienne version ou la nouvelle, jamais un morceau. Un
//! instantané porte un nom unique, et le manifeste n'est réécrit qu'une fois
//! le fichier complet : le serveur en attente ne copie jamais une base à
//! moitié écrite, et vérifie de toute façon son empreinte.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{HaConfig, Lease, Manifest, now_ms, write_atomic};

const LEASE_FILE: &str = "lease.json";
const MANIFEST_FILE: &str = "snapshot.json";
const SNAPSHOT_PREFIX: &str = "snapshot-";
const SNAPSHOT_SUFFIX: &str = ".db";

/// Instantanés gardés : le dernier, et le précédent pour un lecteur qui lisait
/// encore l'ancien manifeste.
const KEEP_SNAPSHOTS: usize = 2;

/// Taille des morceaux copiés.
const CHUNK: usize = 64 * 1024;

pub(super) struct Shared {
    dir: PathBuf,
    node: String,
}

impl Shared {
    /// Ouvre le répertoire partagé (créé s'il manque) et vérifie qu'on peut y
    /// écrire : sans cela, ni bail ni réplique.
    pub(super) async fn open(ha: &HaConfig) -> Result<Self> {
        tokio::fs::create_dir_all(&ha.dir).await.with_context(|| {
            format!("creating the shared directory {} (DUMBMONIT_HA_DIR)", ha.dir.display())
        })?;
        let shared = Self { dir: ha.dir.clone(), node: ha.node.clone() };
        let probe = shared.dir.join(format!(".write-test-{}", ha.node));
        write_atomic(&probe, b"").await.with_context(|| {
            format!(
                "the shared directory {} is not writable by the server (uid 65532 unless PUID \
                 is set)",
                ha.dir.display()
            )
        })?;
        let _ = tokio::fs::remove_file(&probe).await;
        Ok(shared)
    }

    #[cfg(test)]
    pub(super) fn at(dir: &Path, node: &str) -> Self {
        Self { dir: dir.to_path_buf(), node: node.to_string() }
    }

    pub(super) async fn read_lease(&self) -> Result<Option<Lease>> {
        read_json(&self.dir.join(LEASE_FILE)).await
    }

    pub(super) async fn write_lease(&self, lease: &Lease) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(lease).context("encoding the lease")?;
        write_atomic(&self.dir.join(LEASE_FILE), &bytes).await
    }

    pub(super) async fn read_manifest(&self) -> Result<Option<Manifest>> {
        read_json(&self.dir.join(MANIFEST_FILE)).await
    }

    /// Publie une copie de `source` (une base complète) : fichier au nom
    /// unique, puis manifeste, puis ménage des anciens instantanés.
    pub(super) async fn publish(&self, source: &Path, epoch: u64) -> Result<Manifest> {
        let taken_at_ms = now_ms();
        let file = format!("{SNAPSHOT_PREFIX}{taken_at_ms}-{}{SNAPSHOT_SUFFIX}", self.node);
        let target = self.dir.join(&file);
        let tmp = self.dir.join(format!(".{file}.tmp"));
        let copied = copy_hashed(source, &tmp).await;
        let (size, sha256) = match copied {
            Ok(done) => done,
            Err(error) => {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(error);
            }
        };
        tokio::fs::rename(&tmp, &target)
            .await
            .with_context(|| format!("renaming {}", tmp.display()))?;
        let manifest = Manifest {
            file,
            node: self.node.clone(),
            epoch,
            taken_at_ms,
            size,
            sha256,
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest).context("encoding the manifest")?;
        write_atomic(&self.dir.join(MANIFEST_FILE), &bytes).await?;
        self.prune(&manifest.file).await;
        Ok(manifest)
    }

    /// Supprime les instantanés au-delà des [`KEEP_SNAPSHOTS`] plus récents,
    /// jamais celui du manifeste.
    async fn prune(&self, current: &str) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await else { return };
        let mut snapshots: Vec<String> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name().to_string_lossy().to_string();
            if is_snapshot_name(&name) {
                snapshots.push(name);
            }
        }
        // Le nom commence par l'instant de l'instantané, en millisecondes :
        // l'ordre des noms est celui du temps.
        snapshots.sort();
        let excess = snapshots.len().saturating_sub(KEEP_SNAPSHOTS);
        for name in snapshots.into_iter().take(excess) {
            if name != current {
                let _ = tokio::fs::remove_file(self.dir.join(&name)).await;
            }
        }
    }

    /// Copie l'instantané du manifeste vers `dest`, en vérifiant sa taille et
    /// son empreinte. `dest` n'est remplacé qu'une fois la copie juste.
    pub(super) async fn fetch(&self, manifest: &Manifest, dest: &Path) -> Result<()> {
        if !is_snapshot_name(&manifest.file) {
            bail!("unexpected snapshot name in the manifest: '{}'", manifest.file);
        }
        let source = self.dir.join(&manifest.file);
        let name = dest.file_name().and_then(|name| name.to_str()).unwrap_or("replica.db");
        let tmp = dest.with_file_name(format!(".{name}.tmp"));
        let copied = copy_hashed(&source, &tmp).await;
        let checked = match copied {
            Ok((size, sha256)) if size == manifest.size && sha256 == manifest.sha256 => Ok(()),
            Ok(_) => Err(anyhow::anyhow!(
                "the snapshot {} does not match its manifest (size or SHA-256)",
                manifest.file
            )),
            Err(error) => Err(error),
        };
        if let Err(error) = checked {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(error);
        }
        tokio::fs::rename(&tmp, dest).await.with_context(|| format!("renaming {}", tmp.display()))
    }
}

/// `snapshot-<ms>-<nom>.db`, rien d'autre : le manifeste vient d'un stockage
/// partagé, son nom de fichier ne doit mener nulle part ailleurs.
fn is_snapshot_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(SNAPSHOT_PREFIX) else { return false };
    let Some(rest) = rest.strip_suffix(SNAPSHOT_SUFFIX) else { return false };
    let Some((stamp, node)) = rest.split_once('-') else { return false };
    !stamp.is_empty() && stamp.bytes().all(|b| b.is_ascii_digit()) && super::is_node_name(node)
}

/// Lit un fichier JSON ; `None` s'il n'existe pas.
async fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .with_context(|| format!("unreadable {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(anyhow::Error::from(error).context(format!("reading {}", path.display())))
        }
    }
}

/// Copie `source` vers `dest` (synchronisé) ; rend la taille et l'empreinte
/// SHA-256 de ce qui a été copié.
async fn copy_hashed(source: &Path, dest: &Path) -> Result<(u64, String)> {
    let mut input = tokio::fs::File::open(source)
        .await
        .with_context(|| format!("opening {}", source.display()))?;
    let mut output = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("creating {}", dest.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut size: u64 = 0;
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .with_context(|| format!("reading {}", source.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .await
            .with_context(|| format!("writing {}", dest.display()))?;
        size += read as u64;
    }
    output.sync_all().await.with_context(|| format!("syncing {}", dest.display()))?;
    Ok((size, hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_snapshot_names_are_followed() {
        assert!(is_snapshot_name("snapshot-1700000000000-a.db"));
        assert!(is_snapshot_name("snapshot-1700000000000-node_2.db"));
        assert!(!is_snapshot_name("snapshot-1700000000000-../../etc/passwd.db"));
        assert!(!is_snapshot_name("snapshot--a.db"));
        assert!(!is_snapshot_name("snapshot-17x-a.db"));
        assert!(!is_snapshot_name("lease.json"));
    }

    #[tokio::test]
    async fn a_snapshot_round_trips_and_old_ones_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        let shared = Shared::at(dir.path(), "a");
        let source = dir.path().join("source.db");
        tokio::fs::write(&source, b"une base").await.unwrap();
        let mut last = None;
        for _ in 0..4 {
            last = Some(shared.publish(&source, 2).await.unwrap());
            // Des noms distincts : l'instant change d'une milliseconde au moins.
            tokio::time::sleep(std::time::Duration::from_millis(3)).await;
        }
        let manifest = last.unwrap();
        assert_eq!(shared.read_manifest().await.unwrap().as_ref(), Some(&manifest));
        assert_eq!((manifest.node.as_str(), manifest.epoch, manifest.size), ("a", 2, 8));
        let mut kept = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .filter(|name| is_snapshot_name(name))
            .collect::<Vec<_>>();
        kept.sort();
        assert_eq!(kept.len(), KEEP_SNAPSHOTS);
        assert_eq!(kept.last(), Some(&manifest.file));

        let dest = dir.path().join("replica.db");
        shared.fetch(&manifest, &dest).await.unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"une base");

        // Un fichier qui ne correspond plus à son manifeste n'est pas adopté.
        std::fs::write(dir.path().join(&manifest.file), b"autre chose").unwrap();
        std::fs::write(&dest, b"ancienne").unwrap();
        assert!(shared.fetch(&manifest, &dest).await.is_err());
        assert_eq!(std::fs::read(&dest).unwrap(), b"ancienne");
    }

    #[tokio::test]
    async fn the_lease_is_absent_then_written_whole() {
        let dir = tempfile::tempdir().unwrap();
        let shared = Shared::at(dir.path(), "a");
        assert_eq!(shared.read_lease().await.unwrap(), None);
        let lease = Lease::new("a", "x", 1);
        shared.write_lease(&lease).await.unwrap();
        assert_eq!(shared.read_lease().await.unwrap(), Some(lease));
        std::fs::write(dir.path().join(LEASE_FILE), b"{pas du json").unwrap();
        assert!(shared.read_lease().await.is_err(), "illisible n'est pas absent");
    }
}
