//! Disponibilité du parc, jour par jour sur 30 jours, et les 30 jours d'avant
//! pour la comparaison. Même lecture que la console (`fleet.ts`) : chaque
//! moniteur sur sa santé (`probe_success` pour une sonde, `up` pour un
//! équipement), le pire de ses lieux, puis la moyenne du parc.

use chrono::{DateTime, TimeDelta, Utc};

use crate::state::AppState;

use super::model::{AvailabilitySeries, DayPct};
use super::{Context, round, vm};

/// Objectif de disponibilité de l'instance (%). Aucun réglage d'instance ne le
/// porte aujourd'hui : les SLO, eux, ont chacun le leur.
pub const SLO_TARGET: f64 = 99.9;
const DAYS: i64 = 30;
const DAY: i64 = 86_400;
const HEALTH: &str = "{__name__=~\"dumbmonit_probe_success|dumbmonit_up\"}";

fn fleet_over(window: &str) -> String {
    format!("avg(min by (target) (avg_over_time({HEALTH}[{window}]))) * 100")
}

pub async fn load(state: &AppState, ctx: &Context) -> AvailabilitySeries {
    let today = vm::midnight(ctx.now);
    let mut pcts: Vec<Option<f64>> = vec![None; (2 * DAYS) as usize];
    if !ctx.fleet.monitors.is_empty() {
        // Un point à chaque minuit résume la journée qui s'achève ; le dernier
        // jour, en cours, est lu à part sur ce qui en est écoulé.
        let first = today - TimeDelta::days(2 * DAYS - 1);
        let elapsed = (ctx.now - today).num_seconds().max(60);
        let per_day = fleet_over("1d");
        let so_far = fleet_over(&format!("{elapsed}s"));
        let (days, current) = tokio::join!(
            vm::range(state, ctx, &per_day, first.timestamp(), today.timestamp(), DAY),
            vm::instant_in(state, ctx, &so_far),
        );
        for serie in days.unwrap_or_default() {
            for (at, value) in vm::points(&serie) {
                // Le point de minuit `at` porte la journée d'avant.
                let index = (at as i64 - DAY - first.timestamp()).div_euclid(DAY);
                if let Some(slot) = usize::try_from(index).ok().and_then(|i| pcts.get_mut(i)) {
                    *slot = Some(value);
                }
            }
        }
        if let Some(value) = current.unwrap_or_default().first().and_then(vm::instant_value)
            && let Some(slot) = pcts.last_mut()
        {
            *slot = Some(value);
        }
    }
    summarize(&pcts, today)
}

/// Les 60 jours (le plus ancien d'abord, aujourd'hui en dernier) ramenés aux
/// 30 derniers, avec leur moyenne, celle d'avant et le budget d'erreur.
pub fn summarize(pcts: &[Option<f64>], today: DateTime<Utc>) -> AvailabilitySeries {
    let total = pcts.len();
    let split = total.saturating_sub(DAYS as usize);
    let (previous, recent) = pcts.split_at(split);
    let days = recent
        .iter()
        .enumerate()
        .map(|(i, pct)| {
            let back = i64::try_from(recent.len() - 1 - i).unwrap_or(0);
            DayPct {
                day: (today - TimeDelta::days(back)).format("%Y-%m-%d").to_string(),
                pct: pct.map(|v| round(v.clamp(0.0, 100.0), 3)),
            }
        })
        .collect();
    let avg_30d = mean(recent);
    AvailabilitySeries {
        days,
        avg_30d: avg_30d.map(|v| round(v, 3)),
        previous_avg_30d: mean(previous).map(|v| round(v, 3)),
        slo_target: SLO_TARGET,
        budget_left_pct: avg_30d.map(|avg| round(budget_left(avg, SLO_TARGET), 1)),
    }
}

fn mean(values: &[Option<f64>]) -> Option<f64> {
    let known: Vec<f64> = values.iter().flatten().copied().collect();
    (!known.is_empty()).then(|| known.iter().sum::<f64>() / known.len() as f64)
}

/// Part du budget d'erreur qui reste, de 0 à 100 : le budget est ce que
/// l'objectif laisse d'indisponibilité, la dépense ce qui en a été consommé.
pub fn budget_left(availability: f64, objective: f64) -> f64 {
    let budget = 100.0 - objective;
    if budget <= 0.0 {
        return if availability >= 100.0 { 100.0 } else { 0.0 };
    }
    let spent = (100.0 - availability).max(0.0);
    ((budget - spent) / budget * 100.0).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_budget_suit_l_objectif() {
        assert!((budget_left(100.0, 99.9) - 100.0).abs() < 1e-9);
        assert!((budget_left(99.95, 99.9) - 50.0).abs() < 1e-6);
        assert_eq!(budget_left(99.0, 99.9), 0.0);
    }

    #[test]
    fn trente_jours_finissent_aujourd_hui() {
        let today =
            DateTime::parse_from_rfc3339("2026-11-23T00:00:00Z").unwrap().with_timezone(&Utc);
        let mut pcts = vec![Some(99.0); 30];
        pcts.extend(vec![None; 29]);
        pcts.push(Some(100.0));
        let out = summarize(&pcts, today);
        assert_eq!(out.days.len(), 30);
        assert_eq!(out.days[29].day, "2026-11-23");
        assert_eq!(out.days[0].day, "2026-10-25");
        assert_eq!(out.avg_30d, Some(100.0));
        assert_eq!(out.previous_avg_30d, Some(99.0));
        assert_eq!(out.budget_left_pct, Some(100.0));
    }
}
