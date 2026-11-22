//! Déroulé d'une fenêtre en occurrences datées, pour le calendrier des
//! maintenances. Réutilise l'horloge et la récurrence de `silence.rs` : la même
//! règle sert à dire « actif maintenant » et à dessiner le mois.

use chrono::{DateTime, Days, Utc};

use super::Schedule;

impl Schedule {
    /// Occurrences `(début, fin)` qui rencontrent `[from, to)`, dans l'ordre,
    /// au plus `limit`. Une fenêtre « jusqu'au rétablissement » n'a pas de fin :
    /// elle n'est pas déroulée.
    pub fn occurrences_between(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: usize,
    ) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
        if to <= from || limit == 0 {
            return Vec::new();
        }
        if let Self::Once { starts_at, ends_at } = self {
            return if *starts_at < to && *ends_at > from {
                vec![(*starts_at, *ends_at)]
            } else {
                Vec::new()
            };
        }
        let Some(clock) = self.clock() else { return Vec::new() };
        let duration = self.duration();
        let start_minute = self.start_minute();
        // Une occurrence ouverte avant `from` peut encore le recouvrir.
        let back = duration.num_days().max(0) as u64 + 1;
        let Some(first) = clock.date_of(from).checked_sub_days(Days::new(back)) else {
            return Vec::new();
        };
        let last = clock.date_of(to);
        let mut out = Vec::new();
        let mut date = first;
        while date <= last && out.len() < limit {
            if self.selects(date)
                && let Some(start) = clock.instant_at(date, start_minute)
            {
                let end = start + duration;
                if start < to && end > from {
                    out.push((start, end));
                }
            }
            let Some(next) = date.checked_add_days(Days::new(1)) else { break };
            date = next;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).expect("valid ISO").with_timezone(&Utc)
    }

    #[test]
    fn une_fenetre_hebdomadaire_est_deroulee_sur_le_mois() {
        // Dimanche 2 h à 4 h, UTC.
        let schedule = Schedule::Weekly {
            days: vec![6],
            start_minute: 120,
            end_minute: 240,
            utc_offset_minutes: 0,
            timezone: None,
        };
        let found =
            schedule.occurrences_between(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"), 100);
        // Dimanches de novembre 2026 : 1, 8, 15, 22, 29.
        assert_eq!(found.len(), 5);
        assert_eq!(found[0], (at("2026-11-01T02:00:00Z"), at("2026-11-01T04:00:00Z")));
    }

    #[test]
    fn une_occurrence_a_cheval_sur_la_borne_est_retenue() {
        let schedule = Schedule::Weekly {
            days: vec![5],
            start_minute: 23 * 60,
            end_minute: 60,
            utc_offset_minutes: 0,
            timezone: None,
        };
        // Samedi 2026-11-07 23 h -> dimanche 1 h ; la borne tombe à minuit.
        let found =
            schedule.occurrences_between(at("2026-11-08T00:00:00Z"), at("2026-11-09T00:00:00Z"), 10);
        assert_eq!(found, vec![(at("2026-11-07T23:00:00Z"), at("2026-11-08T01:00:00Z"))]);
    }

    #[test]
    fn le_dernier_vendredi_du_mois_et_la_limite() {
        let schedule = Schedule::Monthly {
            days: vec![],
            nth_weekdays: vec![super::super::NthWeekday { nth: -1, weekday: 4 }],
            start_minute: 0,
            duration_minutes: 60,
            utc_offset_minutes: 0,
            timezone: None,
        };
        let found =
            schedule.occurrences_between(at("2026-01-01T00:00:00Z"), at("2027-01-01T00:00:00Z"), 100);
        assert_eq!(found.len(), 12);
        assert_eq!(found[10].0, at("2026-11-27T00:00:00Z"));
        let capped =
            schedule.occurrences_between(at("2026-01-01T00:00:00Z"), at("2027-01-01T00:00:00Z"), 3);
        assert_eq!(capped.len(), 3);
    }

    #[test]
    fn ponctuelle_hors_borne_et_sans_fin_ignorees() {
        let once = Schedule::Once {
            starts_at: at("2026-11-10T10:00:00Z"),
            ends_at: at("2026-11-10T11:00:00Z"),
        };
        assert!(
            once.occurrences_between(at("2026-11-11T00:00:00Z"), at("2026-11-12T00:00:00Z"), 5)
                .is_empty()
        );
        assert_eq!(
            once.occurrences_between(at("2026-11-10T10:30:00Z"), at("2026-11-12T00:00:00Z"), 5)
                .len(),
            1
        );
        let recovery = Schedule::Recovery { starts_at: at("2026-11-10T10:00:00Z") };
        assert!(
            recovery
                .occurrences_between(at("2026-11-01T00:00:00Z"), at("2026-12-01T00:00:00Z"), 5)
                .is_empty()
        );
    }
}
