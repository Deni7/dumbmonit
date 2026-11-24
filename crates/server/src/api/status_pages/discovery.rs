//! Annuaire des pages de statut : `GET /api/public/status`.
//!
//! Sert à un client qui connaît l'adresse du serveur et cherche quelle page
//! afficher (le plugin WordPress, « Find status pages »). N'y figurent que les
//! pages publiées, publiques (ni mot de passe ni comptes) et que leur
//! administrateur a laissées dans l'annuaire (`listed`, coché par défaut).
//! Rien d'autre que ce que montre déjà la page elle-même : son adresse, son
//! titre tel qu'écrit et ses langues.
//!
//! Comme le document public, la liste est gardée trente secondes en mémoire :
//! un client qui insiste ne fait pas travailler la base. Sur le domaine public
//! d'une page (`crate::status_host`), cette route n'est pas servie : un
//! visiteur de `status.example.org` n'apprend rien des autres pages.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Instant;

use axum::Json;
use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

use super::CACHE_TTL;
use super::lang::canonical;
use crate::api::ApiResult;
use crate::db;
use crate::db::status_pages::StatusPage;
use crate::state::AppState;

/// Une page de l'annuaire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub slug: String,
    pub title: String,
    /// Adresse de la page publique : absolue quand on la connaît (domaine de la
    /// page, ou origine vue par l'administrateur), sinon `/s/<slug>`, relative
    /// au serveur.
    pub url: String,
    /// Langue par défaut de la page (`en` si elle n'en a pas).
    pub default_lang: String,
    /// Langues dans lesquelles ses textes se lisent : la langue par défaut
    /// d'abord, puis celles qui ont une traduction.
    pub languages: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Directory {
    pages: Vec<Entry>,
}

/// La liste servie et l'instant où elle a été calculée.
type Cached = Option<(Instant, Arc<Value>)>;

static CACHE: LazyLock<Mutex<Cached>> = LazyLock::new(|| Mutex::new(None));

/// Vide la liste en mémoire ; appelé avec `invalidate_cache`.
pub(super) fn invalidate() {
    *CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
}

/// Vrai si la page peut figurer dans l'annuaire.
fn is_listed(page: &StatusPage) -> bool {
    page.published && page.is_public() && page.listed
}

/// L'entrée d'une page, ses langues déjà lues.
fn entry(page: &StatusPage, stored_default: &str, translated: &[String]) -> Entry {
    let default_lang = canonical(stored_default).unwrap_or("en").to_string();
    let mut languages = vec![default_lang.clone()];
    for lang in translated {
        if !languages.contains(lang) {
            languages.push(lang.clone());
        }
    }
    Entry {
        slug: page.slug.clone(),
        title: page.title.clone(),
        url: crate::status_seo::page_url(page).unwrap_or_else(|| format!("/s/{}", page.slug)),
        default_lang,
        languages,
    }
}

async fn build(state: &AppState) -> ApiResult<Value> {
    let pages = db::status_pages::list_pages(&state.pool).await?;
    let defaults: HashMap<i64, String> =
        sqlx::query_as::<_, (i64, String)>("SELECT id, default_lang FROM status_pages")
            .fetch_all(&state.pool)
            .await
            .map_err(anyhow::Error::from)?
            .into_iter()
            .collect();
    let mut translated: HashMap<i64, Vec<String>> = HashMap::new();
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT page_id, lang FROM status_page_translations ORDER BY page_id, lang")
            .fetch_all(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    for (page_id, lang) in rows {
        translated.entry(page_id).or_default().push(lang);
    }
    let pages = pages
        .iter()
        .filter(|page| is_listed(page))
        .map(|page| {
            entry(
                page,
                defaults.get(&page.id).map_or("", String::as_str),
                translated.get(&page.id).map(Vec::as_slice).unwrap_or_default(),
            )
        })
        .collect();
    Ok(serde_json::to_value(Directory { pages })?)
}

/// `GET /api/public/status` : les pages publiques que l'on peut afficher ailleurs.
pub async fn list(State(state): State<AppState>) -> ApiResult<Response> {
    let cached = {
        let cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.as_ref().filter(|(at, _)| at.elapsed() < CACHE_TTL).map(|(_, body)| Arc::clone(body))
    };
    let body = match cached {
        Some(body) => body,
        None => {
            let body = Arc::new(build(&state).await?);
            *CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) =
                Some((Instant::now(), Arc::clone(&body)));
            body
        }
    };
    Ok(([(header::CACHE_CONTROL, "public, max-age=30")], Json((*body).clone())).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(access_mode: &str, published: bool, listed: bool) -> StatusPage {
        StatusPage {
            id: 1,
            slug: "home".into(),
            title: "Home".into(),
            description: String::new(),
            published,
            theme: "auto".into(),
            show_uptime_days: 90,
            created_at: String::new(),
            updated_at: String::new(),
            accent: "default".into(),
            scenes: String::new(),
            scene_rotation: "visit".into(),
            simple: false,
            show_calendar: false,
            show_latency: false,
            footer_text: String::new(),
            homepage_url: String::new(),
            logo_type: None,
            subscribe_channel_id: None,
            link_origin: "https://monit.example".into(),
            domain: None,
            seo: String::new(),
            access_mode: access_mode.into(),
            has_password: access_mode == "password",
            hide_branding: false,
            subscribe_email: false,
            listed,
        }
    }

    #[test]
    fn seules_les_pages_publiees_publiques_et_listees_figurent() {
        assert!(is_listed(&page("public", true, true)));
        assert!(!is_listed(&page("public", false, true)));
        assert!(!is_listed(&page("public", true, false)));
        assert!(!is_listed(&page("password", true, true)));
        assert!(!is_listed(&page("users", true, true)));
    }

    #[test]
    fn l_entree_donne_l_adresse_et_les_langues() {
        let found = entry(&page("public", true, true), "", &["de".into(), "en".into()]);
        assert_eq!(found.url, "https://monit.example/s/home");
        assert_eq!(found.default_lang, "en");
        assert_eq!(found.languages, ["en", "de"]);

        let mut on_domain = page("public", true, true);
        on_domain.domain = Some("status.example.org".into());
        on_domain.link_origin = String::new();
        let found = entry(&on_domain, "fr", &[]);
        assert_eq!(found.url, "https://status.example.org/");
        assert_eq!(found.languages, ["fr"]);

        let mut no_origin = page("public", true, true);
        no_origin.link_origin = String::new();
        assert_eq!(entry(&no_origin, "", &[]).url, "/s/home");
    }
}
