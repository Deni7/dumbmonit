//! Les sondes de service (HTTP, HTTPS, DNS, TLS, SMTP, ping, TCP) : leurs 36
//! derniers créneaux de vérification sur la fenêtre, le p95 du temps de réponse
//! et la disponibilité sur 30 jours. Une sonde à plusieurs lieux est repliée
//! sur le pire.

use std::collections::HashMap;

use crate::alerting::model::TargetId;
use crate::state::AppState;
use crate::tsdb::{InstantSeries, RangeSeries};

use super::category::{SERVICE_KINDS, service_label};
use super::model::{ServiceCheck, WidgetStatus};
use super::{Context, round, vm};

/// Créneaux de vérification par sonde.
pub const SLOTS: usize = 36;

pub async fn load(state: &AppState, ctx: &Context) -> Vec<ServiceCheck> {
    let probes: Vec<_> =
        ctx.fleet.monitors.iter().filter(|m| SERVICE_KINDS.contains(&m.kind.as_str())).collect();
    if probes.is_empty() {
        return Vec::new();
    }
    let range = ctx.range.seconds();
    let step = (range / SLOTS as i64).max(60);
    let end = ctx.now.timestamp();
    let start = end - step * SLOTS as i64;
    let checks = format!("min by (target) (min_over_time(dumbmonit_probe_success[{step}s]))");
    let p95 = format!(
        "max by (target) (quantile_over_time(0.95, dumbmonit_probe_duration_seconds[{range}s])) \
         * 1000"
    );
    let uptime = "min by (target) (avg_over_time(dumbmonit_probe_success[30d])) * 100";
    let (checks, p95, uptime) = tokio::join!(
        vm::range(state, ctx, &checks, start + step, end, step),
        vm::instant_in(state, ctx, &p95),
        vm::instant_in(state, ctx, uptime),
    );
    let slots = slotted(&checks.unwrap_or_default(), start, step);
    let (p95, uptime) = (by_target(p95), by_target(uptime));

    let mut out: Vec<ServiceCheck> = probes
        .into_iter()
        .map(|m| ServiceCheck {
            target_id: m.id,
            name: m.name.clone(),
            kind: service_label(&m.kind, &m.address).unwrap_or("Check").to_string(),
            status: m.status,
            checks: slots.get(&m.id).cloned().unwrap_or_else(|| vec![None; SLOTS]),
            p95_ms: p95.get(&m.id).map(|v| round(*v, 0)),
            availability_30d: uptime.get(&m.id).map(|v| round(*v, 3)),
        })
        .collect();
    // Les sondes en panne d'abord, puis par nom.
    out.sort_by(|a, b| rank(a.status).cmp(&rank(b.status)).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Valeur de chaque moniteur d'une requête instantanée.
fn by_target(series: Option<Vec<InstantSeries>>) -> HashMap<TargetId, f64> {
    series
        .unwrap_or_default()
        .iter()
        .filter_map(|s| Some((vm::target_of(&s.metric)?, vm::instant_value(s)?)))
        .collect()
}

fn rank(status: WidgetStatus) -> u8 {
    match status {
        WidgetStatus::Down => 0,
        WidgetStatus::Degraded => 1,
        WidgetStatus::Up => 2,
        WidgetStatus::Paused => 3,
    }
}

/// Range les points de chaque série dans [`SLOTS`] créneaux de `step`
/// secondes depuis `start` : le point d'horodatage `t` résume `]t - step, t]`.
/// Un créneau sans point reste `None`.
pub fn slotted(
    series: &[RangeSeries],
    start: i64,
    step: i64,
) -> HashMap<TargetId, Vec<Option<bool>>> {
    let mut out = HashMap::new();
    for serie in series {
        let Some(id) = vm::target_of(&serie.metric) else { continue };
        let slots: &mut Vec<Option<bool>> = out.entry(id).or_insert_with(|| vec![None; SLOTS]);
        for (at, value) in vm::points(serie) {
            let offset = (at as i64 - start - 1).div_euclid(step.max(1));
            if let Ok(index) = usize::try_from(offset)
                && let Some(slot) = slots.get_mut(index)
            {
                *slot = Some(value >= 1.0);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn serie(target: &str, points: &[(f64, &str)]) -> RangeSeries {
        RangeSeries {
            metric: BTreeMap::from([("target".to_string(), target.to_string())]),
            values: points.iter().map(|(t, v)| (*t, (*v).to_string())).collect(),
        }
    }

    #[test]
    fn chaque_point_tombe_dans_son_creneau_et_les_trous_restent_vides() {
        let start = 1_000;
        let step = 100;
        let slots = slotted(&[serie("7", &[(1_100.0, "1"), (1_300.0, "0")])], start, step);
        let seven = &slots[&7];
        assert_eq!(seven.len(), SLOTS);
        assert_eq!(seven[0], Some(true));
        assert_eq!(seven[1], None);
        assert_eq!(seven[2], Some(false));
        assert!(seven[3..].iter().all(Option::is_none));
    }
}
