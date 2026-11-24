//! Le fil des évènements : transitions d'alerte, début et fin des fenêtres de
//! maintenance, moniteurs ajoutés, agents mis à jour — les plus récents
//! d'abord, en anglais court.
//!
//! Aucune suppression de moniteur n'est consignée aujourd'hui (ni journal
//! d'audit, ni trace en base) : le fil n'en montre donc pas.

use chrono::TimeDelta;

use crate::alerting::Silence;

use super::model::{EventKind, WidgetEvent};
use super::{Context, parse_time, stamp};

/// Évènements rendus, au plus.
pub const MAX_EVENTS: usize = 20;
/// Profondeur du fil, en jours.
pub const EVENT_DAYS: i64 = 7;
/// Occurrences déroulées par fenêtre de maintenance, au plus.
const MAX_OCCURRENCES: usize = 20;

/// « Disk almost full fired on nas », ou sans moniteur « Disk almost full fired ».
pub fn alert_text(rule: &str, verb: &str, target: Option<&str>) -> String {
    match target {
        Some(target) => format!("{rule} {verb} on {target}"),
        None => format!("{rule} {verb}"),
    }
}

/// Les moniteurs visibles ajoutés pendant la profondeur du fil.
pub fn monitors_added(ctx: &Context) -> Vec<WidgetEvent> {
    let since = ctx.now - TimeDelta::days(EVENT_DAYS);
    ctx.fleet
        .monitors
        .iter()
        .filter_map(|m| {
            let at = m.created_at.as_deref().and_then(parse_time)?;
            (since..=ctx.now).contains(&at).then(|| WidgetEvent {
                at: stamp(at),
                kind: EventKind::Info,
                text: format!("Monitor added: {}", m.name),
                target_id: Some(m.id),
            })
        })
        .collect()
}

/// Débuts et fins des fenêtres de maintenance passés pendant la profondeur du fil.
pub fn maintenance(silences: &[Silence], ctx: &Context) -> Vec<WidgetEvent> {
    let since = ctx.now - TimeDelta::days(EVENT_DAYS);
    let window = since..=ctx.now;
    let mut out = Vec::new();
    for silence in silences {
        for (start, end) in silence.schedule.occurrences_between(since, ctx.now, MAX_OCCURRENCES) {
            if window.contains(&start) {
                out.push(WidgetEvent {
                    at: stamp(start),
                    kind: EventKind::Maint,
                    text: format!("Maintenance started: {}", silence.name),
                    target_id: silence.target_id,
                });
            }
            if window.contains(&end) {
                out.push(WidgetEvent {
                    at: stamp(end),
                    kind: EventKind::Maint,
                    text: format!("Maintenance ended: {}", silence.name),
                    target_id: silence.target_id,
                });
            }
        }
    }
    out
}

/// Les `max` plus récents. Les horodatages ont tous la même forme
/// (`AAAA-MM-JJ HH:MM:SS`) : l'ordre du texte est celui du temps.
pub fn newest(mut events: Vec<WidgetEvent>, max: usize) -> Vec<WidgetEvent> {
    events.sort_by(|a, b| b.at.cmp(&a.at));
    events.truncate(max);
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(at: &str) -> WidgetEvent {
        WidgetEvent {
            at: at.to_string(),
            kind: EventKind::Info,
            text: String::new(),
            target_id: None,
        }
    }

    #[test]
    fn le_fil_garde_les_plus_recents_d_abord() {
        let events = vec![
            event("2026-11-20 10:00:00"),
            event("2026-11-22 09:00:00"),
            event("2026-11-21 23:59:59"),
        ];
        let out = newest(events, 2);
        let at: Vec<&str> = out.iter().map(|e| e.at.as_str()).collect();
        assert_eq!(at, ["2026-11-22 09:00:00", "2026-11-21 23:59:59"]);
    }

    #[test]
    fn le_texte_nomme_la_regle_et_le_moniteur() {
        assert_eq!(alert_text("Host down", "fired", Some("nas")), "Host down fired on nas");
        assert_eq!(alert_text("Host down", "resolved", None), "Host down resolved");
    }
}
