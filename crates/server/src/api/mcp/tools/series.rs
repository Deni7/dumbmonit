//! Requêtes de séries temporelles (`query_metrics`).

use super::*;

/// Ne garde qu'un point sur n pour rester sous [`MAX_POINTS`].
pub(super) fn thin(values: Vec<(f64, String)>) -> Vec<(f64, String)> {
    if values.len() <= MAX_POINTS {
        return values;
    }
    let stride = values.len().div_ceil(MAX_POINTS);
    values.into_iter().step_by(stride).collect()
}

pub(super) async fn query_metrics(state: &AppState, args: &Args) -> ToolResult {
    let query = args.str("query").ok_or_else(|| failed("\"query\" is required."))?.to_string();
    let range_hours = args.f64("range_hours").filter(|h| h.is_finite() && *h > 0.0).unwrap_or(1.0);
    if range_hours > MAX_RANGE_HOURS {
        return Err(failed(format!("\"range_hours\" is limited to {MAX_RANGE_HOURS} (31 days).")));
    }
    let end = Utc::now().timestamp_millis();
    let start = end - (range_hours * 3_600_000.0) as i64;
    // Le pas est choisi pour tenir en 60 points ; un pas plus fin demandé par
    // l'appelant est élargi silencieusement, comme le fait l'interface.
    let floor = ((end - start) / 1000 / MAX_POINTS as i64).max(1) as u64;
    let step = args.i64("step_secs").filter(|s| *s > 0).map_or(floor, |s| (s as u64).max(floor));

    let Json(series) = metrics::query_range(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
        Query(RangeQuery { query: query.clone(), start, end, step: Some(step) }),
    )
    .await?;

    if series.is_empty() {
        return Ok(ToolOutput::new(
            format!("No series matched `{query}` over the last {range_hours} h."),
            json!({ "query": query, "series": [] }),
        ));
    }

    let mut lines = vec![format!(
        "{} for `{query}` over the last {range_hours} h (step {step} s):",
        plural(series.len(), "series", "series")
    )];
    let mut structured = Vec::new();
    for serie in series {
        let values = thin(serie.values);
        let numbers: Vec<f64> = values.iter().filter_map(|(_, v)| v.parse().ok()).collect();
        let labels =
            serie.metric.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(", ");
        if numbers.is_empty() {
            lines.push(format!("- {{{labels}}}: no numeric value"));
        } else {
            let min = numbers.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = numbers.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let avg = numbers.iter().sum::<f64>() / numbers.len() as f64;
            let last = numbers[numbers.len() - 1];
            lines.push(format!(
                "- {{{labels}}}: {} points, min {}, max {}, avg {}, last {}",
                numbers.len(),
                trim_float(min),
                trim_float(max),
                trim_float(avg),
                trim_float(last)
            ));
        }
        structured.push(json!({
            "metric": serie.metric,
            "values": values.iter().map(|(ts, v)| json!([ts, v.parse::<f64>().ok()])).collect::<Vec<_>>(),
        }));
    }
    Ok(ToolOutput::new(
        lines.join("\n"),
        json!({ "query": query, "step_secs": step, "series": structured }),
    ))
}
