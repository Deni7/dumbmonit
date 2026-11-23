//! Pages de statut publiques : administration et lecture ouverte.
//!
//! Deux faces. Côté administration (session, rôle administrateur) : les pages,
//! leurs services, les incidents et fenêtres de maintenance. Côté public (aucune
//! authentification) : un unique document JSON par page, un badge SVG et un flux
//! RSS des incidents — calculés depuis VictoriaMetrics et mis en cache trente
//! secondes, pour qu'un lien partagé ne devienne pas une charge sur la base de
//! séries.
//!
//! Le document public ne contient que ce que la page a choisi de montrer : le
//! libellé de chaque service, son état et sa disponibilité. Ni identifiant, ni
//! adresse, ni type de collecteur ne sortent par ici.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use dumbmonit_proto::{Target, TargetId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::{ApiError, ApiResult};
use crate::db;

pub(crate) mod access;
mod badges;
pub(crate) mod branding;
pub mod delivery;
mod lang;
mod older;
mod postmortem;
mod subscribers;
mod timeline;

mod feeds;
mod incidents;
mod items;
mod public;
mod public_build;

use crate::auth::teams::TeamAccess;
use crate::db::status_pages::{
    DEFAULT_SCENE, GroupLabel, Incident, IncidentInput, IncidentUpdate, LEGACY_SCENES, PageItem,
    PageItemInput, Postmortem, SCENE_ROTATIONS, SCENES, StatusPage, StatusPageInput,
    canonical_scene, split_scenes,
};
use crate::db::targets::TargetStatus;
use crate::state::AppState;
use crate::status_seo::Seo;
use crate::tsdb::{InstantSeries, Victoria, log_query_failure};
pub use feeds::*;
pub use incidents::*;
pub use items::*;
pub use public::*;
use public_build::*;

// --------------------------------------------------------------------------
// Constantes
// --------------------------------------------------------------------------

const MAX_TITLE_LEN: usize = 120;
const MAX_DESCRIPTION_LEN: usize = 1_000;
const MAX_LABEL_LEN: usize = 80;
const MAX_GROUP_LEN: usize = 60;
const MAX_INCIDENT_TITLE_LEN: usize = 160;
const MAX_BODY_LEN: usize = 4_000;
const MAX_ITEMS: usize = 200;
const MAX_GROUPS: usize = 50;
const MAX_THREAD: usize = 50;
const MAX_GROUP_FOLDER_LEN: usize = 200;
const MIN_HISTORY_DAYS: i64 = 7;
/// Période la plus courte qu'une page peut choisir : 24 h.
const MIN_SETTING_DAYS: i64 = 1;
const MAX_HISTORY_DAYS: i64 = 90;

const MAX_FOOTER_LEN: usize = 280;
const MAX_URL_LEN: usize = 300;

const THEMES: [&str; 3] = ["auto", "light", "dark"];
/// Teintes d'accent proposées. Le jeu est fermé : chacune a ses deux valeurs
/// (jour, nuit) vérifiées pour le contraste dans `web/src/routes/s/`.
pub(crate) const ACCENTS: [&str; 6] = ["default", "blue", "teal", "violet", "rose", "amber"];
const INCIDENT_STATUSES: [&str; 4] = ["investigating", "identified", "monitoring", "resolved"];
const MAINTENANCE_STATUSES: [&str; 3] = ["scheduled", "in_progress", "completed"];
const SEVERITIES: [&str; 2] = ["minor", "major"];

/// Types de cibles qui émettent `probe_success` : leur état vient de là, et non
/// de la simple présence de mesures. Les heartbeats (`push`) en font partie.
pub(crate) const PROBE_KINDS: [&str; 7] = ["http", "tcp", "dns", "ping", "tls", "push", "provider"];

/// Fenêtre au-delà de laquelle une sonde sans mesure est d'état inconnu — la même
/// que celle de l'interface.
const STATE_WINDOW: &str = "15m";

/// Durée de vie du document public en cache.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Profondeur de la liste des incidents passés, en jours.
const PAST_INCIDENTS_DAYS: i64 = 30;

/// Format des horodatages enregistrés, le même que les valeurs par défaut SQL.
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

// --------------------------------------------------------------------------
// Routes
// --------------------------------------------------------------------------

/// Routes d'administration, à monter sous le garde de session.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/status-pages", get(list_pages).post(create_page))
        .route("/status-pages/{id}", get(get_page).put(update_page).delete(delete_page))
        .route("/status-pages/{id}/items", put(set_items))
        .route("/status-pages/{id}/preview", get(preview_page))
        .route("/status-pages/{id}/languages", get(lang::get_languages).put(lang::put_languages))
        .route(
            "/status-pages/{id}/logo",
            get(branding::admin_logo).put(branding::upload_logo).delete(branding::delete_logo),
        )
        .route("/status-pages/{id}/access", put(access::set_access))
        .route("/status-pages/{id}/subscribers", get(subscribers::list))
        .route("/status-pages/{id}/subscribers/{subscriber}", delete(subscribers::remove))
        .route("/status-pages/{id}/subscribers.csv", get(subscribers::export))
        .route(
            "/status-pages/{id}/subscription",
            get(subscribers::get_settings).put(subscribers::put_settings),
        )
        .route("/incidents", get(list_incidents).post(create_incident))
        .route("/incidents/{id}", put(update_incident).delete(delete_incident))
        .route("/incidents/{id}/updates", get(list_updates).post(add_update))
        .route(
            "/incidents/{id}/postmortem",
            get(postmortem::get_postmortem)
                .put(postmortem::put_postmortem)
                .delete(postmortem::delete_postmortem),
        )
}

/// Routes publiques, sans authentification.
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/public/status/{slug}", get(public_status))
        .route("/public/status/{slug}/banner.js", get(public_banner_js))
        .route("/public/status/{slug}/badge.svg", get(public_badge))
        .route("/public/status/{slug}/rss", get(public_rss))
        .route("/public/status/{slug}/incidents", get(older::older))
        .route("/public/status/{slug}/uptime.svg", get(badges::page_uptime))
        .route("/public/status/{slug}/response.svg", get(badges::page_response))
        .route("/public/status/{slug}/components/{key}/badge.svg", get(badges::item_status))
        .route("/public/status/{slug}/components/{key}/uptime.svg", get(badges::item_uptime))
        .route("/public/status/{slug}/components/{key}/response.svg", get(badges::item_response))
        .route("/public/status/{slug}/day/{date}", get(timeline::public_day))
        .route("/public/status/{slug}/logo", get(branding::public_logo))
        .route(
            "/public/status/{slug}/access",
            post(access::unlock)
                .layer(axum::middleware::from_fn(crate::auth::cookie::always_secure)),
        )
        .route("/public/status/{slug}/subscribe", post(subscribers::subscribe))
        .route("/public/status/{slug}/confirm", post(subscribers::confirm))
        .route("/public/status/{slug}/confirm-code", post(subscribers::confirm_code))
        .route("/public/status/{slug}/unsubscribe", post(subscribers::unsubscribe))
}

// --------------------------------------------------------------------------
// Vues d'administration
// --------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct StatusPageView {
    #[serde(flatten)]
    pub page: StatusPage,
    pub items: Vec<PageItem>,
    /// Groupes cochés (dynamiques), dans l'ordre.
    pub groups: Vec<String>,
    /// Noms publics des surveillances arrivées par un groupe.
    pub group_labels: Vec<GroupLabel>,
}

#[derive(Debug, Serialize)]
pub struct IncidentView {
    #[serde(flatten)]
    pub incident: Incident,
    pub updates: Vec<IncidentUpdate>,
    /// Équipements touchés (ceux que suivent les abonnés) ; vide : toute la page.
    pub components: Vec<i64>,
    /// Post-mortem, brouillon ou publié ; `null` tant qu'il n'est pas écrit.
    pub postmortem: Option<Postmortem>,
}

#[derive(Debug, Deserialize)]
pub struct StatusPagePayload {
    pub title: String,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub published: Option<bool>,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(default)]
    pub show_uptime_days: Option<i64>,
    /// Absent : la valeur enregistrée est gardée (défaut à la création).
    #[serde(default)]
    pub accent: Option<String>,
    /// Absent : la liste enregistrée est gardée (la scène par défaut à la
    /// création) ; `[]` : aucune scène.
    #[serde(default)]
    pub scenes: Option<Vec<String>>,
    /// Absent : la valeur enregistrée est gardée (`visit` à la création).
    #[serde(default)]
    pub scene_rotation: Option<String>,
    /// Absent : la valeur enregistrée est gardée (`false` à la création).
    #[serde(default)]
    pub simple: Option<bool>,
    /// Absent : la valeur enregistrée est gardée (`false` à la création).
    #[serde(default)]
    pub show_calendar: Option<bool>,
    /// Absent : la valeur enregistrée est gardée (`false` à la création).
    #[serde(default)]
    pub show_latency: Option<bool>,
    #[serde(default)]
    pub footer_text: Option<String>,
    #[serde(default)]
    pub homepage_url: Option<String>,
    /// Absent : inchangé ; `null` : pas de canal, les courriels partent par le
    /// serveur de courriel de l'instance ; un nombre : le canal SMTP qui envoie
    /// les courriels aux abonnés, prioritaire.
    #[serde(default, deserialize_with = "present")]
    pub subscribe_channel_id: Option<Value>,
    /// Abonnement par courriel ouvert aux visiteurs. Absent : la valeur
    /// enregistrée est gardée — sauf si `subscribe_channel_id` est donné, qui
    /// l'ouvre (un nombre) ou la ferme (`null`), comme avant la migration 0246.
    #[serde(default)]
    pub subscribe_email: Option<bool>,
    /// Absent : inchangé ; `null` ou `""` : pas de domaine ; sinon le nom
    /// d'hôte public de la page (`crate::status_host`).
    #[serde(default, deserialize_with = "present")]
    pub domain: Option<Value>,
    /// Réglages de référencement ; absent : inchangés (tout par défaut à la
    /// création, donc `noindex`).
    #[serde(default)]
    pub seo: Option<Seo>,
    /// Groupes (dossiers de surveillances) cochés : dynamiques, leurs membres
    /// actuels s'affichent sous le nom du groupe. Absent : inchangés.
    #[serde(default)]
    pub groups: Option<Vec<String>>,
    /// Noms publics des surveillances arrivées par un groupe ; absent : inchangés.
    #[serde(default)]
    pub group_labels: Option<Vec<GroupLabel>>,
    /// Marque blanche : rien de DumbMonit sur la page publique. Absent : la
    /// valeur enregistrée est gardée (`false` à la création).
    #[serde(default)]
    pub hide_branding: Option<bool>,
}

/// Distingue un champ absent (`None`, par `default`) d'un `null` explicite
/// (`Some(Value::Null)`) : serde confond sinon les deux.
fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

impl StatusPagePayload {
    /// Valide la saisie. `existing` fournit les réglages d'habillage qu'un
    /// client ne renvoie pas ; `origin` est l'origine vue par l'administrateur.
    async fn validate(
        self,
        state: &AppState,
        existing: Option<&StatusPage>,
        origin: String,
    ) -> ApiResult<StatusPageInput> {
        let title = self.title.trim().to_string();
        if title.is_empty() {
            return Err(ApiError::BadRequest("Title is required.".into()));
        }
        if title.chars().count() > MAX_TITLE_LEN {
            return Err(ApiError::BadRequest(format!(
                "Title is limited to {MAX_TITLE_LEN} characters."
            )));
        }
        // Sans slug explicite, il est dérivé du titre : c'est ce que fait aussi
        // l'interface, mais un client d'API peut s'en dispenser.
        let slug = match self.slug.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => raw.to_string(),
            None => slugify(&title),
        };
        if !is_valid_slug(&slug) {
            return Err(ApiError::BadRequest(
                "Slug must be 2 to 40 characters: lowercase letters, digits and hyphens.".into(),
            ));
        }
        let description = self.description.unwrap_or_default().trim().to_string();
        if description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(ApiError::BadRequest(format!(
                "Description is limited to {MAX_DESCRIPTION_LEN} characters."
            )));
        }
        let theme = self.theme.unwrap_or_else(|| "auto".into());
        if !THEMES.contains(&theme.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Unknown theme \"{theme}\" (expected: auto, light, dark)."
            )));
        }
        let show_uptime_days = self.show_uptime_days.unwrap_or(MAX_HISTORY_DAYS);
        if !(MIN_SETTING_DAYS..=MAX_HISTORY_DAYS).contains(&show_uptime_days) {
            return Err(ApiError::BadRequest(format!(
                "The history must cover {MIN_SETTING_DAYS} to {MAX_HISTORY_DAYS} days."
            )));
        }
        let accent = match self.accent {
            Some(accent) => accent.trim().to_string(),
            None => existing.map_or_else(|| "default".to_string(), |page| page.accent.clone()),
        };
        if !ACCENTS.contains(&accent.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Unknown accent \"{accent}\" (expected: {}).",
                ACCENTS.join(", ")
            )));
        }
        let scenes = match self.scenes {
            Some(list) => list.into_iter().map(|s| s.trim().to_string()).collect(),
            None => existing
                .map_or_else(|| vec![DEFAULT_SCENE.to_string()], |page| split_scenes(&page.scenes)),
        };
        let scenes = validate_scenes(scenes)?;
        let scene_rotation = match self.scene_rotation {
            Some(rotation) => rotation.trim().to_string(),
            None => {
                existing.map_or_else(|| "visit".to_string(), |page| page.scene_rotation.clone())
            }
        };
        if !SCENE_ROTATIONS.contains(&scene_rotation.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Unknown scene rotation \"{scene_rotation}\" (expected: {}).",
                SCENE_ROTATIONS.join(", ")
            )));
        }
        let simple = self.simple.unwrap_or_else(|| existing.is_some_and(|page| page.simple));
        let show_calendar =
            self.show_calendar.unwrap_or_else(|| existing.is_some_and(|page| page.show_calendar));
        let show_latency =
            self.show_latency.unwrap_or_else(|| existing.is_some_and(|page| page.show_latency));
        let hide_branding =
            self.hide_branding.unwrap_or_else(|| existing.is_some_and(|page| page.hide_branding));
        let footer_text = match self.footer_text {
            Some(text) => text.trim().to_string(),
            None => existing.map(|page| page.footer_text.clone()).unwrap_or_default(),
        };
        if footer_text.chars().count() > MAX_FOOTER_LEN {
            return Err(ApiError::BadRequest(format!(
                "The footer text is limited to {MAX_FOOTER_LEN} characters."
            )));
        }
        if footer_text.chars().any(|c| c.is_control() && c != '\n') {
            return Err(ApiError::BadRequest("The footer text must be plain text.".into()));
        }
        let homepage_url = match self.homepage_url {
            Some(url) => validate_homepage(url.trim())?,
            None => existing.map(|page| page.homepage_url.clone()).unwrap_or_default(),
        };
        // Un client d'avant l'interrupteur ouvrait le courriel en choisissant
        // un canal et le fermait avec `null`.
        let subscribe_email = match (self.subscribe_email, &self.subscribe_channel_id) {
            (Some(open), _) => open,
            (None, Some(Value::Null)) => false,
            (None, Some(_)) => true,
            (None, None) => existing.is_some_and(|page| page.subscribe_email),
        };
        let subscribe_channel_id = match self.subscribe_channel_id {
            None => existing.and_then(|page| page.subscribe_channel_id),
            Some(Value::Null) => None,
            Some(value) => {
                let id = value.as_i64().ok_or_else(|| {
                    ApiError::BadRequest(
                        "subscribe_channel_id must be a channel id or null.".into(),
                    )
                })?;
                let channel = db::alerts::get_channel(&state.pool, &state.cipher, id)
                    .await?
                    .ok_or_else(|| {
                        ApiError::BadRequest(format!("Notification channel {id} does not exist."))
                    })?;
                if channel.kind != "smtp" {
                    return Err(ApiError::BadRequest(
                        "Subscribers are mailed through an email (SMTP) channel.".into(),
                    ));
                }
                Some(id)
            }
        };
        let domain = match self.domain {
            None => existing.and_then(|page| page.domain.clone()),
            Some(Value::Null) => None,
            Some(Value::String(raw)) => {
                crate::status_host::normalise_domain(&raw).map_err(ApiError::BadRequest)?
            }
            Some(_) => {
                return Err(ApiError::BadRequest("domain must be a host name or null.".into()));
            }
        };
        if let Some(domain) = &domain {
            check_not_admin_host(state, domain, &origin).await?;
        }
        let seo = match self.seo {
            Some(seo) => seo.validate().map_err(ApiError::BadRequest)?,
            None => existing.map(|page| Seo::parse(&page.seo)).unwrap_or_default(),
        };
        let groups = match self.groups {
            None => None,
            Some(list) => {
                let mut clean: Vec<String> = Vec::new();
                for name in list {
                    let name = name.trim().to_string();
                    if name.is_empty() || clean.contains(&name) {
                        continue;
                    }
                    let too_long = name.chars().count() > MAX_GROUP_FOLDER_LEN;
                    if too_long || name.chars().any(char::is_control) {
                        return Err(ApiError::BadRequest("Invalid group name.".into()));
                    }
                    clean.push(name);
                }
                if clean.len() > MAX_GROUPS {
                    return Err(ApiError::BadRequest(format!(
                        "A page shows at most {MAX_GROUPS} groups."
                    )));
                }
                Some(clean)
            }
        };
        let group_labels = match self.group_labels {
            None => None,
            Some(list) => {
                let mut clean: Vec<GroupLabel> = Vec::new();
                for entry in list {
                    let label = entry.label.trim().to_string();
                    if label.is_empty() || clean.iter().any(|c| c.target_id == entry.target_id) {
                        continue;
                    }
                    if label.chars().count() > MAX_LABEL_LEN || label.chars().any(char::is_control)
                    {
                        return Err(ApiError::BadRequest(format!(
                            "Labels are limited to {MAX_LABEL_LEN} characters of plain text."
                        )));
                    }
                    clean.push(GroupLabel { target_id: entry.target_id, label });
                }
                Some(clean)
            }
        };
        Ok(StatusPageInput {
            slug,
            title,
            description,
            published: self.published.unwrap_or(false),
            theme,
            show_uptime_days,
            accent,
            scenes,
            scene_rotation,
            simple,
            show_calendar,
            show_latency,
            footer_text,
            homepage_url,
            subscribe_channel_id,
            link_origin: origin,
            domain,
            seo,
            hide_branding,
            subscribe_email,
            groups,
            group_labels,
        })
    }
}

/// Refuse un domaine qui est aussi l'adresse de l'administration : l'URL
/// publique réglée, ou l'hôte par lequel l'administrateur enregistre la page.
/// Sans cela, l'enregistrement fermerait l'interface à celui qui l'utilise.
async fn check_not_admin_host(state: &AppState, domain: &str, origin: &str) -> ApiResult<()> {
    let global = crate::notify::policy_store::load_global(&state.pool).await.ok();
    let env_url = crate::config::env_var("DUMBMONIT_PUBLIC_URL");
    let public_url = global.and_then(|global| global.public_url(env_url.as_deref()));
    let admin_hosts = [public_url.as_deref(), Some(origin)]
        .into_iter()
        .flatten()
        .filter_map(crate::status_host::host_of_url);
    for host in admin_hosts {
        if host == domain {
            return Err(ApiError::BadRequest(format!(
                "{domain} is the address of DumbMonit itself: on a status page domain only \
                 the page is reachable, so pick another name (for example status.{domain})."
            )));
        }
    }
    Ok(())
}

/// Valide la liste de scènes : jeu fermé [`SCENES`], sans doublon saisi. Une
/// scène retirée (ancienne sauvegarde, ancien client) est repliée sur la plus
/// proche conservée, sans doublon. Rend la liste enregistrable (virgules).
fn validate_scenes(scenes: Vec<String>) -> ApiResult<String> {
    if scenes.len() > LEGACY_SCENES.len() + SCENES.len() {
        return Err(ApiError::BadRequest("A page shows too many scenes.".into()));
    }
    let mut kept: Vec<&'static str> = Vec::new();
    for (i, scene) in scenes.iter().enumerate() {
        let Some(canonical) = canonical_scene(scene) else {
            return Err(ApiError::BadRequest(format!(
                "Unknown scene \"{scene}\" (expected: {}).",
                SCENES.join(", ")
            )));
        };
        if scenes[..i].contains(scene) {
            return Err(ApiError::BadRequest(format!("The scene \"{scene}\" is listed twice.")));
        }
        if !kept.contains(&canonical) {
            kept.push(canonical);
        }
    }
    Ok(kept.join(","))
}

/// Lien vers le site de l'organisation : vide, ou une URL http(s) absolue sans
/// identifiants. Tout autre schéma (`javascript:`, `data:`) est refusé.
pub(crate) fn validate_homepage(raw: &str) -> ApiResult<String> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    let invalid =
        || ApiError::BadRequest("The website link must be an http:// or https:// address.".into());
    if raw.len() > MAX_URL_LEN {
        return Err(ApiError::BadRequest(format!(
            "The website link is limited to {MAX_URL_LEN} characters."
        )));
    }
    let url = reqwest::Url::parse(raw).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none_or(str::is_empty)
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(invalid());
    }
    Ok(url.to_string())
}

/// `^[a-z0-9-]{2,40}$`, sans expression régulière : la règle tient en une ligne.
fn is_valid_slug(slug: &str) -> bool {
    let len = slug.len();
    (2..=40).contains(&len)
        && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Dérive un slug d'un titre : « My Homelab » devient `my-homelab`.
fn slugify(title: &str) -> String {
    let mut slug = String::with_capacity(title.len());
    let mut pending_dash = false;
    for ch in title.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch);
        } else {
            pending_dash = true;
        }
    }
    slug.truncate(40);
    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

fn page_not_found(id: i64) -> ApiError {
    ApiError::NotFound(format!("Status page {id} not found."))
}

fn incident_not_found(id: i64) -> ApiError {
    ApiError::NotFound(format!("Incident {id} not found."))
}

/// Traduit la violation d'unicité du slug ou du domaine en conflit lisible.
fn duplicate_slug_to_conflict(error: anyhow::Error) -> ApiError {
    let text = format!("{error:#}");
    if text.contains("UNIQUE constraint failed: status_pages.slug") {
        ApiError::Conflict("Another page already uses this slug.".into())
    } else if text.contains("UNIQUE constraint failed: status_pages.domain") {
        ApiError::Conflict("Another page already uses this domain.".into())
    } else {
        ApiError::Internal(error)
    }
}

async fn page_view(state: &AppState, page: StatusPage) -> ApiResult<StatusPageView> {
    let items = db::status_pages::list_items(&state.pool, page.id).await?;
    let groups = db::status_pages::list_groups(&state.pool, page.id).await?;
    let group_labels = db::status_pages::list_group_labels(&state.pool, page.id).await?;
    Ok(StatusPageView { page, items, groups, group_labels })
}

pub async fn list_pages(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<StatusPageView>>> {
    let visible = access.visible_targets(&state.pool).await?;
    let pages = db::status_pages::list_pages(&state.pool).await?;
    let mut items: HashMap<i64, Vec<PageItem>> = HashMap::new();
    for item in db::status_pages::list_all_items(&state.pool).await? {
        items.entry(item.page_id).or_default().push(item);
    }
    let mut groups: HashMap<i64, Vec<String>> = HashMap::new();
    for (page_id, name) in db::status_pages::list_all_groups(&state.pool).await? {
        groups.entry(page_id).or_default().push(name);
    }
    let mut labels: HashMap<i64, Vec<GroupLabel>> = HashMap::new();
    for (page_id, entry) in db::status_pages::list_all_group_labels(&state.pool).await? {
        labels.entry(page_id).or_default().push(entry);
    }
    // Compte restreint par ses équipes : seulement les pages dont tous les
    // composants sont des équipements qu'il voit. Mode MSP : en plus, les pages
    // de ses clients seulement (celles de l'instance pour une vue
    // d'administrateur).
    let owners =
        crate::auth::clients::owners(&state.pool, crate::auth::clients::Kind::StatusPage).await?;
    let restricted = access.is_scoped();
    let pages: Vec<_> = pages
        .into_iter()
        .filter(|page| {
            crate::auth::clients::visible_owner(&access, owners.get(&page.id).copied(), !restricted)
        })
        .filter(|page| {
            !restricted
                || visible.as_ref().is_none_or(|visible| {
                    items.get(&page.id).is_some_and(|list| {
                        !list.is_empty() && list.iter().all(|i| visible.contains(&i.target_id))
                    })
                })
        })
        .collect();
    Ok(Json(
        pages
            .into_iter()
            .map(|page| {
                let items = items.remove(&page.id).unwrap_or_default();
                let groups = groups.remove(&page.id).unwrap_or_default();
                let group_labels = labels.remove(&page.id).unwrap_or_default();
                StatusPageView { page, items, groups, group_labels }
            })
            .collect(),
    ))
}

pub async fn get_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<StatusPageView>> {
    let page =
        db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    Ok(Json(page_view(&state, page).await?))
}

/// Aperçu de l'éditeur : le document public de la page enregistrée, publiée ou
/// non, avec l'identifiant de surveillance de chaque service. L'éditeur y
/// reprend l'état et l'historique des services pendant qu'on modifie la page.
pub async fn preview_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Response> {
    let page =
        db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    let status = build_public(&state, &page, true).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(status)).into_response())
}

pub async fn create_page(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    origin: super::oidc::Origin,
    Json(payload): Json<StatusPagePayload>,
) -> ApiResult<(StatusCode, Json<StatusPageView>)> {
    let input = payload.validate(&state, None, origin.as_str().to_string()).await?;
    let id = db::status_pages::create_page(&state.pool, &input)
        .await
        .map_err(duplicate_slug_to_conflict)?;
    // Mode MSP : créée dans la vue d'un client, la page lui appartient ; sinon
    // elle est à l'instance.
    crate::auth::clients::adopt(&state.pool, &access, crate::auth::clients::Kind::StatusPage, id)
        .await?;
    let page =
        db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    invalidate_cache();
    tracing::info!(slug = %page.slug, "status page created");
    Ok((StatusCode::CREATED, Json(page_view(&state, page).await?)))
}

pub async fn update_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    origin: super::oidc::Origin,
    Json(payload): Json<StatusPagePayload>,
) -> ApiResult<Json<StatusPageView>> {
    let existing =
        db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    let input = payload.validate(&state, Some(&existing), origin.as_str().to_string()).await?;
    let updated = db::status_pages::update_page(&state.pool, id, &input)
        .await
        .map_err(duplicate_slug_to_conflict)?;
    if !updated {
        return Err(page_not_found(id));
    }
    invalidate_cache();
    let page =
        db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    Ok(Json(page_view(&state, page).await?))
}

pub async fn delete_page(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if db::status_pages::delete_page(&state.pool, id).await? {
        branding::remove_logo_file(&state, id).await;
        invalidate_cache();
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(page_not_found(id))
    }
}

#[cfg(test)]
mod tests;
