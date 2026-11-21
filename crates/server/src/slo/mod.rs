//! Objectifs de niveau de service (SLO) : budget d'erreur et vitesse de
//! consommation.
//!
//! Un SLO dit « ces moniteurs doivent être disponibles 99,9 % du temps sur
//! trente jours glissants ». Ce qui reste permis (0,1 % de la fenêtre, soit
//! 43,2 minutes) est le **budget d'erreur** ; la **vitesse de consommation**
//! (burn rate) compare l'indisponibilité récente à ce que le budget tolère :
//! à 1×, le budget s'épuise pile à la fin de la fenêtre ; à 14,4×, une heure
//! en consomme 2 % sur trente jours.
//!
//! Disponibilité : les mêmes mesures que les pages de statut et les rapports
//! SLA ([`measure`]) — part des sondes réussies pour une sonde, tranches de
//! cinq minutes vivantes pour un équipement. La disponibilité d'un SLO qui
//! couvre plusieurs moniteurs est la moyenne de leurs disponibilités, chacun
//! avec le même poids, comme la disponibilité du parc dans les rapports.
//!
//! Alertes : méthode multi-fenêtres du livre SRE de Google, avec ses
//! multiplicateurs fixes quelle que soit la fenêtre du SLO :
//!
//! * consommation rapide : au moins 14,4× sur la dernière heure **et** sur les
//!   cinq dernières minutes → sévérité `critical` (« Warning » à l'écran) ;
//! * consommation lente : au moins 6× sur les six dernières heures **et** sur
//!   les trente dernières minutes → sévérité `warning` (« Advisory »).
//!
//! La fenêtre courte fait retomber l'alerte dès que la panne est finie, sans
//! attendre que l'heure ou les six heures se vident.
//!
//! Tout le calcul de ce fichier est pur ; [`measure`] interroge
//! VictoriaMetrics, [`store`] la base, et [`burn_rule`] rend les séries de la
//! règle livrée `slo_burn_rate` au moteur d'alerting.

pub mod measure;
pub mod store;

use std::collections::{BTreeMap, HashMap};

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::SqlitePool;

use crate::alerting::cycle::Annotation;
use crate::alerting::derived::Derived;
use crate::alerting::model::{self, Rule};
use crate::alerting::source::{MetricSource, SeriesPoint};
use crate::reports::layout::Scope;

/// Fenêtres glissantes proposées, en jours.
pub const WINDOW_DAYS: [u32; 4] = [7, 28, 30, 90];
/// Nombre maximal de SLO d'une instance.
pub const MAX_SLOS: i64 = 200;
/// Longueurs maximales du nom et de la description.
pub const MAX_NAME_LEN: usize = 80;
pub const MAX_DESCRIPTION_LEN: usize = 500;

/// Consommation rapide : multiplicateur sur une heure et cinq minutes.
pub const FAST_BURN: f64 = 14.4;
/// Consommation lente : multiplicateur sur six heures et trente minutes.
pub const SLOW_BURN: f64 = 6.0;

/// Fenêtres courtes des vitesses de consommation, en secondes.
pub const WINDOW_5M: i64 = 300;
pub const WINDOW_30M: i64 = 1_800;
pub const WINDOW_1H: i64 = 3_600;
pub const WINDOW_6H: i64 = 21_600;
pub const BURN_WINDOWS: [i64; 4] = [WINDOW_5M, WINDOW_30M, WINDOW_1H, WINDOW_6H];

/// `uid` de la règle livrée qui surveille la consommation des budgets.
pub const RULE_SLO_BURN: &str = "slo_burn_rate";
/// Nom de la série rendue au moteur d'alerting.
pub const BURN_METRIC: &str = "dumbmonit_slo_burn";
/// Étiquette de la série qui porte l'identifiant du SLO (masquée à l'écran,
/// comme tout ce qui finit par `_id`).
pub const SLO_ID_LABEL: &str = "slo_id";

const SECS_PER_DAY: i64 = 86_400;

// --------------------------------------------------------------------------
// Objet
// --------------------------------------------------------------------------

/// Un SLO tel qu'enregistré.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slo {
    pub id: i64,
    pub name: String,
    pub description: String,
    /// Disponibilité visée, en pour cent (strictement entre 0 et 100).
    pub objective: f64,
    /// Fenêtre glissante, en jours (voir [`WINDOW_DAYS`]).
    pub window_days: u32,
    /// Moniteurs et dossiers couverts ; vide : tous les moniteurs.
    pub scope: Scope,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// Un SLO soumis, avant validation.
#[derive(Debug, Clone, PartialEq)]
pub struct SloInput {
    pub name: String,
    pub description: String,
    pub objective: f64,
    pub window_days: u32,
    pub scope: Scope,
    pub enabled: bool,
}

impl SloInput {
    /// Normalise et vérifie ; le message d'erreur est destiné à l'interface.
    pub fn validate(mut self) -> Result<Self, String> {
        self.name = self.name.trim().to_string();
        if self.name.is_empty()
            || self.name.chars().count() > MAX_NAME_LEN
            || self.name.chars().any(char::is_control)
        {
            return Err(format!(
                "The name must be 1 to {MAX_NAME_LEN} characters, without control characters."
            ));
        }
        self.description = self.description.trim().to_string();
        if self.description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(format!("The description is limited to {MAX_DESCRIPTION_LEN} characters."));
        }
        if !(self.objective.is_finite() && self.objective > 0.0 && self.objective < 100.0) {
            return Err("The target must be a percentage above 0 and below 100: at 100%, \
                        there is no error budget left to spend."
                .to_string());
        }
        if !WINDOW_DAYS.contains(&self.window_days) {
            return Err("The window must be 7, 28, 30 or 90 days.".to_string());
        }
        self.scope = self.scope.validate()?;
        Ok(self)
    }
}

/// Ce qu'il faut savoir d'un moniteur pour le rattacher à un SLO et le mesurer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorRef {
    pub id: i64,
    pub kind: String,
    pub group: String,
    pub enabled: bool,
}

impl MonitorRef {
    /// Une sonde (HTTP, ping…) se mesure par ses réussites ; un équipement par
    /// sa présence.
    pub fn is_probe(&self) -> bool {
        crate::api::status_pages::PROBE_KINDS.contains(&self.kind.as_str())
    }
}

/// Les moniteurs actifs couverts par un périmètre.
pub fn members<'a>(scope: &Scope, monitors: &'a [MonitorRef]) -> Vec<&'a MonitorRef> {
    monitors.iter().filter(|m| m.enabled && scope.keeps(m.id, &m.group)).collect()
}

// --------------------------------------------------------------------------
// Calcul (pur)
// --------------------------------------------------------------------------

/// Part d'indisponibilité permise : 0,001 pour 99,9 %.
pub fn allowed_error(objective: f64) -> f64 {
    ((100.0 - objective) / 100.0).max(0.0)
}

/// Budget d'erreur d'une fenêtre, en minutes et en pour cent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Budget {
    /// Indisponibilité permise sur la fenêtre.
    pub total_minutes: f64,
    /// Indisponibilité mesurée sur la fenêtre.
    pub consumed_minutes: f64,
    /// Ce qui reste ; négatif quand l'objectif est déjà manqué.
    pub remaining_minutes: f64,
    /// Ce qui reste, en pour cent du budget ; négatif une fois dépassé.
    pub remaining_percent: f64,
}

/// Budget d'un SLO dont la disponibilité mesurée sur la fenêtre est `attained`
/// (en pour cent).
pub fn budget(objective: f64, window_days: u32, attained: f64) -> Budget {
    let window_minutes = f64::from(window_days) * 1_440.0;
    let total = window_minutes * allowed_error(objective);
    let consumed = window_minutes * (1.0 - attained.clamp(0.0, 100.0) / 100.0);
    let remaining = total - consumed;
    let remaining_percent = if total > 0.0 { remaining / total * 100.0 } else { 0.0 };
    Budget {
        total_minutes: round2(total),
        consumed_minutes: round2(consumed),
        remaining_minutes: round2(remaining),
        remaining_percent: round2(remaining_percent),
    }
}

/// Vitesse de consommation pour une disponibilité mesurée (en pour cent) :
/// l'indisponibilité observée rapportée à celle que l'objectif permet. 1 : le
/// budget s'épuise exactement à la fin de la fenêtre.
pub fn burn_rate(objective: f64, availability: f64) -> Option<f64> {
    let allowed = allowed_error(objective);
    if allowed <= 0.0 || !availability.is_finite() {
        return None;
    }
    let error = 1.0 - availability.clamp(0.0, 100.0) / 100.0;
    Some(error / allowed)
}

/// Vitesses de consommation sur les quatre fenêtres courtes ; `None` sans mesure.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct BurnRates {
    pub m5: Option<f64>,
    pub m30: Option<f64>,
    pub h1: Option<f64>,
    pub h6: Option<f64>,
}

impl BurnRates {
    /// Les vitesses d'un SLO à partir de ses disponibilités sur 5 min, 30 min,
    /// 1 h et 6 h.
    pub fn from_availability(objective: f64, availability: [Option<f64>; 4]) -> Self {
        let rate = |value: Option<f64>| value.and_then(|v| burn_rate(objective, v)).map(round2);
        Self {
            m5: rate(availability[0]),
            m30: rate(availability[1]),
            h1: rate(availability[2]),
            h6: rate(availability[3]),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.m5.is_none() && self.m30.is_none() && self.h1.is_none() && self.h6.is_none()
    }
}

/// Verdict de consommation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Burn {
    #[default]
    None,
    /// Au moins [`SLOW_BURN`] sur 6 h et 30 min : « Advisory » à l'écran.
    Slow,
    /// Au moins [`FAST_BURN`] sur 1 h et 5 min : « Warning » à l'écran.
    Fast,
}

impl Burn {
    /// Valeur de la série d'alerte : 0, 1 (lente) ou 2 (rapide).
    pub fn level(self) -> f64 {
        match self {
            Self::None => 0.0,
            Self::Slow => 1.0,
            Self::Fast => 2.0,
        }
    }
}

/// Verdict multi-fenêtres : la fenêtre longue dit que la consommation compte,
/// la courte qu'elle dure encore. Il faut les deux.
pub fn verdict(rates: &BurnRates) -> Burn {
    let both = |long: Option<f64>, short: Option<f64>, limit: f64| matches!((long, short), (Some(l), Some(s)) if l >= limit && s >= limit);
    if both(rates.h1, rates.m5, FAST_BURN) {
        Burn::Fast
    } else if both(rates.h6, rates.m30, SLOW_BURN) {
        Burn::Slow
    } else {
        Burn::None
    }
}

/// Moyenne des disponibilités des moniteurs mesurés ; `None` si aucun ne l'est.
pub fn aggregate(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let (sum, n) = values
        .into_iter()
        .filter(|v| v.is_finite())
        .fold((0.0, 0usize), |(sum, n), v| (sum + v, n + 1));
    (n > 0).then(|| sum / n as f64)
}

/// Courbe de consommation du budget sur la fenêtre : `[secondes, % restant]`,
/// partant de 100 % au début de la fenêtre. Chaque point `(fin, disponibilité,
/// durée)` retire du budget l'indisponibilité de sa tranche. Le résultat peut
/// passer sous zéro : l'objectif est alors manqué.
pub fn burndown(
    objective: f64,
    window_days: u32,
    start: f64,
    slices: &[(f64, f64, f64)],
) -> Vec<[f64; 2]> {
    let total_secs = f64::from(window_days) * SECS_PER_DAY as f64 * allowed_error(objective);
    let mut sorted: Vec<&(f64, f64, f64)> = slices.iter().filter(|s| s.1.is_finite()).collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::with_capacity(sorted.len() + 1);
    out.push([start, 100.0]);
    let mut spent = 0.0;
    for (end, availability, span) in sorted {
        spent += span.max(0.0) * (1.0 - availability.clamp(0.0, 100.0) / 100.0);
        let remaining = if total_secs > 0.0 { 100.0 * (1.0 - spent / total_secs) } else { 0.0 };
        out.push([*end, round2(remaining)]);
    }
    out
}

/// Pas de la courbe de consommation : une soixantaine de points au plus.
pub fn burndown_step(window_days: u32) -> i64 {
    match window_days {
        0..=7 => 3 * 3_600,
        8..=30 => 12 * 3_600,
        _ => SECS_PER_DAY,
    }
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// « 15.2× » : deux chiffres significatifs suffisent dans un message.
fn times(rate: Option<f64>) -> String {
    match rate {
        Some(rate) if rate >= 10.0 => format!("{rate:.0}×"),
        Some(rate) => format!("{rate:.1}×"),
        None => "n/a".to_string(),
    }
}

/// Phrase du message d'alerte.
pub fn burn_detail(burn: Burn, rates: &BurnRates) -> Option<String> {
    match burn {
        Burn::Fast => Some(format!(
            "fast burn: {} the sustainable rate over 1 h, {} over 5 min",
            times(rates.h1),
            times(rates.m5)
        )),
        Burn::Slow => Some(format!(
            "slow burn: {} the sustainable rate over 6 h, {} over 30 min",
            times(rates.h6),
            times(rates.m30)
        )),
        Burn::None => None,
    }
}

// --------------------------------------------------------------------------
// État d'un SLO (mesures assemblées)
// --------------------------------------------------------------------------

/// Ce que l'interface montre d'un SLO.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SloStatus {
    /// Faux quand VictoriaMetrics n'a pas répondu.
    pub measured: bool,
    /// Moniteurs actifs couverts.
    pub monitors: usize,
    /// Disponibilité atteinte sur la fenêtre, en pour cent.
    pub attained: Option<f64>,
    pub budget: Option<Budget>,
    pub burn: BurnRates,
    pub verdict: Burn,
}

/// Disponibilités par moniteur, une table par fenêtre (en secondes).
pub type Measures = HashMap<i64, HashMap<i64, f64>>;

/// Assemble l'état d'un SLO à partir des mesures par moniteur.
pub fn status_of(slo: &Slo, members: &[&MonitorRef], measures: Option<&Measures>) -> SloStatus {
    let Some(measures) = measures else {
        return SloStatus { monitors: members.len(), ..SloStatus::default() };
    };
    let over = |window: i64| {
        measures
            .get(&window)
            .and_then(|per| aggregate(members.iter().filter_map(|m| per.get(&m.id).copied())))
    };
    let window = i64::from(slo.window_days) * SECS_PER_DAY;
    let attained = over(window).map(|v| (v * 1000.0).round() / 1000.0);
    let burn = BurnRates::from_availability(
        slo.objective,
        [over(WINDOW_5M), over(WINDOW_30M), over(WINDOW_1H), over(WINDOW_6H)],
    );
    SloStatus {
        measured: true,
        monitors: members.len(),
        attained,
        budget: attained.map(|a| budget(slo.objective, slo.window_days, a)),
        verdict: verdict(&burn),
        burn,
    }
}

// --------------------------------------------------------------------------
// Règle d'alerte
// --------------------------------------------------------------------------

/// Évalue la règle de consommation : une série par SLO actif mesuré, de valeur
/// [`Burn::level`]. La règle livrée déclenche à partir de 1 (`warning`) et la
/// sévérité monte à `critical` à 2 (voir `alerting::cycle`).
///
/// Une mesure qui échoue fait échouer la règle : ses alertes sont gelées
/// plutôt que résolues à tort.
pub async fn burn_rule(
    pool: &SqlitePool,
    source: &dyn MetricSource,
    rule: &Rule,
    now: DateTime<Utc>,
) -> Result<Derived> {
    let slos: Vec<Slo> = store::list(pool).await?.into_iter().filter(|s| s.enabled).collect();
    let mut derived = Derived::default();
    if slos.is_empty() {
        return Ok(derived);
    }
    let monitors = store::monitors(pool).await?;
    let mut union: BTreeMap<i64, &MonitorRef> = BTreeMap::new();
    for slo in &slos {
        for member in members(&slo.scope, &monitors) {
            union.insert(member.id, member);
        }
    }
    let all: Vec<&MonitorRef> = union.into_values().collect();
    let measures = measure::availability(source, &all, &BURN_WINDOWS, now).await?;

    for slo in &slos {
        let covered = members(&slo.scope, &monitors);
        let status = status_of(slo, &covered, Some(&measures));
        if status.burn.is_empty() {
            continue;
        }
        let labels: BTreeMap<String, String> = [
            ("__name__".to_string(), BURN_METRIC.to_string()),
            (SLO_ID_LABEL.to_string(), slo.id.to_string()),
            (model::SLO_LABEL.to_string(), slo.name.clone()),
        ]
        .into_iter()
        .collect();
        let fingerprint = model::fingerprint(&rule.uid, &model::identity_key(&labels));
        derived.annotations.insert(
            fingerprint,
            Annotation {
                score: None,
                detail: burn_detail(status.verdict, &status.burn),
                armed: true,
            },
        );
        derived.series.push(SeriesPoint {
            labels,
            value: status.verdict.level(),
            ts_ms: now.timestamp_millis(),
        });
    }
    Ok(derived)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn le_budget_de_99_9_sur_trente_jours_vaut_43_minutes() {
        let full = budget(99.9, 30, 100.0);
        assert!(close(full.total_minutes, 43.2), "{full:?}");
        assert!(close(full.remaining_minutes, 43.2));
        assert!(close(full.remaining_percent, 100.0));

        // 99,95 % atteints : la moitié du budget est partie.
        let half = budget(99.9, 30, 99.95);
        assert!(close(half.consumed_minutes, 21.6), "{half:?}");
        assert!(close(half.remaining_percent, 50.0), "{half:?}");
    }

    #[test]
    fn un_objectif_manque_laisse_un_budget_negatif() {
        let missed = budget(99.0, 7, 98.0);
        assert!(close(missed.total_minutes, 100.8), "{missed:?}");
        assert!(close(missed.consumed_minutes, 201.6));
        assert!(close(missed.remaining_percent, -100.0), "{missed:?}");
    }

    #[test]
    fn la_vitesse_rapporte_l_indisponibilite_a_ce_qui_est_permis() {
        assert!(close(burn_rate(99.9, 100.0).unwrap(), 0.0));
        assert!(close(burn_rate(99.9, 99.9).unwrap(), 1.0));
        // 14,4× sur 99,9 % : 1,44 % d'indisponibilité.
        assert!(close(burn_rate(99.9, 98.56).unwrap(), 14.4));
        assert!(close(burn_rate(99.0, 0.0).unwrap(), 100.0));
        assert!(burn_rate(99.9, f64::NAN).is_none());
    }

    #[test]
    fn la_consommation_rapide_exige_l_heure_et_les_cinq_minutes() {
        let rates = BurnRates { m5: Some(20.0), m30: Some(20.0), h1: Some(15.0), h6: Some(3.0) };
        assert_eq!(verdict(&rates), Burn::Fast);
        // La panne est finie : les cinq dernières minutes sont propres.
        let over = BurnRates { m5: Some(0.0), ..rates };
        assert_eq!(verdict(&over), Burn::None, "6 h below 6×: nothing");
        // Une heure chargée mais déjà retombée, six heures lourdes : avis.
        let slow = BurnRates { m5: Some(0.0), m30: Some(7.0), h1: Some(15.0), h6: Some(6.5) };
        assert_eq!(verdict(&slow), Burn::Slow);
    }

    #[test]
    fn la_consommation_lente_exige_six_heures_et_trente_minutes() {
        let rates = BurnRates { m5: Some(8.0), m30: Some(8.0), h1: Some(8.0), h6: Some(6.0) };
        assert_eq!(verdict(&rates), Burn::Slow);
        let short_clean = BurnRates { m30: Some(5.9), ..rates };
        assert_eq!(verdict(&short_clean), Burn::None);
        // Sans mesure, pas de verdict.
        assert_eq!(verdict(&BurnRates::default()), Burn::None);
        let partial = BurnRates { h1: Some(50.0), ..BurnRates::default() };
        assert_eq!(verdict(&partial), Burn::None);
    }

    #[test]
    fn les_seuils_sont_inclusifs() {
        let fast = BurnRates { m5: Some(FAST_BURN), h1: Some(FAST_BURN), ..BurnRates::default() };
        assert_eq!(verdict(&fast), Burn::Fast);
        assert_eq!(Burn::Fast.level(), 2.0);
        assert_eq!(Burn::Slow.level(), 1.0);
        assert_eq!(Burn::None.level(), 0.0);
    }

    #[test]
    fn la_courbe_part_de_cent_et_descend_avec_les_pannes() {
        // 7 jours à 99 % : budget = 7 × 86 400 × 0,01 = 6 048 s.
        let day = 86_400.0;
        let slices = [
            (2.0 * day, 100.0, day),
            (day, 99.5, day), // 432 s perdues
            (3.0 * day, 100.0, day),
        ];
        let curve = burndown(99.0, 7, 0.0, &slices);
        assert_eq!(curve[0], [0.0, 100.0]);
        assert_eq!(curve.len(), 4);
        assert_eq!(curve[1][0], day, "sorted by time");
        assert!(close(curve[1][1], 100.0 * (1.0 - 432.0 / 6048.0)), "{curve:?}");
        assert!(close(curve[3][1], curve[1][1]), "clean days spend nothing");
        // Une journée entière de panne dépasse le budget.
        let down = burndown(99.0, 7, 0.0, &[(day, 0.0, day)]);
        assert!(down[1][1] < 0.0, "{down:?}");
    }

    #[test]
    fn la_moyenne_ignore_les_valeurs_absentes() {
        assert_eq!(aggregate(Vec::<f64>::new()), None);
        assert_eq!(aggregate([100.0, 98.0]), Some(99.0));
        assert_eq!(aggregate([f64::NAN, 50.0]), Some(50.0));
    }

    fn slo(objective: f64, window_days: u32) -> Slo {
        Slo {
            id: 1,
            name: "API".to_string(),
            description: String::new(),
            objective,
            window_days,
            scope: Scope::default(),
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn monitor(id: i64, kind: &str, group: &str) -> MonitorRef {
        MonitorRef { id, kind: kind.to_string(), group: group.to_string(), enabled: true }
    }

    #[test]
    fn l_etat_assemble_les_moniteurs_du_perimetre() {
        let monitors =
            [monitor(1, "http", "Web"), monitor(2, "ping", "Web"), monitor(3, "snmp", "")];
        let mut target = slo(99.0, 7);
        target.scope = Scope { targets: vec![], groups: vec!["Web".to_string()] };
        let covered = members(&target.scope, &monitors);
        assert_eq!(covered.iter().map(|m| m.id).collect::<Vec<_>>(), [1, 2]);

        let mut measures: Measures = HashMap::new();
        measures.insert(7 * 86_400, [(1, 100.0), (2, 98.0), (3, 0.0)].into_iter().collect());
        measures.insert(WINDOW_5M, [(1, 0.0), (2, 0.0)].into_iter().collect());
        measures.insert(WINDOW_1H, [(1, 80.0), (2, 80.0)].into_iter().collect());
        let status = status_of(&target, &covered, Some(&measures));
        assert!(status.measured);
        assert_eq!(status.monitors, 2);
        assert_eq!(status.attained, Some(99.0), "the device outside the scope is ignored");
        assert!(close(status.budget.unwrap().remaining_percent, 0.0));
        assert_eq!(status.burn.h1, Some(20.0));
        assert_eq!(status.burn.m5, Some(100.0));
        assert_eq!(status.verdict, Burn::Fast);
        assert!(burn_detail(status.verdict, &status.burn).unwrap().starts_with("fast burn"));

        let unknown = status_of(&target, &covered, None);
        assert!(!unknown.measured);
        assert_eq!(unknown.attained, None);
    }

    #[test]
    fn la_validation_refuse_ce_qui_n_a_pas_de_sens() {
        let good = SloInput {
            name: "  API  ".to_string(),
            description: String::new(),
            objective: 99.9,
            window_days: 30,
            scope: Scope::default(),
            enabled: true,
        };
        assert_eq!(good.clone().validate().unwrap().name, "API");
        for objective in [0.0, 100.0, 101.0, f64::NAN] {
            assert!(SloInput { objective, ..good.clone() }.validate().is_err(), "{objective}");
        }
        assert!(SloInput { window_days: 14, ..good.clone() }.validate().is_err());
        assert!(SloInput { name: " ".to_string(), ..good.clone() }.validate().is_err());
        assert!(SloInput { name: "a\nb".to_string(), ..good.clone() }.validate().is_err());
    }

    #[test]
    fn le_pas_de_la_courbe_garde_une_soixantaine_de_points() {
        for days in WINDOW_DAYS {
            let points = i64::from(days) * SECS_PER_DAY / burndown_step(days);
            assert!((50..=100).contains(&points), "{days} d: {points} points");
        }
    }
}
