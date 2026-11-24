//! Ce que l'historique des alertes dit des 30 derniers jours : déclenchements
//! par jour et par gravité, délais moyens d'acquittement et de résolution
//! (MTTA, MTTR) face aux 30 jours d'avant, part acquittée en moins d'un quart
//! d'heure, règle la plus bruyante ; les ouvertures des 14 derniers jours ; et
//! les dernières transitions pour le fil des évènements.
//!
//! Les épisodes et les délais sont ceux du rapport de bruit
//! ([`crate::alerting::noise`], [`crate::db::alert_insights`]).

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Context as _, Result};
use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use sqlx::{Row, SqlitePool};

use crate::alerting::model::TargetId;
use crate::alerting::noise::{self, Episode};
use crate::db::{self, alert_insights};
use crate::state::AppState;

use super::events::{self, EVENT_DAYS};
use super::model::{
    AlertDay, AlertStats, AlertsOpened, DayCount, EventKind, Noisiest, WidgetEvent,
};
use super::{Context, round, stamp, vm};

const WINDOW_DAYS: i64 = 30;
const OPENED_DAYS: i64 = 14;
/// Épisodes lus au plus sur les deux fenêtres.
const MAX_EPISODES: usize = 100_000;
/// Transitions lues pour le fil des évènements.
const HISTORY_EVENTS: i64 = 40;
/// Un acquittement « rapide ».
const QUICK_ACK_SECS: i64 = 15 * 60;

#[derive(Debug, Default)]
pub struct AlertPart {
    pub stats: AlertStats,
    pub opened: AlertsOpened,
    pub events: Vec<WidgetEvent>,
}

/// Même format que les horodatages d'`alert_history`.
fn to_sql(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Restriction aux équipements visibles, à coller dans une clause `WHERE`.
/// Les identifiants sont des entiers lus en base, jamais une saisie.
fn scope_clause(visible: Option<&HashSet<TargetId>>) -> String {
    match visible {
        None => String::new(),
        Some(visible) => {
            let mut ids: Vec<TargetId> = visible.iter().copied().collect();
            ids.sort_unstable();
            let ids = ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            if ids.is_empty() {
                " AND target_id IN (-1)".to_string()
            } else {
                format!(" AND target_id IN ({ids})")
            }
        }
    }
}

pub async fn load(state: &AppState, ctx: &Context) -> Result<AlertPart> {
    let pool = &state.pool;
    let scope = ctx.visible.as_ref();
    let today = vm::midnight(ctx.now);
    let first_day = today - TimeDelta::days(WINDOW_DAYS - 1);
    let since = ctx.now - TimeDelta::days(2 * WINDOW_DAYS);
    let (episodes, _) = alert_insights::load_episodes(pool, since, scope, MAX_EPISODES).await?;
    let counts = daily_counts(pool, first_day, scope).await?;
    let last_resolved = last_resolved(pool, scope).await?;
    let rules: HashMap<String, String> =
        db::alerts::list_rules(pool).await?.into_iter().map(|r| (r.uid, r.name)).collect();
    let rule_name = |uid: &str| rules.get(uid).cloned().unwrap_or_else(|| uid.to_string());

    let days: Vec<String> = (0..WINDOW_DAYS)
        .map(|i| (first_day + TimeDelta::days(i)).format("%Y-%m-%d").to_string())
        .collect();
    let stats = stats(ctx, &episodes, &days, &counts, &rule_name);
    let opened = opened(&days, &counts, last_resolved);
    let events = history_events(pool, ctx, &rule_name).await?;
    Ok(AlertPart { stats, opened, events })
}

/// Déclenchements par jour (`AAAA-MM-JJ`, UTC) et par gravité depuis `since`.
async fn daily_counts(
    pool: &SqlitePool,
    since: DateTime<Utc>,
    visible: Option<&HashSet<TargetId>>,
) -> Result<HashMap<String, AlertDay>> {
    let sql = format!(
        "SELECT substr(at, 1, 10) AS day, severity, COUNT(*) AS n
         FROM alert_history
         WHERE to_phase = 'firing' AND at >= ?{}
         GROUP BY day, severity",
        scope_clause(visible)
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(to_sql(since))
        .fetch_all(pool)
        .await
        .context("décompte quotidien des alertes")?;
    let mut out: HashMap<String, AlertDay> = HashMap::new();
    for row in rows {
        let day: String = row.try_get("day")?;
        let severity: String = row.try_get("severity")?;
        let n = u32::try_from(row.try_get::<i64, _>("n")?.max(0)).unwrap_or(u32::MAX);
        let entry =
            out.entry(day.clone()).or_insert_with(|| AlertDay { day, ..AlertDay::default() });
        match severity.as_str() {
            "critical" => entry.critical += n,
            "info" => entry.info += n,
            _ => entry.warning += n,
        }
    }
    Ok(out)
}

async fn last_resolved(
    pool: &SqlitePool,
    visible: Option<&HashSet<TargetId>>,
) -> Result<Option<DateTime<Utc>>> {
    let sql = format!(
        "SELECT MAX(at) FROM alert_history WHERE to_phase = 'resolved'{}",
        scope_clause(visible)
    );
    let at: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_one(pool)
        .await
        .context("dernière résolution d'alerte")?;
    Ok(at.as_deref().and_then(super::parse_time))
}

fn stats(
    ctx: &Context,
    episodes: &[Episode],
    days: &[String],
    counts: &HashMap<String, AlertDay>,
    rule_name: &(dyn Fn(&str) -> String + Sync),
) -> AlertStats {
    let cut = ctx.now - TimeDelta::days(WINDOW_DAYS);
    let (current, previous): (Vec<Episode>, Vec<Episode>) =
        episodes.iter().cloned().partition(|e| e.fired_at >= cut);
    let now_report = noise::summarize(&current, &[], 0).overall;
    let before = noise::summarize(&previous, &[], 0).overall;
    let quick = current
        .iter()
        .filter(|e| e.acked_at.is_some_and(|at| (at - e.fired_at).num_seconds() <= QUICK_ACK_SECS))
        .count();
    let acked_within_15m_pct =
        (!current.is_empty()).then(|| round(quick as f64 * 100.0 / current.len() as f64, 1));

    let mut noise_by: BTreeMap<(String, Option<TargetId>), u32> = BTreeMap::new();
    for episode in &current {
        *noise_by.entry((episode.rule_uid.clone(), episode.target_id)).or_default() += 1;
    }
    let noisiest = noise_by
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|((uid, target_id), count)| Noisiest {
            rule: rule_name(&uid),
            target: target_id
                .and_then(|id| ctx.fleet.name_of(id))
                .map(str::to_string)
                .unwrap_or_default(),
            target_id,
            count,
        });

    let per_day: Vec<AlertDay> =
        days.iter().map(|day| counts.get(day).cloned().unwrap_or_else(|| empty_day(day))).collect();
    AlertStats {
        days: per_day,
        total: u32::try_from(current.len()).unwrap_or(u32::MAX),
        previous_total: u32::try_from(previous.len()).unwrap_or(u32::MAX),
        mtta_s: now_report.mtta_secs,
        previous_mtta_s: before.mtta_secs,
        mttr_s: now_report.mttr_secs,
        previous_mttr_s: before.mttr_secs,
        acked_within_15m_pct,
        noisiest,
    }
}

fn empty_day(day: &str) -> AlertDay {
    AlertDay { day: day.to_string(), ..AlertDay::default() }
}

/// Alertes ouvertes par jour sur les 14 derniers jours : les gravités qui
/// comptent comme un problème (critique et avertissement), comme le badge.
fn opened(
    days: &[String],
    counts: &HashMap<String, AlertDay>,
    last_resolved: Option<DateTime<Utc>>,
) -> AlertsOpened {
    let skip = days.len().saturating_sub(OPENED_DAYS as usize);
    let days: Vec<DayCount> = days
        .iter()
        .skip(skip)
        .map(|day| DayCount {
            day: day.clone(),
            count: counts.get(day).map_or(0, |c| c.critical + c.warning),
        })
        .collect();
    let half = days.len().saturating_sub(7);
    AlertsOpened {
        this_week: days[half..].iter().map(|d| d.count).sum(),
        last_week: days[..half].iter().map(|d| d.count).sum(),
        days,
        last_resolved_at: last_resolved.map(stamp),
    }
}

/// Les dernières transitions des alertes visibles : déclenchées, résolues,
/// acquittées.
async fn history_events(
    pool: &SqlitePool,
    ctx: &Context,
    rule_name: &(dyn Fn(&str) -> String + Sync),
) -> Result<Vec<WidgetEvent>> {
    let since = ctx.now - TimeDelta::days(EVENT_DAYS);
    let sql = format!(
        "SELECT rule_uid, target_id, to_phase, severity, at, acked_at
         FROM alert_history
         WHERE at >= ? AND to_phase IN ('firing', 'resolved'){}
         ORDER BY at DESC, id DESC
         LIMIT ?",
        scope_clause(ctx.visible.as_ref())
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(to_sql(since))
        .bind(HISTORY_EVENTS)
        .fetch_all(pool)
        .await
        .context("dernières transitions d'alerte")?;
    let mut out = Vec::new();
    for row in rows {
        let uid: String = row.try_get("rule_uid")?;
        let target_id: Option<TargetId> = row.try_get("target_id")?;
        let phase: String = row.try_get("to_phase")?;
        let severity: String = row.try_get("severity")?;
        let Some(at) = super::parse_time(&row.try_get::<String, _>("at")?) else { continue };
        let acked_at = row.try_get::<Option<String>, _>("acked_at")?;
        let rule = rule_name(&uid);
        let target = target_id.and_then(|id| ctx.fleet.name_of(id));
        if phase == "resolved" {
            out.push(WidgetEvent {
                at: stamp(at),
                kind: EventKind::Ok,
                text: events::alert_text(&rule, "resolved", target),
                target_id,
            });
            continue;
        }
        let kind = match severity.as_str() {
            "critical" => EventKind::Crit,
            "info" => EventKind::Info,
            _ => EventKind::Warn,
        };
        out.push(WidgetEvent {
            at: stamp(at),
            kind,
            text: events::alert_text(&rule, "fired", target),
            target_id,
        });
        if let Some(acked) = acked_at.as_deref().and_then(super::parse_time)
            && acked >= since
        {
            out.push(WidgetEvent {
                at: stamp(acked),
                kind: EventKind::Ack,
                text: events::alert_text(&rule, "acknowledged", target),
                target_id,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(day: &str, critical: u32, warning: u32) -> (String, AlertDay) {
        (day.to_string(), AlertDay { day: day.to_string(), critical, warning, info: 5 })
    }

    #[test]
    fn la_semaine_se_compare_a_la_precedente_sans_les_infos() {
        let days: Vec<String> = (1..=30).map(|d| format!("2026-11-{d:02}")).collect();
        let entries = [day("2026-11-30", 2, 1), day("2026-11-24", 1, 0), day("2026-11-20", 0, 4)];
        let counts: HashMap<String, AlertDay> = entries.into_iter().collect();
        let out = opened(&days, &counts, None);
        assert_eq!(out.days.len(), 14);
        assert_eq!(out.days[0].day, "2026-11-17");
        assert_eq!(out.this_week, 4, "24 → 30 : 3 + 1");
        assert_eq!(out.last_week, 4, "17 → 23 : le 20");
        assert!(out.last_resolved_at.is_none());
    }

    #[test]
    fn la_portee_ne_laisse_rien_passer_sans_moniteur_visible() {
        assert_eq!(scope_clause(None), "");
        assert_eq!(scope_clause(Some(&HashSet::new())), " AND target_id IN (-1)");
        assert_eq!(scope_clause(Some(&HashSet::from([3, 1]))), " AND target_id IN (1,3)");
    }
}
