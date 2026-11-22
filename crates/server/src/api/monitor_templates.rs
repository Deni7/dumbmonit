//! Modèles de moniteur : « Enregistrer comme modèle » sur un moniteur, la liste
//! des modèles (renommer, supprimer) et « Nouveau depuis un modèle » dans
//! l'ajout, qui préremplit le formulaire comme une copie.
//!
//! Un modèle retient le type, les options et étiquettes sans secret, la période
//! et les canaux du moniteur dont il est tiré — jamais son adresse ni ses
//! identifiants. Il est construit ici, à partir de ce qui est enregistré, pas
//! de ce que l'interface enverrait : aucun secret ne peut s'y glisser.

use std::collections::BTreeMap;

use anyhow::Context;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use dumbmonit_proto::TargetId;
use serde::Deserialize;

use crate::api::targets::{REDACTED, reader_tags, redact_userinfo};
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::Identity;
use crate::db;
use crate::db::monitor_templates::{MonitorTemplate, TemplateInput};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/monitor-templates", get(list).post(create))
        .route("/monitor-templates/{id}", get(get_one).put(rename).delete(remove))
}

/// Longueur maximale du nom d'un modèle.
const MAX_NAME_CHARS: usize = 80;

/// Types qui ne naissent pas du formulaire d'ajout : un agent s'installe sur sa
/// machine, ses services viennent de lui.
const NOT_TEMPLATABLE: &[&str] = &["agent", "agent_service"];

fn clean_name(name: &str) -> ApiResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ApiError::BadRequest("The template needs a name.".into()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(ApiError::BadRequest(format!(
            "The template name must be at most {MAX_NAME_CHARS} characters."
        )));
    }
    Ok(name.to_string())
}

/// Les options et étiquettes d'un moniteur qu'un modèle peut garder.
///
/// Écartés : ce dont la clé nomme un secret (`api_key`, `password`…), les
/// en-têtes HTTP (un `Authorization` s'y glisse vite) et toute valeur qui porte
/// un mot de passe (`https://user:pass@hôte`). D'un scénario HTTP ne restent
/// que le nom, la méthode et l'URL de chaque étape, comme pour un lecteur.
pub fn template_tags(tags: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    tags.iter()
        .filter_map(|(key, value)| {
            if dumbmonit_proto::looks_secret(key) || key.eq_ignore_ascii_case("headers") {
                return None;
            }
            if key == "steps" {
                let single = BTreeMap::from([(key.clone(), value.clone())]);
                let skeleton = reader_tags(&single).remove(key)?;
                return (!skeleton.contains(REDACTED)).then(|| (key.clone(), skeleton));
            }
            (redact_userinfo(value) == *value).then(|| (key.clone(), value.clone()))
        })
        .collect()
}

fn not_found(id: i64) -> ApiError {
    ApiError::NotFound(format!("Template {id} not found."))
}

/// Un nom déjà pris est une erreur de saisie, pas une panne.
fn name_conflict(error: anyhow::Error) -> ApiError {
    if format!("{error:#}").contains("UNIQUE constraint failed") {
        ApiError::Conflict("A template with this name already exists.".into())
    } else {
        ApiError::Internal(error)
    }
}

/// `GET /api/monitor-templates`
pub async fn list(State(state): State<AppState>) -> ApiResult<Json<Vec<MonitorTemplate>>> {
    Ok(Json(db::monitor_templates::list(&state.pool).await?))
}

/// `GET /api/monitor-templates/{id}`
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<MonitorTemplate>> {
    let template =
        db::monitor_templates::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    Ok(Json(template))
}

#[derive(Debug, Deserialize)]
pub struct CreatePayload {
    pub name: String,
    /// Le moniteur dont le modèle est tiré.
    pub target_id: TargetId,
}

/// `POST /api/monitor-templates` : enregistre un moniteur comme modèle.
pub async fn create(
    Identity(who): Identity,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Json(payload): Json<CreatePayload>,
) -> ApiResult<(StatusCode, Json<MonitorTemplate>)> {
    let name = clean_name(&payload.name)?;
    let target = db::targets::get(&state.pool, &state.cipher, payload.target_id)
        .await?
        .ok_or_else(|| ApiError::monitor_not_found(&payload.target_id))?;
    if NOT_TEMPLATABLE.contains(&target.kind.as_str()) {
        return Err(ApiError::BadRequest(
            "An agent is installed on its machine: it cannot be saved as a template.".into(),
        ));
    }
    let input = TemplateInput {
        name,
        kind: target.kind.clone(),
        interval_secs: target.interval.as_secs(),
        tags: template_tags(&target.tags),
        channels: db::target_channels::of(&state.pool, target.id).await?,
    };
    let id = db::monitor_templates::create(&state.pool, &input).await.map_err(name_conflict)?;
    let actor = who.label();
    audit::record(&state.pool, Some(&actor), "monitor_template.created", Some(&input.name), ip)
        .await;
    let template = db::monitor_templates::get(&state.pool, id)
        .await?
        .context("modèle introuvable juste après sa création")?;
    Ok((StatusCode::CREATED, Json(template)))
}

#[derive(Debug, Deserialize)]
pub struct RenamePayload {
    pub name: String,
}

/// `PUT /api/monitor-templates/{id}` : renomme un modèle.
pub async fn rename(
    Identity(who): Identity,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<RenamePayload>,
) -> ApiResult<Json<MonitorTemplate>> {
    let name = clean_name(&payload.name)?;
    if !db::monitor_templates::rename(&state.pool, id, &name).await.map_err(name_conflict)? {
        return Err(not_found(id));
    }
    audit::record(&state.pool, Some(&who.label()), "monitor_template.renamed", Some(&name), ip)
        .await;
    let template =
        db::monitor_templates::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    Ok(Json(template))
}

/// `DELETE /api/monitor-templates/{id}`
pub async fn remove(
    Identity(who): Identity,
    ClientIp(ip): ClientIp,
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let template =
        db::monitor_templates::get(&state.pool, id).await?.ok_or_else(|| not_found(id))?;
    db::monitor_templates::delete(&state.pool, id).await?;
    audit::record(
        &state.pool,
        Some(&who.label()),
        "monitor_template.deleted",
        Some(&template.name),
        ip,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    /// Un modèle ne garde jamais un secret, ni dans une clé ni dans une valeur.
    #[test]
    fn un_modele_ne_garde_aucun_secret() {
        let kept = template_tags(&tags(&[
            ("port", "8443"),
            ("api_key", "abc"),
            ("headers", "Authorization: Bearer x"),
            ("url", "https://user:pass@nas.lan/health"),
            ("keyword", "ok"),
            ("env", "prod"),
        ]));
        assert_eq!(kept, tags(&[("port", "8443"), ("keyword", "ok"), ("env", "prod")]));
    }

    #[test]
    fn d_un_scenario_ne_reste_que_le_squelette() {
        let steps = r#"[{"name":"Login","method":"POST","url":"https://app.test/login",
            "headers":[{"name":"Authorization","value":"Bearer JETON"}],"body":"p=SECRET"}]"#;
        let kept = template_tags(&tags(&[("steps", steps)]));
        let shown = &kept["steps"];
        assert!(!shown.contains("JETON") && !shown.contains("SECRET"), "{shown}");
        assert!(shown.contains("https://app.test/login"));
        let unreadable = template_tags(&tags(&[("steps", "pas du json")]));
        assert!(unreadable.is_empty());
    }

    #[test]
    fn un_nom_vide_ou_trop_long_est_refuse() {
        assert!(clean_name("  ").is_err());
        assert!(clean_name(&"x".repeat(MAX_NAME_CHARS + 1)).is_err());
        assert_eq!(clean_name("  Web server ").unwrap(), "Web server");
    }
}
