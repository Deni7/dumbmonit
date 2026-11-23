//! Les sauvegardes, une ligne par tâche ou par appareil, d'après ce que les
//! collecteurs publient déjà :
//!
//! - les séries « âge de la dernière réussite » des familles de sauvegarde
//!   (celles des échéances, [`crate::api::deadlines::BACKUP_FAMILIES`]) :
//!   Proxmox VE, PBS, Plakar, restic/borg, Duplicati, Kopia, Hyper Backup,
//!   Veeam… L'historique des tâches PBS, gardé en base, y ajoute les dernières
//!   exécutions ;
//! - l'historique Active Backup for Business, gardé en base par appareil.

use std::collections::{BTreeMap, HashMap};

use chrono::DateTime;

use dumbmonit_collectors::synology::DeviceRun as AbbRun;

use crate::alerting::model::TargetId;
use crate::api::deadlines::BACKUP_FAMILIES;
use crate::db;
use crate::state::AppState;
use crate::tsdb::METRIC_PREFIX;

use super::model::{BackupJob, BackupRun, BackupStatus};
use super::{Context, round, stamp, vm};

const DAY: f64 = 86_400.0;
/// Profondeur de recherche du dernier point, comme les échéances.
const LOOKBACK_SECONDS: i64 = 3 * 86_400;
/// Historique relu en base.
const HISTORY_DAYS: i64 = 30;
/// Exécutions rendues par ligne, au plus.
const MAX_RUNS: usize = 14;
/// Lignes rendues, au plus.
const MAX_JOBS: usize = 200;
/// Retard d'un appareil Active Backup : celui des règles livrées.
const ABB_LIMIT_DAYS: f64 = 2.0;
/// Série « tâche Hyper Backup en cours », mêmes étiquettes que son âge.
const HYPER_BACKUP_RUNNING: &str = "synology_backup_running";

pub async fn load(state: &AppState, ctx: &Context) -> Vec<BackupJob> {
    if ctx.fleet.monitors.is_empty() {
        return Vec::new();
    }
    let (mut jobs, abb) = tokio::join!(metric_jobs(state, ctx), abb_jobs(state, ctx));
    jobs.extend(abb);
    jobs.sort_by(|a, b| a.status.cmp(&b.status).then_with(|| a.job.cmp(&b.job)));
    jobs.truncate(MAX_JOBS);
    jobs
}

/// Nom du logiciel de sauvegarde d'une famille.
fn engine(metric: &str, labels: &BTreeMap<String, String>) -> String {
    match metric {
        "proxmox_backup_last_age_seconds" => "Proxmox VE",
        "pbs_backup_last_age_seconds" => "PBS",
        "backup_last_success_seconds" => "Plakar",
        "agent_backup_repo_last_snapshot_age_seconds" => {
            return labels
                .get("tool")
                .filter(|tool| !tool.is_empty())
                .cloned()
                .unwrap_or_else(|| "Backup repository".to_string());
        }
        "duplicati_backup_last_success_age_seconds" => "Duplicati",
        "kopia_source_last_snapshot_age_seconds" => "Kopia",
        "synology_backup_last_success_age_seconds" => "Hyper Backup",
        "veeam_config_backup_last_success_age_seconds" => "Veeam",
        "hetzner_server_backup_age_seconds" => "Hetzner Cloud",
        "truenas_backup_task_last_run_age_seconds" => "TrueNAS",
        "immich_db_backup_last_age_seconds" => "Immich",
        _ => "Backup",
    }
    .to_string()
}

/// Étiquettes d'une série sans son nom : ce qui rapproche deux séries d'une
/// même tâche.
fn identity(labels: &BTreeMap<String, String>) -> Vec<(String, String)> {
    labels.iter().filter(|(k, _)| *k != "__name__").map(|(k, v)| (k.clone(), v.clone())).collect()
}

async fn metric_jobs(state: &AppState, ctx: &Context) -> Vec<BackupJob> {
    let mut names: Vec<String> =
        BACKUP_FAMILIES.iter().map(|f| format!("{METRIC_PREFIX}{}", f.metric)).collect();
    names.push(format!("{METRIC_PREFIX}{HYPER_BACKUP_RUNNING}"));
    let selector = format!("{{__name__=~\"{}\"}}", names.join("|"));
    let last = format!("last_over_time({selector}[{LOOKBACK_SECONDS}s]) keep_metric_names");
    let at = format!("tlast_over_time({selector}[{LOOKBACK_SECONDS}s]) keep_metric_names");
    let (last, at) =
        tokio::join!(vm::instant_in(state, ctx, &last), vm::instant_in(state, ctx, &at));
    let (Some(last), Some(at)) = (last, at) else { return Vec::new() };

    let stamps: HashMap<Vec<(String, String)>, f64> = at
        .iter()
        .filter(|s| s.metric.get("__name__").is_some_and(|n| !n.ends_with(HYPER_BACKUP_RUNNING)))
        .filter_map(|s| Some((full_key(&s.metric), vm::instant_value(s)?)))
        .collect();
    let running: HashMap<Vec<(String, String)>, bool> = last
        .iter()
        .filter(|s| s.metric.get("__name__").is_some_and(|n| n.ends_with(HYPER_BACKUP_RUNNING)))
        .filter_map(|s| Some((identity(&s.metric), vm::instant_value(s)? >= 1.0)))
        .collect();
    let now = ctx.now.timestamp() as f64;

    let mut jobs = Vec::new();
    // Ligne → (cible PBS, `datastore:groupe`), pour y joindre l'historique.
    let mut pbs_rows: Vec<(usize, TargetId, String)> = Vec::new();
    for series in &last {
        let Some(name) = series.metric.get("__name__") else { continue };
        let Some(metric) = name.strip_prefix(METRIC_PREFIX) else { continue };
        let Some(family) = BACKUP_FAMILIES.iter().find(|f| f.metric == metric) else { continue };
        let Some(id) = vm::target_of(&series.metric).filter(|id| ctx.sees(*id)) else { continue };
        let Some(monitor) = ctx.fleet.monitor(id) else { continue };
        let Some(age) = vm::instant_value(series).filter(|age| *age >= 0.0) else { continue };
        let Some(&sampled) = stamps.get(&full_key(&series.metric)) else { continue };
        let succeeded = sampled - age;
        let late = now - succeeded > family.limit_days * DAY;
        let label = |key: &str| series.metric.get(key).filter(|v| !v.is_empty()).cloned();
        let parts: Vec<String> = family.what.iter().filter_map(|key| label(*key)).collect();
        let job = if parts.is_empty() { monitor.name.clone() } else { parts.join(" · ") };
        let detail = match label("datastore").or_else(|| label("repo")) {
            Some(place) if !parts.contains(&place) => format!("{} → {place}", monitor.name),
            _ => monitor.name.clone(),
        };
        let status = if running.get(&identity(&series.metric)).copied().unwrap_or(false) {
            BackupStatus::Running
        } else if late {
            BackupStatus::Stale
        } else {
            BackupStatus::Ok
        };
        if metric == "pbs_backup_last_age_seconds"
            && let (Some(store), Some(group)) = (label("datastore"), label("group"))
        {
            pbs_rows.push((jobs.len(), id, format!("{store}:{group}")));
        }
        jobs.push(BackupJob {
            job,
            engine: engine(metric, &series.metric),
            detail,
            site: ctx.fleet.site_of(id),
            target_id: id,
            last_success: DateTime::from_timestamp(succeeded as i64, 0).map(stamp),
            duration_s: None,
            changed_bytes: None,
            runs: Vec::new(),
            status,
        });
    }
    add_pbs_history(state, ctx, &mut jobs, &pbs_rows).await;
    jobs
}

fn full_key(labels: &BTreeMap<String, String>) -> Vec<(String, String)> {
    labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// Une tâche PBS a réussi : `OK`, ou terminée avec des avertissements.
fn pbs_ok(status: Option<&str>) -> bool {
    status.is_some_and(|s| s == "OK" || s.starts_with("WARNINGS"))
}

/// Joint aux lignes PBS leurs dernières exécutions (tâches `backup` de
/// l'historique, `worker_id` = `datastore:type/id`).
async fn add_pbs_history(
    state: &AppState,
    ctx: &Context,
    jobs: &mut [BackupJob],
    rows: &[(usize, TargetId, String)],
) {
    let since = ctx.now.timestamp() - HISTORY_DAYS * 86_400;
    let mut targets: Vec<TargetId> = rows.iter().map(|(_, id, _)| *id).collect();
    targets.sort_unstable();
    targets.dedup();
    let mut tasks: HashMap<(TargetId, String), Vec<db::pbs::TaskRow>> = HashMap::new();
    for target in targets {
        match db::pbs::list_tasks(&state.pool, target, since).await {
            Ok(list) => {
                for task in list.into_iter().filter(|t| t.worker_type == "backup") {
                    tasks.entry((target, task.worker_id.clone())).or_default().push(task);
                }
            }
            Err(error) => {
                tracing::warn!(
                    ?error,
                    target_id = target,
                    "overview widgets: PBS history not read"
                );
            }
        }
    }
    for (index, target, key) in rows {
        let Some(list) = tasks.get_mut(&(*target, key.clone())) else { continue };
        list.sort_by_key(|t| t.starttime);
        let Some(job) = jobs.get_mut(*index) else { continue };
        let finished: Vec<&db::pbs::TaskRow> =
            list.iter().filter(|t| t.endtime.is_some()).collect();
        let skip = finished.len().saturating_sub(MAX_RUNS);
        job.runs = finished
            .iter()
            .skip(skip)
            .filter_map(|t| {
                let at = DateTime::from_timestamp(t.endtime.unwrap_or(t.starttime), 0)?;
                Some(BackupRun { at: stamp(at), ok: pbs_ok(t.status.as_deref()) })
            })
            .collect();
        if let Some(latest) = finished.last() {
            job.duration_s = latest.endtime.map(|end| (end - latest.starttime).max(0) as f64);
            if !pbs_ok(latest.status.as_deref()) {
                job.status = BackupStatus::Failed;
            }
        }
        if list.last().is_some_and(|t| t.endtime.is_none()) {
            job.status = BackupStatus::Running;
        }
    }
}

/// Une exécution Active Backup réussie : réussite, ou réussite partielle.
fn abb_ok(status: i64) -> bool {
    matches!(status, 2 | 3)
}

/// Les appareils Active Backup for Business des NAS Synology visibles.
async fn abb_jobs(state: &AppState, ctx: &Context) -> Vec<BackupJob> {
    let since = ctx.now.timestamp() - HISTORY_DAYS * 86_400;
    let mut jobs = Vec::new();
    for monitor in ctx.fleet.monitors.iter().filter(|m| m.kind == "synology") {
        let runs = match db::abb_runs::list_since(&state.pool, monitor.id, since).await {
            Ok(runs) => runs,
            Err(error) => {
                tracing::warn!(
                    ?error,
                    target_id = monitor.id,
                    "overview widgets: ABB history not read"
                );
                continue;
            }
        };
        let mut devices: BTreeMap<i64, Vec<AbbRun>> = BTreeMap::new();
        for run in runs {
            devices.entry(run.device_id).or_default().push(run);
        }
        for runs in devices.into_values() {
            jobs.push(abb_job(ctx, monitor.id, &runs));
        }
    }
    jobs
}

/// Une ligne par appareil, d'après ses exécutions (de la plus ancienne à la
/// plus récente).
fn abb_job(ctx: &Context, target: TargetId, runs: &[AbbRun]) -> BackupJob {
    let latest = runs.last();
    let finished: Vec<&AbbRun> = runs.iter().filter(|r| r.time_end > 0).collect();
    let last_ok = finished.iter().rev().find(|r| abb_ok(r.status));
    let last_done = finished.last();
    let now = ctx.now.timestamp();
    let status = match (latest, last_done) {
        (Some(run), _) if run.time_end <= 0 => BackupStatus::Running,
        (_, Some(done)) if abb_ok(done.status) => {
            if (now - done.time_end) as f64 > ABB_LIMIT_DAYS * DAY {
                BackupStatus::Stale
            } else {
                BackupStatus::Ok
            }
        }
        (_, Some(_)) => BackupStatus::Failed,
        _ => BackupStatus::Stale,
    };
    let skip = finished.len().saturating_sub(MAX_RUNS);
    BackupJob {
        job: latest.map(|r| r.device_name.clone()).unwrap_or_default(),
        engine: "Active Backup".to_string(),
        detail: latest.map(|r| r.task_name.clone()).unwrap_or_default(),
        site: ctx.fleet.site_of(target),
        target_id: target,
        last_success: last_ok.and_then(|r| DateTime::from_timestamp(r.time_end, 0)).map(stamp),
        duration_s: last_done.map(|r| (r.time_end - r.time_start).max(0) as f64),
        changed_bytes: last_done
            .map(|r| r.transfered_bytes)
            .filter(|bytes| *bytes >= 0)
            .map(|bytes| round(bytes as f64, 0)),
        runs: finished
            .iter()
            .skip(skip)
            .filter_map(|r| {
                let at = DateTime::from_timestamp(r.time_end, 0)?;
                Some(BackupRun { at: stamp(at), ok: abb_ok(r.status) })
            })
            .collect(),
        status,
    }
}
