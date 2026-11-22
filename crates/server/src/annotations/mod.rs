//! Repères de changement (« annotations ») : ce qui s'est passé à la main ou
//! par un pipeline (déploiement, mise à jour, maintenance) et qui explique une
//! cassure sur un graphique.
//!
//! Un repère est un instant, ou une fenêtre (`until`), avec un titre, un texte
//! et un lien facultatifs, et un périmètre : toute l'instance, une liste de
//! moniteurs, un dossier (sous-dossiers compris) ou des étiquettes de moniteurs.
//! L'API est dans [`crate::api`] (`annotations.rs`), le stockage dans [`store`].
//! Les graphiques les dessinent ; une alerte déclenchée peu après un repère du
//! même moniteur le cite (voir [`after_change`]).

pub mod store;

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::teams::folder_covers;
use crate::db;

/// Repères conservés au plus : au-delà, la création est refusée (409).
pub const MAX_ANNOTATIONS: i64 = 20_000;
/// Les repères plus vieux que cela sont purgés (la rétention des métriques est
/// plus courte ou du même ordre : un repère sans courbe ne sert plus).
pub const RETENTION_DAYS: i64 = 366;
/// Une alerte déclenchée dans ce délai après la fin d'un repère du même
/// moniteur le cite.
pub const AFTER_CHANGE_MINUTES: i64 = 30;
/// Durée maximale d'une fenêtre.
const MAX_WINDOW_DAYS: i64 = 366;
const MAX_TITLE: usize = 200;
const MAX_TEXT: usize = 4000;
const MAX_LINK: usize = 500;
const MAX_TAGS: usize = 10;
const MAX_TAG: usize = 40;
const MAX_SCOPE_ITEMS: usize = 200;

/// Où un repère s'applique.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    /// Toute l'instance.
    #[default]
    Global,
    /// Ces moniteurs.
    Monitors { targets: Vec<i64> },
    /// Les moniteurs de ce dossier et de ses sous-dossiers.
    Folder { folder: String },
    /// Les moniteurs qui portent l'une de ces étiquettes (`clé` ou `clé=valeur`).
    Tags { tags: Vec<String> },
}

/// Ce qu'il faut savoir d'un moniteur pour décider qu'un repère le concerne.
#[derive(Debug, Clone, Default)]
pub struct TargetInfo {
    pub id: i64,
    pub folder: String,
    pub tags: BTreeMap<String, String>,
}

impl Scope {
    /// Vrai si le repère concerne ce moniteur.
    pub fn matches(&self, target: &TargetInfo) -> bool {
        match self {
            Self::Global => true,
            Self::Monitors { targets } => targets.contains(&target.id),
            Self::Folder { folder } => folder_covers(folder, &target.folder),
            Self::Tags { tags } => tags.iter().any(|wanted| match wanted.split_once('=') {
                Some((key, value)) => target.tags.get(key).is_some_and(|have| have == value),
                None => target.tags.contains_key(wanted),
            }),
        }
    }

    /// Les moniteurs concernés parmi `all`. Vide pour un périmètre qui ne
    /// désigne plus rien (dossier vidé, moniteurs supprimés).
    pub fn covered(&self, all: &[TargetInfo]) -> HashSet<i64> {
        all.iter().filter(|target| self.matches(target)).map(|target| target.id).collect()
    }
}

/// Un repère tel que stocké et rendu par l'API.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Annotation {
    pub id: i64,
    /// UTC, sans suffixe (`2026-11-22 10:00:00`).
    pub at: String,
    pub until: Option<String>,
    pub title: String,
    pub text: String,
    pub link: String,
    pub tags: Vec<String>,
    pub scope: Scope,
    /// `api`, `ui` ou `ci`.
    pub source: String,
    /// Compte, ou `token:nom` pour un jeton d'API.
    pub author: String,
    pub created_at: String,
}

/// Un repère validé, prêt à être écrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub at: DateTime<Utc>,
    pub until: Option<DateTime<Utc>>,
    pub title: String,
    pub text: String,
    pub link: String,
    pub tags: Vec<String>,
    pub scope: Scope,
    pub source: String,
}

/// Les sources reconnues.
pub const SOURCES: [&str; 3] = ["api", "ui", "ci"];

/// Lit une date : RFC 3339, ou `AAAA-MM-JJ[ T]HH:MM[:SS]` lu en UTC.
pub fn parse_time(value: &str) -> Result<DateTime<Utc>, String> {
    let value = value.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(naive.and_utc());
        }
    }
    Err(format!("\"{value}\" is not a date: use RFC 3339, like 2026-11-22T10:00:00Z."))
}

/// Champs d'un repère tels qu'ils arrivent, avant validation.
#[derive(Debug, Clone, Default)]
pub struct RawDraft {
    pub at: Option<String>,
    pub until: Option<String>,
    pub title: String,
    pub text: String,
    pub link: String,
    pub tags: Vec<String>,
    pub scope: Scope,
    pub source: Option<String>,
}

impl RawDraft {
    /// Valide et normalise. `now` sert d'instant par défaut ; `default_source`
    /// quand l'appelant n'en nomme pas.
    pub fn validate(self, now: DateTime<Utc>, default_source: &str) -> Result<Draft, String> {
        let title = self.title.trim().to_string();
        if title.is_empty() {
            return Err("A marker needs a title.".into());
        }
        if title.chars().count() > MAX_TITLE {
            return Err(format!("The title is limited to {MAX_TITLE} characters."));
        }
        let text = self.text.trim().to_string();
        if text.chars().count() > MAX_TEXT {
            return Err(format!("The text is limited to {MAX_TEXT} characters."));
        }
        let link = self.link.trim().to_string();
        if !link.is_empty() {
            let web = link.starts_with("https://") || link.starts_with("http://");
            if !web || link.chars().count() > MAX_LINK || link.chars().any(char::is_whitespace) {
                return Err(format!(
                    "The link must be an http(s) address of at most {MAX_LINK} characters."
                ));
            }
        }
        let at = match self.at.as_deref().filter(|at| !at.trim().is_empty()) {
            Some(at) => parse_time(at)?,
            None => now,
        };
        let until = match self.until.as_deref().filter(|until| !until.trim().is_empty()) {
            Some(until) => Some(parse_time(until)?),
            None => None,
        };
        if let Some(until) = until
            && (until < at || until - at > TimeDelta::days(MAX_WINDOW_DAYS))
        {
            return Err(format!(
                "The end of the window must not be before its start, nor {MAX_WINDOW_DAYS} days \
                 after it."
            ));
        }
        let tags = clean_list(&self.tags, MAX_TAG, MAX_TAGS, "tags")?;
        let scope = match self.scope {
            Scope::Global => Scope::Global,
            Scope::Monitors { mut targets } => {
                targets.sort_unstable();
                targets.dedup();
                if targets.is_empty() || targets.len() > MAX_SCOPE_ITEMS {
                    return Err(format!("Name between 1 and {MAX_SCOPE_ITEMS} monitors."));
                }
                Scope::Monitors { targets }
            }
            Scope::Folder { folder } => {
                let folder = crate::auth::teams::normalize_folder(&folder);
                if folder.is_empty() {
                    return Err("Name the folder the marker applies to.".into());
                }
                Scope::Folder { folder }
            }
            Scope::Tags { tags } => {
                let tags = clean_list(&tags, MAX_TAG * 2, MAX_SCOPE_ITEMS, "monitor tags")?;
                if tags.is_empty() {
                    return Err("Name at least one monitor tag.".into());
                }
                Scope::Tags { tags }
            }
        };
        let source = match self.source.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(source) if SOURCES.contains(&source) => source.to_string(),
            Some(_) => return Err("The source must be \"api\", \"ui\" or \"ci\".".into()),
            None => default_source.to_string(),
        };
        Ok(Draft { at, until, title, text, link, tags, scope, source })
    }
}

/// Retire les blancs et les doublons d'une liste de mots.
fn clean_list(
    items: &[String],
    max_len: usize,
    max_items: usize,
    what: &str,
) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let item = item.trim();
        if item.is_empty() || out.iter().any(|seen| seen == item) {
            continue;
        }
        if item.chars().count() > max_len {
            return Err(format!("A tag in {what} is limited to {max_len} characters."));
        }
        out.push(item.to_string());
    }
    if out.len() > max_items {
        return Err(format!("At most {max_items} {what} are allowed."));
    }
    Ok(out)
}

/// Le repère qui précède une alerte, tel que l'alerte le cite.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AfterChange {
    pub id: i64,
    pub title: String,
    pub at: String,
}

/// Parmi `changes`, le repère le plus récent qui concerne `target` et qui
/// précède `firing_since` de moins de [`AFTER_CHANGE_MINUTES`] (comptées depuis
/// la fin de sa fenêtre, s'il en a une). Un repère postérieur au
/// déclenchement n'explique rien : il est ignoré.
pub fn after_change(
    changes: &[Annotation],
    target: &TargetInfo,
    firing_since: DateTime<Utc>,
) -> Option<AfterChange> {
    let fired = db::timestamp(firing_since);
    let floor = db::timestamp(firing_since - TimeDelta::minutes(AFTER_CHANGE_MINUTES));
    changes
        .iter()
        .filter(|change| change.at <= fired)
        .filter(|change| change.until.as_deref().unwrap_or(&change.at) >= floor.as_str())
        .filter(|change| change.scope.matches(target))
        .max_by(|a, b| a.at.cmp(&b.at).then(a.id.cmp(&b.id)))
        .map(|change| AfterChange {
            id: change.id,
            title: change.title.clone(),
            at: change.at.clone(),
        })
}

/// Les repères qui ont pu précéder les alertes d'une liste, lus une fois pour
/// toutes (voir [`changes_before_alerts`]).
#[derive(Default)]
pub struct AlertChanges {
    changes: Vec<Annotation>,
    targets: Vec<TargetInfo>,
}

impl AlertChanges {
    /// Le repère que cite cette alerte, s'il y en a un.
    pub fn cited_by(&self, alert: &crate::alerting::cycle::StoredAlert) -> Option<AfterChange> {
        let target = self.targets.iter().find(|t| Some(t.id) == alert.target_id)?;
        after_change(&self.changes, target, alert.state.firing_since?)
    }
}

/// Lit, d'une seule requête, les repères des 30 minutes qui précèdent chaque
/// déclenchement. Un échec de lecture n'enlève rien à la liste d'alertes : on
/// n'affiche simplement pas de repère.
pub async fn changes_before_alerts(
    pool: &sqlx::SqlitePool,
    alerts: &[crate::alerting::cycle::StoredAlert],
) -> AlertChanges {
    let fired: Vec<DateTime<Utc>> = alerts
        .iter()
        .filter(|alert| alert.target_id.is_some())
        .filter_map(|alert| alert.state.firing_since)
        .collect();
    let (Some(first), Some(last)) = (fired.iter().min(), fired.iter().max()) else {
        return AlertChanges::default();
    };
    let since = *first - TimeDelta::minutes(AFTER_CHANGE_MINUTES);
    let loaded = async {
        let changes = store::list(pool, Some(since), Some(*last), 5000).await?;
        if changes.is_empty() {
            return anyhow::Ok(AlertChanges::default());
        }
        Ok(AlertChanges { changes, targets: store::target_infos(pool).await? })
    };
    loaded.await.unwrap_or_else(|error| {
        tracing::warn!(%error, "markers before alerts not read");
        AlertChanges::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(title: &str) -> RawDraft {
        RawDraft { title: title.into(), ..RawDraft::default() }
    }

    fn now() -> DateTime<Utc> {
        parse_time("2026-11-22T10:00:00Z").unwrap()
    }

    fn info(id: i64, folder: &str, tags: &[(&str, &str)]) -> TargetInfo {
        TargetInfo {
            id,
            folder: folder.into(),
            tags: tags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        }
    }

    fn change(id: i64, at: &str, until: Option<&str>, scope: Scope) -> Annotation {
        Annotation {
            id,
            at: at.into(),
            until: until.map(String::from),
            title: format!("change {id}"),
            text: String::new(),
            link: String::new(),
            tags: Vec::new(),
            scope,
            source: "api".into(),
            author: String::new(),
            created_at: at.into(),
        }
    }

    #[test]
    fn dates_are_read_in_utc_with_or_without_a_suffix() {
        let expected = now();
        assert_eq!(parse_time("2026-11-22T10:00:00Z").unwrap(), expected);
        assert_eq!(parse_time("2026-11-22 10:00:00").unwrap(), expected);
        assert_eq!(parse_time("2026-11-22T12:00:00+02:00").unwrap(), expected);
        assert!(parse_time("yesterday").is_err());
    }

    #[test]
    fn a_marker_defaults_to_now_and_needs_a_title() {
        let draft = raw("  Deploy v2  ").validate(now(), "api").unwrap();
        assert_eq!(draft.title, "Deploy v2");
        assert_eq!(draft.at, now());
        assert_eq!(draft.scope, Scope::Global);
        assert_eq!(draft.source, "api");
        assert!(raw("   ").validate(now(), "api").is_err());
    }

    #[test]
    fn a_window_cannot_end_before_it_starts() {
        let mut bad = raw("Deploy");
        bad.at = Some("2026-11-22T10:00:00Z".into());
        bad.until = Some("2026-11-22T09:00:00Z".into());
        assert!(bad.validate(now(), "api").is_err());
        let mut good = raw("Deploy");
        good.at = Some("2026-11-22T10:00:00Z".into());
        good.until = Some("2026-11-22T10:10:00Z".into());
        assert!(good.validate(now(), "ci").unwrap().until.is_some());
    }

    #[test]
    fn links_must_be_web_addresses() {
        for bad in ["javascript:alert(1)", "ftp://x", "https://a b"] {
            let mut item = raw("Deploy");
            item.link = bad.into();
            assert!(item.validate(now(), "api").is_err(), "{bad}");
        }
        let mut item = raw("Deploy");
        item.link = "https://github.com/acme/app/actions/runs/1".into();
        assert!(item.validate(now(), "api").is_ok());
    }

    #[test]
    fn scopes_are_normalised_and_never_empty() {
        let mut item = raw("Deploy");
        item.scope = Scope::Monitors { targets: vec![3, 1, 3] };
        assert_eq!(
            item.validate(now(), "api").unwrap().scope,
            Scope::Monitors { targets: vec![1, 3] }
        );
        let mut item = raw("Deploy");
        item.scope = Scope::Monitors { targets: vec![] };
        assert!(item.validate(now(), "api").is_err());
        let mut item = raw("Deploy");
        item.scope = Scope::Folder { folder: " Paris//Rack ".into() };
        assert_eq!(
            item.validate(now(), "api").unwrap().scope,
            Scope::Folder { folder: "Paris/Rack".into() }
        );
        let mut item = raw("Deploy");
        item.source = Some("robot".into());
        assert!(item.validate(now(), "api").is_err());
    }

    #[test]
    fn a_scope_matches_monitors_by_id_folder_or_tag() {
        let web = info(1, "Paris/Rack", &[("env", "prod")]);
        let db = info(2, "Lyon", &[("team", "data")]);
        assert!(Scope::Global.matches(&web));
        assert!(Scope::Monitors { targets: vec![2] }.matches(&db));
        assert!(!Scope::Monitors { targets: vec![2] }.matches(&web));
        let paris = Scope::Folder { folder: "Paris".into() };
        assert!(paris.matches(&web) && !paris.matches(&db));
        let prod = Scope::Tags { tags: vec!["env=prod".into()] };
        assert!(prod.matches(&web) && !prod.matches(&db));
        let team = Scope::Tags { tags: vec!["team".into()] };
        assert!(team.matches(&db) && !team.matches(&web));
        assert_eq!(Scope::Global.covered(&[web, db]).len(), 2);
    }

    #[test]
    fn an_alert_cites_the_latest_marker_of_the_half_hour_before() {
        let web = info(1, "", &[]);
        let other = info(2, "", &[]);
        let fired = parse_time("2026-11-22T10:00:00Z").unwrap();
        let changes = vec![
            change(1, "2026-11-22 09:10:00", None, Scope::Global),
            change(2, "2026-11-22 09:40:00", None, Scope::Monitors { targets: vec![1] }),
            change(3, "2026-11-22 09:50:00", None, Scope::Monitors { targets: vec![2] }),
            change(4, "2026-11-22 10:05:00", None, Scope::Global),
        ];
        let cited = after_change(&changes, &web, fired).expect("a marker");
        assert_eq!(cited.id, 2, "latest marker that applies, none from the future");
        assert_eq!(after_change(&changes, &other, fired).unwrap().id, 3);
        // Hors délai : rien.
        let late = parse_time("2026-11-22T12:00:00Z").unwrap();
        assert!(after_change(&changes[..1], &web, late).is_none());
    }

    #[test]
    fn the_thirty_minutes_run_from_the_end_of_a_window() {
        let web = info(1, "", &[]);
        let changes =
            vec![change(1, "2026-11-22 08:00:00", Some("2026-11-22 09:50:00"), Scope::Global)];
        let fired = parse_time("2026-11-22T10:10:00Z").unwrap();
        assert_eq!(after_change(&changes, &web, fired).unwrap().id, 1);
        let later = parse_time("2026-11-22T10:30:00Z").unwrap();
        assert!(after_change(&changes, &web, later).is_none());
    }
}
