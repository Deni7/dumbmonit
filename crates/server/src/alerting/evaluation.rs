//! Un pas d'évaluation d'une série : condition jugée, puis machine à états.
//!
//! C'est le seul endroit où se décide, pour une valeur donnée à un instant
//! donné, si une alerte à seuil tient, attend ou se résout. Deux appelants :
//! le moteur ([`crate::alerting::cycle::plan_cycle`]), à chaque cycle, et le
//! rejeu sur l'historique ([`crate::alerting::replay`]), sur une grille de
//! temps passée. Partager ces fonctions plutôt que de les recopier garantit
//! que « tester sur l'historique » dit exactement ce que le moteur aurait
//! fait : même hystérésis, même `for`, même horloge.
//!
//! Ce qui suit le pas — suppression, silences, acquittement — reste au cycle :
//! ce sont des surcouches qui n'ont pas d'existence dans le passé rejoué.

use chrono::{DateTime, Utc};

use crate::alerting::machine::{self, AlertState, Transition};
use crate::alerting::model::Rule;
use crate::alerting::overrides::Effective;

/// Vrai si la condition tenait au pas précédent : elle est datée, que
/// l'alerte soit encore en attente ou déjà partie. C'est ce qui arme
/// l'hystérésis.
pub fn was_active(before: &AlertState) -> bool {
    before.condition_since.is_some()
}

/// Condition d'une règle comparée à un seuil (seuil, prédictive, journal,
/// prévision, consommation de budget, dérive), surcharge par équipement et
/// hystérésis comprises.
pub fn threshold_holds(rule: &Rule, effective: &Effective, value: f64, active: bool) -> bool {
    rule.operator.holds(value, effective.threshold, effective.clear_threshold, active)
}

/// Fait avancer l'état d'un pas, la condition déjà jugée : c'est ici que
/// s'applique la durée de confirmation (`for`) de la règle.
pub fn step(
    before: &AlertState,
    condition: bool,
    now: DateTime<Utc>,
    rule: &Rule,
) -> (AlertState, Option<Transition>) {
    machine::advance(before, condition, now, rule.for_duration)
}

/// Pas complet d'une série à seuil : condition, puis machine.
///
/// `value` à `None` : la série n'existe pas à cet instant (aucun point dans
/// la fenêtre de recherche). La condition est alors fausse, exactement comme
/// pour une série disparue d'un cycle du moteur, et la valeur précédente est
/// conservée.
pub fn threshold_step(
    before: &AlertState,
    rule: &Rule,
    effective: &Effective,
    value: Option<f64>,
    now: DateTime<Utc>,
) -> (AlertState, Option<Transition>) {
    let active = was_active(before);
    let condition = value.is_some_and(|value| threshold_holds(rule, effective, value, active));
    let (mut state, transition) = step(before, condition, now, rule);
    if let Some(value) = value {
        state.value = Some(value);
    }
    (state, transition)
}

/// Ce que le moteur consigne après avoir pris en charge une notification
/// (`db::alerts::mark_notified`) : la date du dernier envoi et leur nombre.
pub fn mark_notified(state: &mut AlertState, now: DateTime<Utc>) {
    state.last_notified_at = Some(now);
    state.notify_count = state.notify_count.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::alerting::machine::Phase;
    use crate::alerting::model::{AnomalyParams, Operator, RuleKind, Severity, TargetSelector};

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + seconds, 0).expect("valid timestamp")
    }

    fn rule(for_secs: u64) -> Rule {
        Rule {
            id: 1,
            uid: "cpu".to_string(),
            name: "CPU".to_string(),
            description: String::new(),
            kind: RuleKind::Threshold,
            query: "q".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(for_secs),
            severity: Severity::Warning,
            selector: TargetSelector::All,
            channels: Vec::new(),
            params: AnomalyParams::default(),
            unit: "%".to_string(),
            repeat_interval: None,
            escalate_after: None,
            enabled: true,
            builtin: false,
        }
    }

    fn effective(rule: &Rule) -> Effective {
        Effective {
            threshold: rule.threshold,
            clear_threshold: rule.clear_threshold,
            enabled: true,
        }
    }

    #[test]
    fn l_hysteresis_tient_l_alerte_entre_les_deux_seuils() {
        let rule = rule(0);
        let eff = effective(&rule);
        let (firing, _) = threshold_step(&AlertState::default(), &rule, &eff, Some(95.0), at(0));
        assert_eq!(firing.phase, Phase::Firing);
        let (still, transition) = threshold_step(&firing, &rule, &eff, Some(85.0), at(30));
        assert_eq!(still.phase, Phase::Firing, "85 stays above the clear line");
        assert_eq!(transition, None);
        let (resolved, _) = threshold_step(&still, &rule, &eff, Some(79.0), at(60));
        assert_eq!(resolved.phase, Phase::Resolved);
    }

    #[test]
    fn une_serie_absente_vaut_une_condition_fausse_et_garde_sa_valeur() {
        let rule = rule(0);
        let eff = effective(&rule);
        let (firing, _) = threshold_step(&AlertState::default(), &rule, &eff, Some(95.0), at(0));
        let (resolved, _) = threshold_step(&firing, &rule, &eff, None, at(30));
        assert_eq!(resolved.phase, Phase::Resolved);
        assert_eq!(resolved.value, Some(95.0));
    }

    #[test]
    fn le_for_de_la_regle_est_applique() {
        let rule = rule(60);
        let eff = effective(&rule);
        let (pending, _) = threshold_step(&AlertState::default(), &rule, &eff, Some(95.0), at(0));
        assert_eq!(pending.phase, Phase::Pending);
        let (firing, _) = threshold_step(&pending, &rule, &eff, Some(95.0), at(60));
        assert_eq!(firing.phase, Phase::Firing);
    }

    #[test]
    fn une_notification_est_comptee_et_datee() {
        let mut state = AlertState::default();
        mark_notified(&mut state, at(10));
        mark_notified(&mut state, at(20));
        assert_eq!(state.notify_count, 2);
        assert_eq!(state.last_notified_at, Some(at(20)));
    }
}
