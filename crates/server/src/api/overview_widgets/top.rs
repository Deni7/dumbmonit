//! Les plus gros consommateurs : processeur, mémoire, disque (par volume, avec
//! la prévision de remplissage) et réseau (par interface), cinq de chaque, avec
//! leur courbe sur la fenêtre (48 points au plus).
//!
//! Une famille que le parc ne publie pas rend simplement une liste vide.

use crate::alerting::capacity::{self, FAMILIES};
use crate::alerting::forecast::{self, Status};
use crate::state::AppState;
use crate::tsdb::RangeSeries;

use super::model::{TopConsumers, TopEntry};
use super::{Context, round, vm};

/// Entrées par liste.
const TOP: usize = 5;
/// Points par courbe, au plus.
const POINTS: i64 = 48;

/// Taux d'occupation du processeur, toutes intégrations confondues.
const CPU: &str =
    "dumbmonit_(cpu_usage_percent|proxmox_node_cpu_percent|synology_cpu_usage_percent)";
/// Taux d'occupation de la mémoire.
const MEM: &str = "dumbmonit_(memory_used_percent|proxmox_node_memory_percent|\
                   synology_memory_usage_percent|unraid_memory_used_percent)";
/// Familles de capacité exprimées en pourcentage sans octets qui sont tout de
/// même des disques.
const DISK_WITHOUT_BYTES: &[&str] =
    &["proxmox_storage", "proxmox_rootfs", "proxmox_zfs", "unraid_disk", "opnsense_disk"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Cpu,
    Mem,
    Disk,
    Net,
}

/// Les métriques de disque : les familles de capacité en octets, plus celles
/// de [`DISK_WITHOUT_BYTES`], quand leur pourcentage est une métrique nue.
fn disk_metrics() -> Vec<&'static str> {
    FAMILIES
        .iter()
        .filter(|f| f.unit == "bytes" || DISK_WITHOUT_BYTES.contains(&f.id))
        .filter(|f| capacity::is_bare_metric(f.percent))
        .map(|f| f.percent)
        .collect()
}

fn query(family: Family, step: i64) -> String {
    match family {
        Family::Cpu => format!(
            "topk_last({TOP}, max by (target) (avg_over_time({{__name__=~\"{CPU}\"}}[{step}s])))"
        ),
        Family::Mem => format!(
            "topk_last({TOP}, max by (target) (avg_over_time({{__name__=~\"{MEM}\"}}[{step}s])))"
        ),
        Family::Disk => format!(
            "topk_last({TOP}, last_over_time({{__name__=~\"{}\"}}[{step}s]) keep_metric_names \
             or (100 * last_over_time(dumbmonit_storage_bytes_used[{step}s]) \
             / last_over_time(dumbmonit_storage_bytes_total[{step}s])))",
            disk_metrics().join("|")
        ),
        Family::Net => {
            // Un débit se lit sur au moins cinq minutes : un compteur
            // interrogé toutes les minutes n'a sinon qu'un point par fenêtre.
            let window = step.max(300);
            format!(
                "topk_last({TOP}, sum by (target, ifname) \
                 (rate({{__name__=~\"dumbmonit_if_octets_(in|out)\", ifname!~\"lo|veth.*\"}}\
                 [{window}s])) * 8 / 1000000)"
            )
        }
    }
}

pub async fn load(state: &AppState, ctx: &Context) -> TopConsumers {
    if ctx.fleet.monitors.is_empty() {
        return TopConsumers::default();
    }
    let range = ctx.range.seconds();
    let step = vm::step_for(range, POINTS);
    let end = ctx.now.timestamp();
    let start = end - range;
    let run = move |family: Family| async move {
        let series = vm::range(state, ctx, &query(family, step), start, end, step).await;
        entries(ctx, family, &series.unwrap_or_default(), range)
    };
    let (cpu, mem, disk, net) =
        tokio::join!(run(Family::Cpu), run(Family::Mem), run(Family::Disk), run(Family::Net));
    TopConsumers { cpu, mem, disk, net }
}

fn entries(ctx: &Context, family: Family, series: &[RangeSeries], range: i64) -> Vec<TopEntry> {
    let now_s = ctx.now.timestamp() as f64;
    let mut out: Vec<TopEntry> = series
        .iter()
        .filter_map(|serie| {
            let id = vm::target_of(&serie.metric)?;
            if !ctx.sees(id) {
                return None;
            }
            let monitor = ctx.fleet.monitor(id)?;
            let points = vm::points(serie);
            let &(_, last) = points.last()?;
            let detail = match family {
                Family::Cpu | Family::Mem => String::new(),
                Family::Disk => capacity::entity(&serie.metric),
                Family::Net => serie.metric.get("ifname").cloned().unwrap_or_default(),
            };
            let label = if detail.is_empty() {
                monitor.name.clone()
            } else {
                format!("{} {detail}", monitor.name)
            };
            let forecast_days = (family == Family::Disk)
                .then(|| forecast::forecast(&points, 100.0, now_s, range as f64))
                .filter(|f| matches!(f.status, Status::Growing | Status::Full))
                .and_then(|f| f.days_to_full)
                .filter(|days| days.is_finite())
                .map(|days| round(days, 1));
            let skip = points.len().saturating_sub(POINTS as usize);
            Some(TopEntry {
                target_id: id,
                label,
                site: ctx.fleet.site_of(id),
                value: round(last, 1),
                unit: if family == Family::Net { "Mb/s" } else { "%" }.to_string(),
                series: points.iter().skip(skip).map(|(_, v)| round(*v, 2)).collect(),
                forecast_days,
            })
        })
        .collect();
    out.sort_by(|a, b| b.value.total_cmp(&a.value).then_with(|| a.label.cmp(&b.label)));
    out.truncate(TOP);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_disques_ne_comptent_ni_licences_ni_onduleurs() {
        let metrics = disk_metrics();
        assert!(metrics.contains(&"dumbmonit_filesystem_used_percent"));
        assert!(metrics.contains(&"dumbmonit_proxmox_storage_used_percent"));
        assert!(!metrics.iter().any(|m| m.contains("license") || m.contains("nut_")));
        assert!(metrics.iter().all(|m| capacity::is_bare_metric(m)));
    }

    #[test]
    fn chaque_requete_est_bornee_a_cinq_series() {
        for family in [Family::Cpu, Family::Mem, Family::Disk, Family::Net] {
            assert!(query(family, 1800).starts_with("topk_last(5, "), "{family:?}");
        }
        assert!(query(Family::Net, 60).contains("[300s]"));
    }
}
