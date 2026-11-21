//! Mesures de disponibilité des SLO, lues dans VictoriaMetrics.
//!
//! Les requêtes sont celles des pages de statut et des rapports SLA
//! (`api::status_pages`) : part des sondes réussies pour une sonde, part des
//! tranches de cinq minutes vivantes pour un équipement, ramenée à la partie
//! de la fenêtre réellement couverte par des données (un moniteur ajouté hier
//! n'a pas été « en panne » le mois dernier).

use std::collections::HashMap;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::{Measures, MonitorRef, burndown, burndown_step};
use crate::alerting::source::{MetricSource, SeriesPoint, SeriesRange};
use crate::api::status_pages::{
    first_up_query, presence_slots, probe_uptime_query, rescale, selector,
};

/// Identifiant de cible porté par une série (`target`).
fn target_of(labels: &std::collections::BTreeMap<String, String>) -> Option<i64> {
    labels.get("target")?.parse().ok()
}

fn by_target(points: Vec<SeriesPoint>) -> HashMap<i64, f64> {
    points
        .into_iter()
        .filter_map(|point| Some((target_of(&point.labels)?, point.value)))
        .filter(|(_, value)| value.is_finite())
        .collect()
}

fn split(members: &[&MonitorRef]) -> (Vec<i64>, Vec<i64>) {
    let probes = members.iter().filter(|m| m.is_probe()).map(|m| m.id).collect();
    let devices = members.iter().filter(|m| !m.is_probe()).map(|m| m.id).collect();
    (probes, devices)
}

/// Disponibilité (en pour cent) de chaque moniteur sur chacune des fenêtres
/// `windows` (en secondes) qui se terminent maintenant.
///
/// Un moniteur sans mesure sur une fenêtre en est absent. Une requête qui
/// échoue fait échouer l'ensemble.
pub async fn availability(
    source: &dyn MetricSource,
    members: &[&MonitorRef],
    windows: &[i64],
    now: DateTime<Utc>,
) -> Result<Measures> {
    let mut out: Measures = windows.iter().map(|w| (*w, HashMap::new())).collect();
    if members.is_empty() || windows.is_empty() {
        return Ok(out);
    }
    let (probes, devices) = split(members);

    if !probes.is_empty() {
        let sel = selector(&probes);
        for window in windows {
            let query = probe_uptime_query(&sel, &format!("{window}s"), "");
            let values = by_target(source.instant(&query).await?);
            out.entry(*window).or_default().extend(values);
        }
    }

    if !devices.is_empty() {
        let sel = selector(&devices);
        let reach = windows.iter().copied().max().unwrap_or(0);
        let first = by_target(source.instant(&first_up_query(&sel, &format!("{reach}s"))).await?);
        let now_secs = now.timestamp() as f64;
        for window in windows {
            let presence = presence_slots(&sel, &format!("{window}s"));
            let query = format!("avg_over_time({presence}) * 100");
            let span = *window as f64;
            let per = out.entry(*window).or_default();
            for (id, measured) in by_target(source.instant(&query).await?) {
                let Some(first_ts) = first.get(&id) else { continue };
                let covered = span.min(now_secs - first_ts);
                if let Some(value) = rescale(measured, span, covered) {
                    per.insert(id, value);
                }
            }
        }
    }
    Ok(out)
}

/// Disponibilité moyenne des moniteurs par tranche de `step` secondes sur
/// `[start, end]` : `(fin de tranche, disponibilité, durée)`.
///
/// Une tranche d'un équipement qui commence avant sa première mesure est
/// écartée : la présence y vaudrait zéro sans que rien n'ait été en panne.
pub async fn slices(
    source: &dyn MetricSource,
    members: &[&MonitorRef],
    start: i64,
    end: i64,
    step: i64,
) -> Result<Vec<(f64, f64, f64)>> {
    let (probes, devices) = split(members);
    let window = format!("{step}s");
    // Par fin de tranche (secondes), les disponibilités des moniteurs mesurés.
    let mut per_slice: HashMap<i64, Vec<f64>> = HashMap::new();
    let mut collect = |series: Vec<SeriesRange>, first: Option<&HashMap<i64, f64>>| {
        for serie in series {
            let Some(id) = target_of(&serie.labels) else { continue };
            for (ts, value) in serie.points {
                if !value.is_finite() {
                    continue;
                }
                if let Some(first) = first {
                    let Some(first_ts) = first.get(&id) else { break };
                    if ts - (step as f64) < *first_ts {
                        continue;
                    }
                }
                per_slice.entry(ts.round() as i64).or_default().push(value.clamp(0.0, 100.0));
            }
        }
    };

    if !probes.is_empty() {
        let query = probe_uptime_query(&selector(&probes), &window, "");
        collect(source.range(&query, start, end, step as u64, None).await?, None);
    }
    if !devices.is_empty() {
        let sel = selector(&devices);
        let reach = (end - start).max(step) + step;
        let first = by_target(source.instant(&first_up_query(&sel, &format!("{reach}s"))).await?);
        let query = format!("avg_over_time({}) * 100", presence_slots(&sel, &window));
        collect(source.range(&query, start, end, step as u64, None).await?, Some(&first));
    }

    let mut out: Vec<(f64, f64, f64)> = per_slice
        .into_iter()
        .filter_map(|(ts, values)| {
            super::aggregate(values).map(|value| (ts as f64, value, step as f64))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

/// Courbe de consommation du budget d'un SLO sur sa fenêtre, jusqu'à `now`.
pub async fn burndown_curve(
    source: &dyn MetricSource,
    members: &[&MonitorRef],
    objective: f64,
    window_days: u32,
    now: DateTime<Utc>,
) -> Result<Vec<[f64; 2]>> {
    let step = burndown_step(window_days);
    let end = now.timestamp();
    let start = end - i64::from(window_days) * 86_400;
    if members.is_empty() {
        return Ok(Vec::new());
    }
    // Le premier point couvre la tranche qui finit une tranche après le début.
    let slices = slices(source, members, start + step, end, step).await?;
    Ok(burndown(objective, window_days, start as f64, &slices))
}
