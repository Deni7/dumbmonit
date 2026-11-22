//! Langues d'une page de statut publique : choix de la langue d'un visiteur,
//! traductions des textes de l'administrateur, et leur édition.
//!
//! Les messages d'un incident restent tels qu'écrits : seuls le titre, la
//! description, le pied de page et les noms de services ou de groupes se
//! traduisent. Le document public mis en cache est traduit après coup, donc le
//! cache ne dépend pas de la langue.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{MAX_DESCRIPTION_LEN, MAX_FOOTER_LEN, MAX_LABEL_LEN, MAX_TITLE_LEN, page_not_found};
use crate::api::{ApiError, ApiResult};
use crate::db;
use crate::db::status_lang::{SUPPORTED, Translation};
use crate::state::AppState;

/// Nombre maximal de libellés traduits par langue.
const MAX_LABELS: usize = 400;

/// La langue de l'interface correspondant à une étiquette (`fr-CA`, `pt_BR`,
/// `zh-CN`…), parmi les neuf prises en charge.
pub fn canonical(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().replace('_', "-").to_ascii_lowercase();
    if tag.is_empty() || tag == "*" {
        return None;
    }
    if let Some(exact) = SUPPORTED.iter().find(|s| s.to_ascii_lowercase() == tag) {
        return Some(*exact);
    }
    let primary = tag.split('-').next().unwrap_or("");
    match primary {
        // Chinois : l'interface n'a que le simplifié.
        "zh" => Some("zh-Hans"),
        // Portugais : le brésilien à part, le reste en portugais européen.
        "pt" => Some(if tag == "pt-br" { "pt-BR" } else { "pt" }),
        other => SUPPORTED.iter().find(|s| s.to_ascii_lowercase() == other).copied(),
    }
}

/// Choisit la langue d'un visiteur : `?lang=` d'abord, puis `Accept-Language`
/// par poids décroissant. `None` : aucune des neuf, la page retombe sur sa
/// langue par défaut.
pub fn pick_language(query: Option<&str>, accept: Option<&str>) -> Option<&'static str> {
    if let Some(found) = query.and_then(canonical) {
        return Some(found);
    }
    let mut ranked: Vec<(f32, usize, &str)> = accept
        .unwrap_or("")
        .split(',')
        .enumerate()
        .filter_map(|(index, part)| {
            let mut pieces = part.split(';');
            let tag = pieces.next()?.trim();
            let weight = pieces
                .find_map(|p| {
                    p.trim().strip_prefix("q=").and_then(|q| q.trim().parse::<f32>().ok())
                })
                .unwrap_or(1.0);
            (weight > 0.0 && !tag.is_empty()).then_some((weight, index, tag))
        })
        .collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().find_map(|(_, _, tag)| canonical(tag))
}

/// Langue par défaut effective : celle de la page, l'anglais si elle n'en a pas.
fn effective_default(stored: &str) -> &'static str {
    canonical(stored).unwrap_or("en")
}

/// Applique une traduction au document public : un champ vide garde le texte écrit.
pub fn apply(body: &mut Value, translation: &Translation) {
    if let Some(page) = body.get_mut("page").and_then(Value::as_object_mut) {
        for (key, text) in [
            ("title", &translation.title),
            ("description", &translation.description),
            ("footer_text", &translation.footer_text),
        ] {
            if !text.trim().is_empty() {
                page.insert(key.to_string(), Value::String(text.clone()));
            }
        }
    }
    let rename = |value: &mut Value, field: &str| {
        let Some(original) = value.get(field).and_then(Value::as_str) else { return };
        if let Some(text) = translation.labels.get(original).filter(|t| !t.trim().is_empty()) {
            value[field] = Value::String(text.clone());
        }
    };
    if let Some(groups) = body.get_mut("groups").and_then(Value::as_array_mut) {
        for group in groups {
            rename(group, "name");
            if let Some(items) = group.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    rename(item, "label");
                }
            }
        }
    }
}

/// Le document public dans la langue du visiteur, avec `default_lang`, `lang`
/// et `languages` (celles qui ont des textes traduits) dans `page`.
pub(super) async fn localise(
    state: &AppState,
    slug: &str,
    headers: &HeaderMap,
    query: Option<&str>,
    body: &Value,
) -> ApiResult<Value> {
    let mut body = body.clone();
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT id, default_lang FROM status_pages WHERE slug = ?")
            .bind(slug)
            .fetch_optional(&state.pool)
            .await
            .map_err(anyhow::Error::from)?;
    let Some((page_id, stored)) = row else { return Ok(body) };
    let default = effective_default(&stored);
    let accept = headers.get(axum::http::header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok());
    let lang = pick_language(query, accept).unwrap_or(default);
    let translations = db::status_lang::list(&state.pool, page_id).await?;
    if let Some(found) = translations.iter().find(|t| t.lang == lang) {
        apply(&mut body, found);
    }
    if let Some(page) = body.get_mut("page").and_then(Value::as_object_mut) {
        page.insert("default_lang".into(), Value::String(default.into()));
        page.insert("lang".into(), Value::String(lang.into()));
        page.insert(
            "languages".into(),
            Value::Array(translations.iter().map(|t| Value::String(t.lang.clone())).collect()),
        );
    }
    Ok(body)
}

// --------------------------------------------------------------------------
// Administration
// --------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct Languages {
    /// Langue par défaut de l'interface de la page ; `""` : l'anglais.
    #[serde(default)]
    pub default_lang: String,
    #[serde(default)]
    pub translations: Vec<Translation>,
}

pub async fn get_languages(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Languages>> {
    db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    Ok(Json(Languages {
        default_lang: db::status_lang::default_lang(&state.pool, id).await?,
        translations: db::status_lang::list(&state.pool, id).await?,
    }))
}

/// Valide la saisie : langues connues et uniques, longueurs comme celles des
/// textes d'origine. Les lignes entièrement vides sont écartées.
pub fn validate(input: Languages) -> ApiResult<Languages> {
    let default_lang = match input.default_lang.trim() {
        "" => String::new(),
        tag => canonical(tag)
            .filter(|found| found.eq_ignore_ascii_case(tag))
            .ok_or_else(|| ApiError::BadRequest(format!("Unknown language \"{tag}\".")))?
            .to_string(),
    };
    let too_long = |field: &str, max: usize| {
        ApiError::BadRequest(format!("{field} is limited to {max} characters."))
    };
    let mut seen = std::collections::HashSet::new();
    let mut translations = Vec::new();
    for mut t in input.translations {
        let lang = SUPPORTED
            .iter()
            .find(|s| **s == t.lang.trim())
            .ok_or_else(|| ApiError::BadRequest(format!("Unknown language \"{}\".", t.lang)))?;
        if !seen.insert(*lang) {
            return Err(ApiError::BadRequest(format!("Language \"{lang}\" is listed twice.")));
        }
        t.lang = (*lang).to_string();
        t.title = t.title.trim().to_string();
        t.description = t.description.trim().to_string();
        t.footer_text = t.footer_text.trim().to_string();
        if t.title.chars().count() > MAX_TITLE_LEN {
            return Err(too_long("Title", MAX_TITLE_LEN));
        }
        if t.description.chars().count() > MAX_DESCRIPTION_LEN {
            return Err(too_long("Description", MAX_DESCRIPTION_LEN));
        }
        if t.footer_text.chars().count() > MAX_FOOTER_LEN {
            return Err(too_long("Footer", MAX_FOOTER_LEN));
        }
        if t.labels.len() > MAX_LABELS {
            return Err(ApiError::BadRequest(format!(
                "At most {MAX_LABELS} translated names per language."
            )));
        }
        t.labels = t
            .labels
            .into_iter()
            .map(|(from, to)| (from.trim().to_string(), to.trim().to_string()))
            .filter(|(from, to)| !from.is_empty() && !to.is_empty())
            .collect();
        if t.labels.iter().any(|(from, to)| {
            from.chars().count() > MAX_LABEL_LEN || to.chars().count() > MAX_LABEL_LEN
        }) {
            return Err(too_long("A name", MAX_LABEL_LEN));
        }
        let empty = t.title.is_empty()
            && t.description.is_empty()
            && t.footer_text.is_empty()
            && t.labels.is_empty();
        if !empty {
            translations.push(t);
        }
    }
    Ok(Languages { default_lang, translations })
}

pub async fn put_languages(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<Languages>,
) -> ApiResult<Json<Languages>> {
    db::status_pages::get_page(&state.pool, id).await?.ok_or_else(|| page_not_found(id))?;
    let languages = validate(input)?;
    db::status_lang::replace(&state.pool, id, &languages.default_lang, &languages.translations)
        .await?;
    super::invalidate_cache();
    Ok(Json(languages))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn les_etiquettes_se_ramenent_aux_neuf_langues() {
        assert_eq!(canonical("fr-CA"), Some("fr"));
        assert_eq!(canonical("pt_BR"), Some("pt-BR"));
        assert_eq!(canonical("pt-PT"), Some("pt"));
        assert_eq!(canonical("zh-CN"), Some("zh-Hans"));
        assert_eq!(canonical("zh-Hans"), Some("zh-Hans"));
        assert_eq!(canonical("ja"), None);
        assert_eq!(canonical("*"), None);
    }

    #[test]
    fn accept_language_suit_les_poids_et_saute_les_inconnues() {
        assert_eq!(pick_language(None, Some("ja, de;q=0.8, fr;q=0.9")), Some("fr"));
        assert_eq!(pick_language(None, Some("ja, ko;q=0.5")), None);
        assert_eq!(pick_language(None, Some("fr;q=0, es;q=0.1")), Some("es"));
        assert_eq!(pick_language(None, None), None);
    }

    #[test]
    fn le_parametre_lang_prime_sur_l_en_tete() {
        assert_eq!(pick_language(Some("de"), Some("fr")), Some("de"));
        assert_eq!(pick_language(Some("xx"), Some("fr")), Some("fr"));
    }

    #[test]
    fn la_langue_par_defaut_vide_est_l_anglais() {
        assert_eq!(effective_default(""), "en");
        assert_eq!(effective_default("fr"), "fr");
    }

    #[test]
    fn la_traduction_remplace_les_textes_et_les_noms() {
        let mut body = json!({
            "page": {"title": "Status", "description": "d", "footer_text": "f"},
            "groups": [{"name": "Web", "items": [{"label": "API", "key": "api"}]}]
        });
        let t = Translation {
            lang: "fr".into(),
            title: "État".into(),
            description: String::new(),
            footer_text: "pied".into(),
            labels: [
                ("Web".to_string(), "Site".to_string()),
                ("API".to_string(), "Interface".to_string()),
            ]
            .into(),
        };
        apply(&mut body, &t);
        assert_eq!(body["page"]["title"], "État");
        assert_eq!(body["page"]["description"], "d");
        assert_eq!(body["page"]["footer_text"], "pied");
        assert_eq!(body["groups"][0]["name"], "Site");
        assert_eq!(body["groups"][0]["items"][0]["label"], "Interface");
        assert_eq!(body["groups"][0]["items"][0]["key"], "api");
    }

    #[test]
    fn la_validation_refuse_les_langues_inconnues_ou_doublees() {
        let t =
            |lang: &str| Translation { lang: lang.into(), title: "x".into(), ..Default::default() };
        let bad = Languages { default_lang: "xx".into(), translations: vec![] };
        assert!(validate(bad).is_err());
        let twice = Languages { default_lang: String::new(), translations: vec![t("fr"), t("fr")] };
        assert!(validate(twice).is_err());
        let unknown = Languages { default_lang: String::new(), translations: vec![t("ja")] };
        assert!(validate(unknown).is_err());
        let ok = Languages { default_lang: "fr".into(), translations: vec![t("de"), t("pt-BR")] };
        assert_eq!(validate(ok).ok().map(|l| l.translations.len()), Some(2));
        // Une langue sans aucun texte est écartée.
        let mut blank = Languages { default_lang: String::new(), translations: vec![t("it")] };
        blank.translations[0].title.clear();
        assert!(validate(blank).ok().is_some_and(|l| l.translations.is_empty()));
    }
}
