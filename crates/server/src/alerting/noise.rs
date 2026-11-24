//! Rapport de bruit : quelles règles parlent trop, et en combien de temps on
//! leur répond.
//!
//! L'unité est l'épisode : une transition vers `firing` dans l'historique,
//! avec ce qui l'a suivi — sa résolution, son premier acquittement, et le
//! nombre de notifications qu'il a coûté (déclenchement, rappels, escalades,
//! résolution). La lecture en base vit dans [`crate::db::alert_insights`] ;
//! ici, seulement l'agrégation, sans entrée/sortie.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::alerting::model::TargetId;

/// Un épisode `firing` de l'historique.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub rule_uid: String,
    pub target_id: Option<TargetId>,
    pub fired_at: DateTime<Utc>,
    /// Premier acquittement (ou « j'ai compris ») de l'épisode.
    pub acked_at: Option<DateTime<Utc>>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub notifications: u32,
}

/// Une équipe et les équipements que ses droits couvrent.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamTargets {
    pub id: i64,
    pub name: String,
    pub targets: HashSet<TargetId>,
}

/// Chiffres d'un ensemble d'épisodes.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct NoiseStats {
    /// Épisodes `firing` dans la fenêtre.
    pub fired: u32,
    pub notifications: u32,
    /// Épisodes acquittés : la base du délai moyen d'acquittement.
    pub acknowledged: u32,
    /// Épisodes résolus : la base du délai moyen de résolution.
    pub resolved: u32,
    /// Délai moyen avant acquittement (MTTA), en secondes.
    pub mtta_secs: Option<i64>,
    /// Délai moyen avant résolution (MTTR), en secondes.
    pub mttr_secs: Option<i64>,
}

/// Les chiffres d'une règle.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleNoise {
    pub rule_uid: String,
    pub stats: NoiseStats,
}

/// Les chiffres d'une équipe.
#[derive(Debug, Clone, PartialEq)]
pub struct TeamNoise {
    pub team_id: i64,
    pub team_name: String,
    pub stats: NoiseStats,
}

/// Le rapport complet.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub overall: NoiseStats,
    /// Les règles les plus bruyantes d'abord (notifications, puis épisodes).
    pub rules: Vec<RuleNoise>,
    /// Une ligne par équipe, épisodes de ses équipements seulement.
    pub teams: Vec<TeamNoise>,
}

/// Sommes en cours, avant les moyennes.
#[derive(Debug, Default)]
struct Tally {
    fired: u32,
    notifications: u32,
    acknowledged: u32,
    ack_secs: i64,
    resolved: u32,
    resolve_secs: i64,
}

impl Tally {
    fn add(&mut self, episode: &Episode) {
        self.fired += 1;
        self.notifications = self.notifications.saturating_add(episode.notifications);
        // Un acquittement posé pendant l'attente, avant le déclenchement,
        // compte pour zéro : on ne réagit pas plus vite que l'instant présent.
        if let Some(acked) = episode.acked_at {
            self.acknowledged += 1;
            self.ack_secs += acked.signed_duration_since(episode.fired_at).num_seconds().max(0);
        }
        if let Some(resolved) = episode.resolved_at {
            self.resolved += 1;
            self.resolve_secs +=
                resolved.signed_duration_since(episode.fired_at).num_seconds().max(0);
        }
    }

    fn stats(&self) -> NoiseStats {
        let mean = |sum: i64, count: u32| (count > 0).then(|| sum / i64::from(count));
        NoiseStats {
            fired: self.fired,
            notifications: self.notifications,
            acknowledged: self.acknowledged,
            resolved: self.resolved,
            mtta_secs: mean(self.ack_secs, self.acknowledged),
            mttr_secs: mean(self.resolve_secs, self.resolved),
        }
    }
}

/// Agrège les épisodes : tout, par règle (les `max_rules` plus bruyantes), et
/// par équipe. Un équipement couvert par deux équipes compte pour les deux ;
/// un épisode sans équipement ne compte pour aucune.
pub fn summarize(episodes: &[Episode], teams: &[TeamTargets], max_rules: usize) -> Report {
    let mut overall = Tally::default();
    let mut by_rule: HashMap<&str, Tally> = HashMap::new();
    let mut by_team: Vec<Tally> = teams.iter().map(|_| Tally::default()).collect();

    for episode in episodes {
        overall.add(episode);
        by_rule.entry(episode.rule_uid.as_str()).or_default().add(episode);
        if let Some(target) = episode.target_id {
            for (team, tally) in teams.iter().zip(by_team.iter_mut()) {
                if team.targets.contains(&target) {
                    tally.add(episode);
                }
            }
        }
    }

    let mut rules: Vec<RuleNoise> = by_rule
        .into_iter()
        .map(|(uid, tally)| RuleNoise { rule_uid: uid.to_string(), stats: tally.stats() })
        .collect();
    rules.sort_by(|a, b| {
        b.stats
            .notifications
            .cmp(&a.stats.notifications)
            .then(b.stats.fired.cmp(&a.stats.fired))
            .then_with(|| a.rule_uid.cmp(&b.rule_uid))
    });
    rules.truncate(max_rules);

    let mut teams: Vec<TeamNoise> = teams
        .iter()
        .zip(by_team)
        .map(|(team, tally)| TeamNoise {
            team_id: team.id,
            team_name: team.name.clone(),
            stats: tally.stats(),
        })
        .collect();
    teams.sort_by(|a, b| {
        b.stats
            .notifications
            .cmp(&a.stats.notifications)
            .then(b.stats.fired.cmp(&a.stats.fired))
            .then_with(|| a.team_name.cmp(&b.team_name))
    });

    Report { overall: overall.stats(), rules, teams }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + seconds, 0).expect("valid timestamp")
    }

    fn episode(rule: &str, target: Option<TargetId>, fired: i64) -> Episode {
        Episode {
            rule_uid: rule.to_string(),
            target_id: target,
            fired_at: at(fired),
            acked_at: None,
            resolved_at: None,
            notifications: 1,
        }
    }

    #[test]
    fn les_delais_moyens_ne_comptent_que_les_episodes_concernes() {
        let mut acked = episode("disk", Some(1), 0);
        acked.acked_at = Some(at(120));
        acked.resolved_at = Some(at(600));
        let mut resolved = episode("disk", Some(1), 1_000);
        resolved.resolved_at = Some(at(1_200));
        let open = episode("disk", Some(1), 2_000);

        let report = summarize(&[acked, resolved, open], &[], 10);
        assert_eq!(report.overall.fired, 3);
        assert_eq!(report.overall.acknowledged, 1);
        assert_eq!(report.overall.mtta_secs, Some(120));
        assert_eq!(report.overall.resolved, 2);
        assert_eq!(report.overall.mttr_secs, Some((600 + 200) / 2));
    }

    #[test]
    fn un_acquittement_anterieur_au_declenchement_compte_pour_zero() {
        let mut early = episode("cpu", None, 300);
        early.acked_at = Some(at(100));
        let report = summarize(&[early], &[], 10);
        assert_eq!(report.overall.mtta_secs, Some(0));
    }

    #[test]
    fn sans_acquittement_ni_resolution_les_delais_sont_absents() {
        let report = summarize(&[episode("cpu", None, 0)], &[], 10);
        assert_eq!(report.overall.mtta_secs, None);
        assert_eq!(report.overall.mttr_secs, None);
        assert_eq!(summarize(&[], &[], 10), Report::default());
    }

    #[test]
    fn les_regles_les_plus_bruyantes_viennent_en_tete_et_la_liste_est_bornee() {
        let mut chatty = episode("cpu", Some(1), 0);
        chatty.notifications = 12;
        let episodes = vec![
            episode("disk", Some(1), 0),
            episode("disk", Some(2), 10),
            chatty,
            episode("temp", Some(3), 0),
        ];
        let report = summarize(&episodes, &[], 2);
        let order: Vec<&str> = report.rules.iter().map(|r| r.rule_uid.as_str()).collect();
        assert_eq!(order, vec!["cpu", "disk"]);
        assert_eq!(report.rules[1].stats.fired, 2);
        assert_eq!(report.overall.notifications, 15, "the total ignores the cut");
    }

    #[test]
    fn chaque_equipe_ne_compte_que_ses_equipements() {
        let teams = vec![
            TeamTargets { id: 1, name: "Network".into(), targets: [1, 2].into_iter().collect() },
            TeamTargets { id: 2, name: "Storage".into(), targets: [2, 3].into_iter().collect() },
            TeamTargets { id: 3, name: "Idle".into(), targets: HashSet::new() },
        ];
        let episodes = vec![
            episode("a", Some(1), 0),
            episode("a", Some(2), 0),
            episode("a", Some(3), 0),
            episode("a", None, 0),
        ];
        let report = summarize(&episodes, &teams, 10);
        let fired: Vec<(&str, u32)> =
            report.teams.iter().map(|t| (t.team_name.as_str(), t.stats.fired)).collect();
        assert_eq!(fired, vec![("Network", 2), ("Storage", 2), ("Idle", 0)]);
        assert_eq!(report.overall.fired, 4);
    }
}
