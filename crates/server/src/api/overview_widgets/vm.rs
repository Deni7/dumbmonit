//! Lectures de séries pour les tableaux de bord : toujours filtrées par les
//! équipes du compte, et sans jamais faire échouer la réponse. Un échec rend
//! `None` ; il n'est journalisé qu'une fois par requête.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};

use crate::alerting::model::TargetId;
use crate::state::AppState;
use crate::tsdb::{InstantSeries, RangeSeries, log_query_failure};

use super::Context;

/// Une requête de séries a-t-elle déjà échoué pendant cette réponse ?
#[derive(Debug, Default)]
pub struct Health {
    failed: AtomicBool,
}

impl Health {
    fn note(&self, error: &anyhow::Error) {
        if !self.failed.swap(true, Ordering::Relaxed) {
            log_query_failure(error, "overview widgets: series not read, metric parts left empty");
        }
    }
}

/// Requête instantanée, restreinte aux moniteurs visibles.
pub async fn instant(
    state: &AppState,
    filter: Option<&str>,
    health: &Health,
    query: &str,
) -> Option<Vec<InstantSeries>> {
    match state.victoria.query_fresh_filtered(query, filter).await {
        Ok(series) => Some(series),
        Err(error) => {
            health.note(&error);
            None
        }
    }
}

/// Requête instantanée avec le contexte d'une réponse.
pub async fn instant_in(
    state: &AppState,
    ctx: &Context,
    query: &str,
) -> Option<Vec<InstantSeries>> {
    instant(state, ctx.filter.as_deref(), &ctx.vm, query).await
}

/// Requête sur une plage, restreinte aux moniteurs visibles. Bornes en
/// secondes Unix.
pub async fn range(
    state: &AppState,
    ctx: &Context,
    query: &str,
    start_s: i64,
    end_s: i64,
    step_s: i64,
) -> Option<Vec<RangeSeries>> {
    let step = u64::try_from(step_s.max(1)).unwrap_or(1);
    match state
        .victoria
        .query_range_filtered(query, start_s * 1000, end_s * 1000, step, ctx.filter.as_deref())
        .await
    {
        Ok(series) => Some(series),
        Err(error) => {
            ctx.vm.note(&error);
            None
        }
    }
}

/// Identifiant de moniteur d'une série (étiquette `target`).
pub fn target_of(metric: &BTreeMap<String, String>) -> Option<TargetId> {
    metric.get("target").and_then(|raw| raw.parse::<TargetId>().ok())
}

/// Valeur numérique finie d'un point.
pub fn number(raw: &str) -> Option<f64> {
    raw.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Valeur d'une série instantanée.
pub fn instant_value(series: &InstantSeries) -> Option<f64> {
    number(&series.value.1)
}

/// Points d'une série de plage, du plus ancien au plus récent, sans les
/// valeurs illisibles.
pub fn points(series: &RangeSeries) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> =
        series.values.iter().filter_map(|(at, raw)| number(raw).map(|v| (*at, v))).collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

/// Pas d'une plage pour en tirer au plus `points` points, jamais moins d'une
/// minute.
pub fn step_for(range_s: i64, points: i64) -> i64 {
    (range_s / points.max(1)).max(60)
}

/// Début de la journée UTC de `at`.
pub fn midnight(at: DateTime<Utc>) -> DateTime<Utc> {
    at.date_naive().and_hms_opt(0, 0, 0).map_or(at, |naive| naive.and_utc())
}
