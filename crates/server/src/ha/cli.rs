//! `dumbmonit ha status|promote [--force]` : la haute disponibilité vue et
//! pilotée depuis le conteneur (`docker compose exec dumbmonit /dumbmonit ha
//! …`). Ni base ni réseau : des fichiers, sous `/data/ha` et dans le
//! répertoire partagé.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{
    HaConfig, Lease, Manifest, PROMOTE_FILE, PromoteRequest, STATE_FILE, Status, WORK_DIR,
};

/// Attente maximale du résultat d'une promotion : le relevé suivant, le
/// délai d'observation du bail et sa relecture tiennent largement dedans.
const PROMOTE_WAIT: Duration = Duration::from_secs(120);

/// Un serveur en attente qui n'a pas écrit son état depuis plus longtemps
/// est arrêté ou bloqué : la sonde de santé ne le dit plus sain.
const STATE_FRESH_MS: i64 = 90_000;

const USAGE: &str = "usage: dumbmonit ha status | dumbmonit ha promote [--force]";

/// Point d'entrée ; rend le code de sortie.
pub fn run(args: &[String]) -> i32 {
    let ha = match HaConfig::from_env() {
        Ok(Some(ha)) => ha,
        Ok(None) => {
            eprintln!("high availability is off: DUMBMONIT_HA_DIR is not set");
            return 1;
        }
        Err(error) => {
            eprintln!("invalid high availability settings: {error:#}");
            return 1;
        }
    };
    let work = data_dir().join(WORK_DIR);
    match args.first().map(String::as_str) {
        Some("status") => status(&ha, &work),
        Some("promote") => promote(&work, args[1..].iter().any(|arg| arg == "--force")),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    }
}

/// Vrai pour un serveur en attente bien vivant : la sonde de santé du
/// conteneur le dit sain, alors qu'aucun port n'est ouvert.
pub fn standby_is_healthy() -> bool {
    let Ok(Some(_)) = HaConfig::from_env() else { return false };
    let Some(status) = read::<Status>(&data_dir().join(WORK_DIR).join(STATE_FILE)) else {
        return false;
    };
    matches!(status.role.as_str(), "standby" | "promoting")
        && now_ms().saturating_sub(status.updated_at_ms) < STATE_FRESH_MS
}

fn data_dir() -> PathBuf {
    PathBuf::from(
        crate::config::env_var("DUMBMONIT_DATA_DIR")
            .map(|dir| dir.trim().to_string())
            .filter(|dir| !dir.is_empty())
            .unwrap_or_else(|| "/data".into()),
    )
}

fn status(ha: &HaConfig, work: &Path) -> i32 {
    println!("node      {} (role {:?})", ha.node, ha.role);
    match &ha.takeover_after {
        Some(after) => println!("takeover  automatic after {} min", after.as_secs() / 60),
        None => println!("takeover  manual only (dumbmonit ha promote)"),
    }
    match read::<Status>(&work.join(STATE_FILE)) {
        Some(state) => {
            println!("state     {} (updated {})", state.role, ago(state.updated_at_ms));
            if let Some(holder) = &state.holder {
                let alive = match state.holder_alive {
                    Some(true) => ", renewing it",
                    Some(false) => ", not renewing it",
                    None => "",
                };
                println!("holder    {holder}{alive}");
            }
            if let Some(at) = state.replica_taken_at_ms {
                let from = state.replica_from.as_deref().unwrap_or("?");
                println!("replica   from {from}, taken {}", ago(at));
            }
            if let Some(error) = &state.replica_error {
                println!("problem   {error}");
            }
            if let Some(promotion) = &state.promotion {
                println!("promotion {promotion}");
            }
        }
        None => println!("state     unknown (is the server running?)"),
    }
    match read::<Lease>(&ha.dir.join("lease.json")) {
        Some(lease) => println!(
            "lease     held by {}, epoch {}, beat {}{}, renewed {}",
            lease.holder,
            lease.epoch,
            lease.beat,
            if lease.released { ", released" } else { "" },
            ago(lease.renewed_at_ms)
        ),
        None => println!("lease     none"),
    }
    match read::<Manifest>(&ha.dir.join("snapshot.json")) {
        Some(manifest) => println!(
            "snapshot  {} from {} (version {}), {} bytes, taken {}",
            manifest.file,
            manifest.node,
            manifest.version,
            manifest.size,
            ago(manifest.taken_at_ms)
        ),
        None => println!("snapshot  none"),
    }
    0
}

fn promote(work: &Path, force: bool) -> i32 {
    let state = read::<Status>(&work.join(STATE_FILE));
    if state.as_ref().is_some_and(|state| state.role == "active") {
        println!("this server is already the active one");
        return 0;
    }
    let request = PromoteRequest { force, requested_at_ms: now_ms() };
    let written = serde_json::to_vec(&request)
        .map_err(std::io::Error::other)
        .and_then(|bytes| write_atomic(&work.join(PROMOTE_FILE), &bytes));
    if let Err(error) = written {
        eprintln!("cannot write the promotion request in {}: {error}", work.display());
        return 1;
    }
    println!("promotion requested, waiting for the standby to take over…");
    let started = std::time::Instant::now();
    while started.elapsed() < PROMOTE_WAIT {
        std::thread::sleep(Duration::from_secs(1));
        let Some(state) = read::<Status>(&work.join(STATE_FILE)) else { continue };
        if state.role == "active" && state.updated_at_ms >= request.requested_at_ms {
            println!("this server is now the active one (epoch {})", state.epoch.unwrap_or(0));
            return 0;
        }
        // Seul un refus de cette demande-ci compte, pas celui d'une précédente.
        if state.promotion_for_ms == Some(request.requested_at_ms)
            && let Some(refused) = state.promotion.as_deref().filter(|p| p.starts_with("refused"))
        {
            eprintln!("promotion {refused}");
            return 1;
        }
    }
    println!("still waiting: see `dumbmonit ha status` and the server log");
    3
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    let tmp = path.with_extension("json.tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// « il y a 12 s », lisible dans un terminal.
fn ago(ms: i64) -> String {
    let seconds = now_ms().saturating_sub(ms) / 1000;
    let at = chrono::DateTime::from_timestamp_millis(ms)
        .map_or_else(|| "?".to_string(), |at| at.format("%Y-%m-%d %H:%M:%S UTC").to_string());
    format!("{at} ({seconds} s ago)")
}
