//! Déploiement par vagues des mises à jour des agents.
//!
//! Sans vague configurée, rien ne change : c'est le déploiement « pilotes
//! d'abord » de [`super::selfupdate`] (machines marquées canari, puis tout le
//! monde). Avec des vagues, l'administrateur ordonne le parc : un canari de
//! quelques machines, puis des pourcentages successifs, ou par site, dossier
//! (`group_name` de la cible) et étiquette. Les machines que aucune vague ne
//! retient forment une dernière vague implicite, « tout le reste ».
//!
//! Tout ce module est pur : [`plan`] reçoit la politique et l'état des machines
//! et rend l'affectation, l'avancement et l'éventuel arrêt. Le serveur n'en
//! garde que la politique (clé `agent.update_rollout` de `settings`), la pause
//! et l'instant avant lequel les échecs sont pardonnés ; l'avancement se
//! déduit à chaque fois de ce que les agents rapportent (empreinte du binaire,
//! issue de la dernière tentative), donc rien ne se désynchronise.
//!
//! Règles :
//!
//! * une vague s'ouvre quand la précédente n'a plus de machine en attente, et
//!   que la dernière machine mise à jour y tourne le nouveau binaire depuis le
//!   délai demandé ;
//! * si, dans une vague, la part des machines en échec (échec, retour arrière,
//!   refus) parmi celles qui ont tenté la mise à jour atteint le seuil, la
//!   propagation s'arrête : plus aucune offre automatique, jusqu'à la reprise
//!   manuelle ;
//! * la pause manuelle retient aussi toutes les offres automatiques ;
//! * une machine exclue (« mises à jour : désactivées » pour elle, ce qui
//!   revient à épingler sa version) n'entre dans aucune vague ; « Update now »
//!   passe outre tout cela, comme avant.

use std::collections::BTreeMap;

use anyhow::Result;
use dumbmonit_proto::TargetId;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Clé de `settings` : la politique, la pause et les échecs pardonnés.
pub const SETTINGS_KEY: &str = "agent.update_rollout";

/// Nombre maximal de vagues explicites.
pub const MAX_WAVES: usize = 20;
/// Délai maximal entre deux vagues : une semaine.
pub const MAX_DELAY_MINUTES: u32 = 7 * 24 * 60;
/// Délai par défaut entre deux vagues.
pub const DEFAULT_DELAY_MINUTES: u32 = 60;
/// Seuil d'arrêt par défaut : un cinquième des machines d'une vague.
pub const DEFAULT_MAX_FAILURE_PERCENT: u32 = 20;
/// Nombre maximal de machines d'une vague « n machines ».
pub const MAX_COUNT: u32 = 100_000;
/// Longueur maximale d'un nom de site, de dossier ou d'étiquette.
pub const MAX_NAME_LEN: usize = 80;

/// Une vague : comment elle choisit ses machines parmi celles que les vagues
/// précédentes n'ont pas déjà prises.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum Wave {
    /// `value` machines de plus (un canari de 1 ou 2 machines, par exemple).
    Count { value: u32 },
    /// Assez de machines pour que les vagues déjà ouvertes et celle-ci
    /// couvrent `value` % du parc (cumulatif : 5, 25, 100).
    Percent { value: u32 },
    /// Les machines d'un site (`site` déclaré par l'agent).
    Site { value: String },
    /// Les machines d'un dossier de la page des équipements.
    Group { value: String },
    /// Les machines dont l'équipement porte cette étiquette, avec cette
    /// valeur si elle est donnée.
    Tag {
        key: String,
        #[serde(default)]
        value: Option<String>,
    },
}

/// Politique de déploiement, état de pause compris.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Rollout {
    /// Vagues ordonnées ; vide : le comportement historique (pilotes d'abord).
    pub waves: Vec<Wave>,
    /// Minutes entre la fin d'une vague et l'ouverture de la suivante.
    pub delay_minutes: u32,
    /// Part de machines en échec, dans une vague, qui arrête la propagation ;
    /// 0 : ne jamais s'arrêter.
    pub max_failure_percent: u32,
    /// Pause manuelle : plus aucune offre automatique.
    pub paused: bool,
    /// Les échecs rapportés avant cet instant (ms) ne comptent plus : posé à
    /// la reprise d'un arrêt, pour qu'il ne se redéclenche pas aussitôt.
    pub forgiven_before_ms: i64,
}

impl Default for Rollout {
    fn default() -> Self {
        Self {
            waves: Vec::new(),
            delay_minutes: DEFAULT_DELAY_MINUTES,
            max_failure_percent: DEFAULT_MAX_FAILURE_PERCENT,
            paused: false,
            forgiven_before_ms: 0,
        }
    }
}

fn name_error(text: &str, what: &str) -> Result<(), String> {
    if text.trim().is_empty() || text.trim() != text || text.chars().count() > MAX_NAME_LEN {
        return Err(format!(
            "A wave {what} must be 1 to {MAX_NAME_LEN} characters, without spaces around."
        ));
    }
    Ok(())
}

impl Rollout {
    /// Vérifie la politique ; le message est celui que l'API renvoie.
    pub fn validate(&self) -> Result<(), String> {
        if self.waves.len() > MAX_WAVES {
            return Err(format!("At most {MAX_WAVES} waves."));
        }
        for wave in &self.waves {
            match wave {
                Wave::Count { value } if !(1..=MAX_COUNT).contains(value) => {
                    return Err(format!("A wave of agents holds 1 to {MAX_COUNT} agents."));
                }
                Wave::Percent { value } if !(1..=100).contains(value) => {
                    return Err("A percentage wave is between 1 and 100.".into());
                }
                Wave::Site { value } => name_error(value, "site")?,
                Wave::Group { value } => name_error(value, "group")?,
                Wave::Tag { key, value } => {
                    name_error(key, "tag key")?;
                    if let Some(value) = value {
                        name_error(value, "tag value")?;
                    }
                }
                _ => {}
            }
        }
        if self.delay_minutes > MAX_DELAY_MINUTES {
            return Err(format!(
                "The delay between waves is limited to {MAX_DELAY_MINUTES} minutes."
            ));
        }
        if self.max_failure_percent > 100 {
            return Err("The failure threshold is a percentage (0 to 100).".into());
        }
        Ok(())
    }
}

/// Politique rangée, ou celle par défaut (aussi quand le réglage est illisible :
/// mieux vaut le comportement historique qu'une mise à jour qui s'arrête).
pub async fn load(pool: &SqlitePool) -> Result<Rollout> {
    match crate::auth::settings::get::<Rollout>(pool, SETTINGS_KEY).await {
        Ok(stored) => Ok(stored.unwrap_or_default()),
        Err(error) => {
            tracing::warn!(error = %format!("{error:#}"), "agent rollout settings unreadable");
            Ok(Rollout::default())
        }
    }
}

pub async fn save(pool: &SqlitePool, rollout: &Rollout) -> Result<()> {
    crate::auth::settings::set(pool, SETTINGS_KEY, rollout).await
}

// ------------------------------------------------------------------- plan

/// Où en est une machine, pour le déploiement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberState {
    /// Tourne le binaire attendu, depuis cet instant (ms).
    Done { since_ms: i64 },
    /// Doit encore se mettre à jour.
    Pending,
    /// A rapporté un échec pour ce binaire.
    Failed,
    /// Ne compte ni pour l'avancement ni pour les échecs : plus récente que le
    /// serveur, ou échec pardonné.
    Idle,
}

/// Une machine candidate au déploiement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: TargetId,
    /// Machine pilote : toujours dans la première vague.
    pub canary: bool,
    pub site: Option<String>,
    pub group: String,
    pub tags: BTreeMap<String, String>,
    pub state: MemberState,
}

/// Pourquoi une machine attend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    Paused,
    Halted,
    /// Sa vague n'est pas encore ouverte.
    Wave,
}

/// Une vague, une fois les machines affectées.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WavePlan {
    /// `None` : la dernière vague implicite, « tout le reste ».
    pub wave: Option<Wave>,
    pub members: Vec<TargetId>,
    pub done: usize,
    pub pending: usize,
    pub failed: usize,
    /// Dernière machine mise à jour de la vague : depuis quand (ms).
    pub latest_since_ms: Option<i64>,
}

/// L'arrêt automatique : où et pourquoi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Halt {
    /// Rang de la vague (0 = la première).
    pub wave: usize,
    pub failed: usize,
    /// Machines de la vague qui ont tenté (réussi ou échoué).
    pub attempted: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub waves: Vec<WavePlan>,
    /// Les vagues de rang inférieur à ce nombre peuvent recevoir l'offre.
    pub open_count: usize,
    /// Quand la prochaine vague s'ouvre, si seul le délai l'en sépare (ms).
    pub next_at_ms: Option<i64>,
    pub halt: Option<Halt>,
    pub paused: bool,
    wave_of: BTreeMap<TargetId, usize>,
}

impl Plan {
    /// Rang (0 = la première) de la vague d'une machine.
    pub fn wave_of(&self, id: TargetId) -> Option<usize> {
        self.wave_of.get(&id).copied()
    }

    /// Première vague ouverte qui a encore des machines en attente.
    pub fn current(&self) -> Option<usize> {
        (0..self.open_count.min(self.waves.len())).find(|&k| self.waves[k].pending > 0)
    }

    /// Toutes les vagues sont ouvertes, rien ne retient l'offre.
    pub fn fully_open(&self) -> bool {
        !self.paused && self.halt.is_none() && self.open_count >= self.waves.len()
    }

    /// Ce qui retient l'offre automatique d'une machine ; `None` : elle peut
    /// la recevoir. Une machine hors plan (revenue après une longue absence)
    /// suit la dernière vague.
    pub fn hold_for(&self, id: TargetId) -> Option<Hold> {
        if self.paused {
            return Some(Hold::Paused);
        }
        if self.halt.is_some() {
            return Some(Hold::Halted);
        }
        let wave = self.wave_of(id).unwrap_or(self.waves.len().saturating_sub(1));
        (wave >= self.open_count).then_some(Hold::Wave)
    }
}

/// Ordre stable et pseudo-aléatoire des machines : un pourcentage ne prend pas
/// toujours les mêmes machines « du début de la liste », et l'ordre ne change
/// pas d'un appel à l'autre.
fn shuffle_key(id: TargetId) -> u64 {
    let mut x = (id as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn matches(wave: &Wave, member: &Member) -> bool {
    match wave {
        Wave::Site { value } => member.site.as_deref().is_some_and(|site| same(site, value)),
        Wave::Group { value } => same(&member.group, value),
        Wave::Tag { key, value } => member
            .tags
            .iter()
            .any(|(k, v)| same(k, key) && value.as_deref().is_none_or(|wanted| same(v, wanted))),
        Wave::Count { .. } | Wave::Percent { .. } => false,
    }
}

/// Affecte chaque machine à une vague : les machines pilotes à la première,
/// puis, vague après vague, celles que la sélection retient parmi les
/// restantes ; la dernière vague (implicite) prend tout le reste. Rend, pour
/// chaque vague (la dernière comprise), les indices dans `members`.
fn assign(waves: &[Wave], members: &[Member]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..members.len()).collect();
    order.sort_by_key(|&index| (shuffle_key(members[index].id), members[index].id));

    let mut taken = vec![false; members.len()];
    let mut out: Vec<Vec<usize>> = vec![Vec::new(); waves.len() + 1];
    let mut total_taken = 0;
    for (index, member) in members.iter().enumerate() {
        if member.canary {
            taken[index] = true;
            out[0].push(index);
            total_taken += 1;
        }
    }
    for (k, wave) in waves.iter().enumerate() {
        let picked: Vec<usize> = match wave {
            Wave::Count { value } => {
                order.iter().copied().filter(|&index| !taken[index]).take(*value as usize).collect()
            }
            Wave::Percent { value } => {
                let wanted = (members.len() * *value as usize).div_ceil(100);
                let need = wanted.saturating_sub(total_taken);
                order.iter().copied().filter(|&index| !taken[index]).take(need).collect()
            }
            other => order
                .iter()
                .copied()
                .filter(|&index| !taken[index] && matches(other, &members[index]))
                .collect(),
        };
        for index in picked {
            taken[index] = true;
            total_taken += 1;
            out[k].push(index);
        }
    }
    let last = waves.len();
    for (index, was_taken) in taken.iter().enumerate() {
        if !was_taken {
            out[last].push(index);
        }
    }
    out
}

/// Calcule l'affectation, l'avancement et l'arrêt éventuel.
pub fn plan(rollout: &Rollout, members: &[Member], now_ms: i64) -> Plan {
    let assignment = assign(&rollout.waves, members);
    let mut wave_of = BTreeMap::new();
    let mut waves = Vec::with_capacity(assignment.len());
    for (k, indices) in assignment.iter().enumerate() {
        let mut wave = WavePlan {
            wave: rollout.waves.get(k).cloned(),
            members: Vec::with_capacity(indices.len()),
            done: 0,
            pending: 0,
            failed: 0,
            latest_since_ms: None,
        };
        for &index in indices {
            let member = &members[index];
            wave.members.push(member.id);
            wave_of.insert(member.id, k);
            match member.state {
                MemberState::Done { since_ms } => {
                    wave.done += 1;
                    let latest = wave.latest_since_ms.map_or(since_ms, |at| at.max(since_ms));
                    wave.latest_since_ms = Some(latest);
                }
                MemberState::Pending => wave.pending += 1,
                MemberState::Failed => wave.failed += 1,
                MemberState::Idle => {}
            }
        }
        waves.push(wave);
    }

    // Ouverture : la vague 0 l'est toujours ; chaque suivante attend que la
    // précédente n'ait plus de machine en attente, puis le délai.
    let delay_ms = i64::from(rollout.delay_minutes) * 60_000;
    let mut open_count = 0;
    let mut next_at_ms = None;
    for (k, wave) in waves.iter().enumerate() {
        open_count = k + 1;
        if k + 1 == waves.len() || wave.pending > 0 {
            break;
        }
        if let Some(since) = wave.latest_since_ms {
            let at = since + delay_ms;
            if now_ms < at {
                next_at_ms = Some(at);
                break;
            }
        }
    }

    // Arrêt : seulement s'il reste quelque chose à propager.
    let remaining = waves.iter().any(|wave| wave.pending > 0);
    let halt = if remaining && rollout.max_failure_percent > 0 {
        waves.iter().enumerate().find_map(|(k, wave)| {
            let attempted = wave.done + wave.failed;
            let over = wave.failed > 0
                && wave.failed * 100 >= rollout.max_failure_percent as usize * attempted;
            over.then_some(Halt { wave: k, failed: wave.failed, attempted })
        })
    } else {
        None
    };

    Plan { waves, open_count, next_at_ms, halt, paused: rollout.paused, wave_of }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const MIN: i64 = 60_000;

    fn member(id: TargetId, state: MemberState) -> Member {
        Member { id, canary: false, site: None, group: String::new(), tags: BTreeMap::new(), state }
    }

    fn fleet(count: i64) -> Vec<Member> {
        (1..=count).map(|id| member(id, MemberState::Pending)).collect()
    }

    fn rollout(waves: Vec<Wave>) -> Rollout {
        Rollout { waves, delay_minutes: 60, max_failure_percent: 20, ..Rollout::default() }
    }

    fn done(since_ms: i64) -> MemberState {
        MemberState::Done { since_ms }
    }

    #[test]
    fn the_default_policy_has_no_wave_so_nothing_changes() {
        let policy = Rollout::default();
        assert!(policy.waves.is_empty() && !policy.paused);
        assert!(policy.validate().is_ok());
        // Un réglage rangé par une version qui ne connaissait que la pause.
        let partial: Rollout = serde_json::from_str(r#"{"paused": true}"#).unwrap();
        assert!(partial.paused && partial.waves.is_empty());
        assert_eq!(partial.delay_minutes, DEFAULT_DELAY_MINUTES);
    }

    #[test]
    fn waves_round_trip_through_json() {
        let text = r#"{"waves":[{"by":"count","value":2},{"by":"percent","value":50},
            {"by":"site","value":"Paris"},{"by":"group","value":"prod"},
            {"by":"tag","key":"env"},{"by":"tag","key":"env","value":"prod"}]}"#;
        let policy: Rollout = serde_json::from_str(text).unwrap();
        assert_eq!(policy.waves.len(), 6);
        assert_eq!(policy.waves[4], Wave::Tag { key: "env".into(), value: None });
        let text = serde_json::to_string(&policy).unwrap();
        let again: Rollout = serde_json::from_str(&text).unwrap();
        assert_eq!(again, policy);
        assert!(policy.validate().is_ok());
    }

    #[test]
    fn validation_refuses_absurd_policies() {
        let bad = |waves: Vec<Wave>| rollout(waves).validate().is_err();
        assert!(bad(vec![Wave::Count { value: 0 }]));
        assert!(bad(vec![Wave::Percent { value: 0 }]));
        assert!(bad(vec![Wave::Percent { value: 101 }]));
        assert!(bad(vec![Wave::Site { value: " ".into() }]));
        assert!(bad(vec![Wave::Group { value: "x".repeat(MAX_NAME_LEN + 1) }]));
        assert!(bad(vec![Wave::Tag { key: String::new(), value: None }]));
        assert!(bad((0..=MAX_WAVES).map(|_| Wave::Count { value: 1 }).collect()));
        let late = Rollout { delay_minutes: MAX_DELAY_MINUTES + 1, ..Rollout::default() };
        assert!(late.validate().is_err());
        let over = Rollout { max_failure_percent: 101, ..Rollout::default() };
        assert!(over.validate().is_err());
    }

    #[test]
    fn a_count_wave_then_percentages_then_everyone_else() {
        let members = fleet(20);
        let policy = rollout(vec![
            Wave::Count { value: 2 },
            Wave::Percent { value: 25 },
            Wave::Percent { value: 50 },
        ]);
        let plan = plan(&policy, &members, NOW);
        let sizes: Vec<usize> = plan.waves.iter().map(|wave| wave.members.len()).collect();
        // 2 de plus ; 25 % de 20 = 5 au total soit 3 de plus ; 10 au total soit 5 de plus ;
        // le reste.
        assert_eq!(sizes, vec![2, 3, 5, 10]);
        assert!(plan.waves[3].wave.is_none(), "la dernière vague est implicite");
        // Chaque machine dans exactement une vague.
        let mut all: Vec<TargetId> = plan.waves.iter().flat_map(|w| w.members.clone()).collect();
        all.sort_unstable();
        assert_eq!(all, (1..=20).collect::<Vec<_>>());
        // Stable : même entrée, même affectation.
        assert_eq!(plan, super::plan(&policy, &members, NOW));
    }

    #[test]
    fn site_group_and_tag_waves_select_their_agents_and_canaries_go_first() {
        let mut members = fleet(6);
        members[0].site = Some("Paris".into());
        members[1].site = Some(" paris ".into());
        members[2].group = "Prod".into();
        members[3].tags.insert("env".into(), "prod".into());
        members[4].tags.insert("env".into(), "dev".into());
        members[5].canary = true;
        members[5].site = Some("Lyon".into());
        let policy = rollout(vec![
            Wave::Site { value: "paris".into() },
            Wave::Group { value: "prod".into() },
            Wave::Tag { key: "ENV".into(), value: Some("prod".into()) },
            Wave::Tag { key: "env".into(), value: None },
        ]);
        let plan = plan(&policy, &members, NOW);
        let ids = |k: usize| {
            let mut ids = plan.waves[k].members.clone();
            ids.sort_unstable();
            ids
        };
        assert_eq!(ids(0), vec![1, 2, 6], "le canari et le site, dans la première vague");
        assert_eq!(ids(1), vec![3]);
        assert_eq!(ids(2), vec![4]);
        assert_eq!(ids(3), vec![5]);
        assert!(plan.waves[4].members.is_empty());
    }

    #[test]
    fn waves_open_one_after_the_other_with_the_delay() {
        let policy = rollout(vec![Wave::Count { value: 1 }, Wave::Percent { value: 50 }]);
        let mut members = fleet(4);
        let first = plan(&policy, &members, NOW);
        assert_eq!(first.open_count, 1, "seule la première vague est ouverte");
        let canary = first.waves[0].members[0];
        let held: Vec<TargetId> = (1..=4).filter(|&id| first.hold_for(id).is_some()).collect();
        assert_eq!(held.len(), 3);
        assert!(first.hold_for(canary).is_none());
        assert_eq!(first.current(), Some(0));

        // Le canari est à jour depuis 10 min : la suite attend le délai.
        for m in &mut members {
            if m.id == canary {
                m.state = done(NOW - 10 * MIN);
            }
        }
        let waiting = plan(&policy, &members, NOW);
        assert_eq!(waiting.open_count, 1);
        assert_eq!(waiting.next_at_ms, Some(NOW + 50 * MIN));
        assert_eq!(waiting.hold_for(if canary == 1 { 2 } else { 1 }), Some(Hold::Wave));

        // 61 minutes plus tard : la deuxième vague s'ouvre, pas la dernière.
        let later = plan(&policy, &members, NOW + 51 * MIN);
        assert_eq!(later.open_count, 2);
        assert_eq!(later.next_at_ms, None);
        assert_eq!(later.current(), Some(1));
        assert!(!later.fully_open());

        // Elle se termine : la dernière s'ouvre après le délai.
        for m in &mut members {
            if later.waves[1].members.contains(&m.id) {
                m.state = done(NOW + 51 * MIN);
            }
        }
        let last = plan(&policy, &members, NOW + 51 * MIN + 61 * MIN);
        assert_eq!(last.open_count, 3);
        assert!(last.fully_open());
    }

    #[test]
    fn an_empty_wave_is_skipped_without_delay() {
        let policy = rollout(vec![Wave::Site { value: "nulle part".into() }]);
        let plan = plan(&policy, &fleet(3), NOW);
        assert_eq!(plan.open_count, 2, "la vague vide ne retient personne");
        assert!(plan.fully_open());
    }

    #[test]
    fn a_pending_agent_holds_the_next_wave_but_failed_and_idle_ones_do_not() {
        let policy = Rollout { max_failure_percent: 0, ..rollout(vec![Wave::Count { value: 3 }]) };
        let mut members = fleet(5);
        let first = plan(&policy, &members, NOW);
        let wave0 = first.waves[0].members.clone();
        members[(wave0[0] - 1) as usize].state = done(NOW - 120 * MIN);
        members[(wave0[1] - 1) as usize].state = MemberState::Failed;
        members[(wave0[2] - 1) as usize].state = MemberState::Idle;
        let after = plan(&policy, &members, NOW);
        assert_eq!(after.open_count, 2);
        assert!(after.halt.is_none(), "seuil à 0 : on ne s'arrête jamais");
    }

    #[test]
    fn too_many_failures_in_a_wave_stop_the_propagation() {
        let policy = rollout(vec![Wave::Count { value: 4 }]);
        let mut members = fleet(10);
        let first = plan(&policy, &members, NOW);
        let wave0 = first.waves[0].members.clone();
        // 1 échec sur 4 tentatives : 25 % >= 20 %.
        members[(wave0[0] - 1) as usize].state = MemberState::Failed;
        for id in &wave0[1..] {
            members[(*id - 1) as usize].state = done(NOW - 5 * MIN);
        }
        let stopped = plan(&policy, &members, NOW);
        let halt = stopped.halt.expect("arrêt");
        assert_eq!((halt.wave, halt.failed, halt.attempted), (0, 1, 4));
        for member in &members {
            assert_eq!(stopped.hold_for(member.id), Some(Hold::Halted));
        }
        assert!(!stopped.fully_open());

        // Un seuil plus haut : pas d'arrêt.
        let lax = Rollout { max_failure_percent: 30, ..policy.clone() };
        assert!(plan(&lax, &members, NOW).halt.is_none());
        // Une seule machine en échec sur un grand nombre : sous le seuil.
        let mut big = fleet(30);
        let wide = rollout(vec![Wave::Percent { value: 100 }]);
        let everyone = plan(&wide, &big, NOW);
        let ids = everyone.waves[0].members.clone();
        big[(ids[0] - 1) as usize].state = MemberState::Failed;
        for id in &ids[1..20] {
            big[(*id - 1) as usize].state = done(NOW - 5 * MIN);
        }
        assert!(plan(&wide, &big, NOW).halt.is_none(), "1 sur 20 : 5 %");
    }

    #[test]
    fn nothing_halts_when_there_is_nothing_left_to_propagate() {
        let policy = rollout(vec![Wave::Count { value: 1 }]);
        let mut members = fleet(2);
        members[0].state = MemberState::Failed;
        members[1].state = MemberState::Failed;
        assert!(plan(&policy, &members, NOW).halt.is_none());
    }

    #[test]
    fn pause_holds_every_automatic_offer_first() {
        let policy = Rollout { paused: true, ..rollout(vec![Wave::Count { value: 1 }]) };
        let plan = plan(&policy, &fleet(3), NOW);
        assert!(plan.paused && !plan.fully_open());
        for id in 1..=3 {
            assert_eq!(plan.hold_for(id), Some(Hold::Paused));
        }
    }

    #[test]
    fn an_unknown_agent_follows_the_last_wave() {
        let policy = rollout(vec![Wave::Count { value: 1 }]);
        let plan = plan(&policy, &fleet(3), NOW);
        assert_eq!(plan.hold_for(999), Some(Hold::Wave));
    }
}
