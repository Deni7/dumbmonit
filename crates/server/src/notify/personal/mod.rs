//! Notifications personnelles : ce que chaque compte choisit de recevoir
//! lui-même, par courriel (le serveur de courriel de l'instance, vers
//! l'adresse du compte) ou par Web Push (ses appareils abonnés), en plus des
//! canaux de l'instance.
//!
//! Les choix d'un compte ([`Prefs`]) : sévérité minimale, résolutions oui ou
//! non, périmètre (tout ce qu'il voit, les dossiers de ses équipes, ou des
//! dossiers et étiquettes choisis), délai minimal entre deux messages pour la
//! même alerte, et heures calmes dans son fuseau. Ce que ses équipes ne lui
//! montrent pas ne lui parvient jamais, quel que soit le périmètre.
//!
//! Le moteur d'alerting appelle [`dispatch::dispatch`] une fois par cycle,
//! avec les groupes qu'il vient de décider de notifier : les mêmes que ceux des
//! canaux, déjà dédupliqués, sans ce que couvre une maintenance. Les mêmes
//! garde-fous que pour un canal s'y appliquent : retenue de battement, plafond
//! horaire de la politique globale, « ne résoudre que ce qui a été annoncé ».

pub mod dispatch;

use std::collections::{BTreeMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::alerting::group::{GroupItem, NotifyReason};
use crate::alerting::model::Severity;
use crate::alerting::silence::Schedule;
use crate::auth::teams::{folder_covers, normalize_folder};

/// Dossiers ou étiquettes choisis, au plus.
pub const MAX_PICKS: usize = 50;
const MAX_PICK_CHARS: usize = 200;
/// Délai minimal le plus long : une journée.
pub const MAX_INTERVAL_SECS: u32 = 86_400;

/// Ce dont parlent les messages personnels.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Tout ce que le compte voit.
    #[default]
    All,
    /// Les équipements des dossiers ouverts par ses équipes.
    Teams,
    /// Les dossiers et étiquettes choisis.
    Pick,
}

/// Les choix d'un compte. Tout est coupé par défaut : rien ne part sans
/// qu'on l'ait demandé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    /// Recevoir par courriel, à l'adresse du compte.
    pub email: bool,
    /// Recevoir sur ses appareils abonnés au Web Push.
    pub webpush: bool,
    pub min_severity: Severity,
    /// Annoncer aussi le retour à la normale de ce qui a été annoncé.
    pub notify_resolved: bool,
    pub scope: Scope,
    /// Dossiers retenus (`scope: pick`) : un dossier couvre ses sous-dossiers.
    pub folders: Vec<String>,
    /// Étiquettes retenues (`scope: pick`) : `clé=valeur`, ou `clé` seule.
    pub tags: Vec<String>,
    /// Délai minimal entre deux messages pour la même alerte. 0 : sans délai.
    pub min_interval_secs: u32,
    /// Heures calmes, fenêtre hebdomadaire dans son fuseau : seule la sévérité
    /// `critical` passe pendant la fenêtre, le reste n'est pas envoyé.
    pub quiet_hours: Option<Schedule>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            email: false,
            webpush: false,
            min_severity: Severity::Warning,
            notify_resolved: true,
            scope: Scope::All,
            folders: Vec::new(),
            tags: Vec::new(),
            min_interval_secs: 0,
            quiet_hours: None,
        }
    }
}

impl Prefs {
    /// Au moins un moyen de recevoir est choisi.
    pub fn active(&self) -> bool {
        self.email || self.webpush
    }

    /// Contrôle une saisie et la rend nettoyée : dossiers normalisés,
    /// doublons et entrées vides retirés.
    pub fn validated(mut self) -> Result<Self, String> {
        self.folders = clean_picks(&self.folders, normalize_folder)?;
        self.tags = clean_picks(&self.tags, |tag| match tag.split_once('=') {
            Some((key, value)) => format!("{}={}", key.trim(), value.trim()),
            None => tag.trim().to_string(),
        })?;
        if self.tags.iter().any(|tag| tag.starts_with('=')) {
            return Err("A tag needs a name before \"=\".".into());
        }
        if self.scope == Scope::Pick && self.folders.is_empty() && self.tags.is_empty() {
            return Err("Pick at least one folder or tag, or choose another scope.".into());
        }
        if self.min_interval_secs > MAX_INTERVAL_SECS {
            return Err("The minimum delay between two messages is one day at most.".into());
        }
        if let Some(schedule) = &self.quiet_hours {
            check_quiet_hours(schedule)?;
        }
        Ok(self)
    }

    /// Vrai si les heures calmes couvrent `now`.
    pub fn in_quiet_hours(&self, now: DateTime<Utc>) -> bool {
        self.quiet_hours.as_ref().is_some_and(|schedule| schedule.covers(now))
    }
}

fn clean_picks(raw: &[String], normalize: impl Fn(&str) -> String) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for entry in raw {
        let entry = normalize(entry);
        if entry.is_empty() {
            continue;
        }
        if entry.chars().count() > MAX_PICK_CHARS || entry.chars().any(char::is_control) {
            return Err("A folder or tag is too long or holds control characters.".into());
        }
        if !out.contains(&entry) {
            out.push(entry);
        }
    }
    if out.len() > MAX_PICKS {
        return Err(format!("Pick {MAX_PICKS} folders and {MAX_PICKS} tags at most."));
    }
    Ok(out)
}

/// Des heures calmes lisibles : une fenêtre hebdomadaire, des jours, deux
/// heures différentes, un fuseau connu s'il y en a un.
fn check_quiet_hours(schedule: &Schedule) -> Result<(), String> {
    let (days, start, end, timezone, offset) = match schedule {
        Schedule::Weekly { days, start_minute, end_minute, timezone, utc_offset_minutes } => {
            (days, *start_minute, *end_minute, timezone, *utc_offset_minutes)
        }
        _ => return Err("Quiet hours are a weekly window.".into()),
    };
    if days.is_empty() || days.iter().any(|day| *day > 6) {
        return Err("Pick at least one day for the quiet hours.".into());
    }
    if start >= 1440 || end >= 1440 || start == end {
        return Err("Quiet hours need a start and an end time that differ.".into());
    }
    if offset.abs() > 14 * 60 {
        return Err("The time offset of the quiet hours is out of range.".into());
    }
    if let Some(name) = timezone.as_deref().filter(|name| !name.is_empty())
        && crate::oncall::schedule::parse_tz(name).is_none()
    {
        return Err(format!("Unknown time zone \"{name}\"."));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Ce qui part
// ---------------------------------------------------------------------------

/// Ce que l'on sait d'un équipement pour décider s'il est dans le périmètre.
pub struct TargetFacts<'a> {
    /// `None` : séries sans équipement identifié.
    pub id: Option<i64>,
    pub folder: Option<&'a str>,
    pub tags: &'a BTreeMap<String, String>,
}

/// Vrai si l'équipement est dans le périmètre choisi. `team_folders` : les
/// dossiers ouverts par les équipes du compte (pour `scope: teams`). La
/// visibilité accordée par les équipes est vérifiée à part.
pub fn in_scope(prefs: &Prefs, team_folders: &[String], facts: &TargetFacts<'_>) -> bool {
    if facts.id.is_none() {
        // Les séries sans équipement n'ont ni dossier ni étiquette.
        return prefs.scope == Scope::All;
    }
    let folder_in = |list: &[String]| {
        facts.folder.is_some_and(|folder| list.iter().any(|grant| folder_covers(grant, folder)))
    };
    match prefs.scope {
        Scope::All => true,
        Scope::Teams => folder_in(team_folders),
        Scope::Pick => {
            let tagged = prefs.tags.iter().any(|wanted| tag_matches(wanted, facts.tags));
            tagged || folder_in(&prefs.folders)
        }
    }
}

/// `clé=valeur` : l'étiquette a cette valeur (sans casse) ; `clé` : elle existe.
fn tag_matches(wanted: &str, tags: &BTreeMap<String, String>) -> bool {
    match wanted.split_once('=') {
        Some((key, value)) => {
            tags.get(key.trim()).is_some_and(|have| have.eq_ignore_ascii_case(value.trim()))
        }
        None => tags.contains_key(wanted.trim()),
    }
}

/// Un message déjà parti vers le compte, pour une alerte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub fingerprint: String,
    pub reason: NotifyReason,
    pub at: DateTime<Utc>,
}

/// Les lignes d'un groupe qui partent vers ce compte, dans l'ordre.
///
/// `held` : empreintes retenues pour battement. `log` : ce qui est déjà parti
/// vers lui, le plus ancien d'abord.
pub fn select_items(
    prefs: &Prefs,
    items: &[GroupItem],
    log: &[Sent],
    held: &HashSet<String>,
    now: DateTime<Utc>,
) -> Vec<GroupItem> {
    let quiet = prefs.in_quiet_hours(now);
    items
        .iter()
        .filter(|item| {
            // Le relais d'escalade ne part que sur le canal d'escalade.
            if item.reason == NotifyReason::Unacked || held.contains(&item.fingerprint) {
                return false;
            }
            if quiet && item.severity < Severity::Critical {
                return false;
            }
            let mine = || log.iter().filter(|sent| sent.fingerprint == item.fingerprint);
            if item.reason == NotifyReason::Resolved {
                // Seulement ce qui lui a été annoncé, et pas deux fois.
                return prefs.notify_resolved
                    && mine().last().is_some_and(|last| last.reason.is_firing_like());
            }
            if item.severity < prefs.min_severity {
                return false;
            }
            if prefs.min_interval_secs > 0 {
                let since = now - TimeDelta::seconds(i64::from(prefs.min_interval_secs));
                if mine().any(|sent| sent.reason.is_firing_like() && sent.at > since) {
                    return false;
                }
            }
            true
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// Base
// ---------------------------------------------------------------------------

/// Les choix d'un compte ; rien d'enregistré ou illisible : tout coupé.
pub async fn load(pool: &SqlitePool, user_id: i64) -> Result<Prefs> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT prefs FROM user_notification_prefs WHERE user_id = ?")
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("lecture des notifications personnelles")?;
    Ok(raw.as_deref().map(parse).unwrap_or_default())
}

fn parse(raw: &str) -> Prefs {
    serde_json::from_str(raw).unwrap_or_else(|error| {
        tracing::warn!(%error, "personal notification preferences unreadable, turned off");
        Prefs::default()
    })
}

pub async fn save(pool: &SqlitePool, user_id: i64, prefs: &Prefs) -> Result<()> {
    sqlx::query(
        "INSERT INTO user_notification_prefs (user_id, prefs) VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET prefs = excluded.prefs,
             updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')",
    )
    .bind(user_id)
    .bind(serde_json::to_string(prefs)?)
    .execute(pool)
    .await
    .context("enregistrement des notifications personnelles")?;
    Ok(())
}

/// Les comptes qui ont choisi au moins un moyen de recevoir.
pub async fn all_active(pool: &SqlitePool) -> Result<Vec<(i64, Prefs)>> {
    Ok(all(pool).await?.into_iter().filter(|(_, prefs)| prefs.active()).collect())
}

/// Toutes les préférences enregistrées, par compte (sauvegarde).
pub async fn all(pool: &SqlitePool) -> Result<Vec<(i64, Prefs)>> {
    let rows = sqlx::query("SELECT user_id, prefs FROM user_notification_prefs ORDER BY user_id")
        .fetch_all(pool)
        .await
        .context("lecture des notifications personnelles")?;
    rows.iter()
        .map(|row| {
            let raw: String = row.try_get("prefs")?;
            Ok((row.try_get("user_id")?, parse(&raw)))
        })
        .collect()
}

#[cfg(test)]
mod tests;
