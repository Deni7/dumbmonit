//! Réglages d'instance : nom, langue, fuseau et assistant de démarrage
//! ([`crate::instance`]).
//!
//! * `GET /instance` — public : seulement `setup_completed`, dont l'interface
//!   a besoin avant même la connexion. Rien d'autre : le nom de l'instance n'a
//!   pas à être lu par un inconnu.
//! * `GET /instance/settings` — tout le réglage, pour une session ouverte.
//! * `PUT /instance/settings` — administrateur ; chaque champ absent garde sa
//!   valeur. Un ancien client qui envoie encore `product_mode` n'est pas
//!   refusé : le champ est ignoré, il n'y a plus qu'une interface.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::middleware::AdminIdentity;
use crate::instance::{self, InstanceSettings, LANGUAGES, MAX_NAME_LEN};
use crate::state::AppState;

/// Routes ouvertes sans session.
pub fn public_routes() -> Router<AppState> {
    Router::new().route("/instance", get(public))
}

/// Routes sous le garde de session.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/instance/settings", get(read).put(update))
        .route("/instance/effective-config", get(effective_config))
}

/// Réponse publique, volontairement minimale.
#[derive(Debug, Serialize)]
pub struct PublicInstance {
    pub setup_completed: bool,
}

/// Modification envoyée par l'interface. Chaque champ absent conserve sa valeur.
#[derive(Debug, Default, Deserialize)]
pub struct InstancePatch {
    #[serde(default)]
    pub setup_completed: Option<bool>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub oncall_enabled: Option<bool>,
    #[serde(default)]
    pub accent: Option<String>,
}

/// La démonstration publique n'a pas d'assistant : son parc est déjà là.
fn effective(state: &AppState, mut value: InstanceSettings) -> InstanceSettings {
    if state.config.demo {
        value.setup_completed = true;
    }
    value
}

async fn public(State(state): State<AppState>) -> Json<PublicInstance> {
    let value = effective(&state, instance::load(&state.pool).await);
    Json(PublicInstance { setup_completed: value.setup_completed })
}

async fn read(State(state): State<AppState>) -> Json<InstanceSettings> {
    Json(effective(&state, instance::load(&state.pool).await))
}

async fn update(
    AdminIdentity(principal): AdminIdentity,
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(patch): Json<InstancePatch>,
) -> ApiResult<Json<InstanceSettings>> {
    let mut value = instance::load(&state.pool).await;
    let changed = changed_fields(&patch);
    apply(&mut value, patch)?;
    instance::save(&state.pool, &value).await?;
    // Les noms des champs seulement, pas leurs valeurs.
    if !changed.is_empty() {
        audit::record(
            &state.pool,
            Some(&principal.label()),
            "instance.settings",
            Some(&changed.join(", ")),
            ip,
        )
        .await;
    }
    Ok(Json(effective(&state, value)))
}

/// `GET /api/instance/effective-config` — les réglages effectifs hors secrets
/// (section « À propos »). Les sauvegardes locales y sont données telles qu'elles
/// s'appliquent, réglage de l'interface compris.
async fn effective_config(
    _: AdminIdentity,
    State(state): State<AppState>,
) -> Json<Vec<crate::config::EffectiveSetting>> {
    let mut settings = state.config.effective();
    let backup = crate::backup::local::Settings::load(&state.pool, &state.config).await;
    for setting in &mut settings {
        match setting.key {
            "backup_enabled" => {
                setting.value = if backup.enabled { "on" } else { "off" }.to_string();
            }
            "backup_interval" => {
                setting.value = format!("{} h", backup.interval.as_secs() / 3600);
            }
            "backup_keep" => setting.value = backup.keep.to_string(),
            _ => {}
        }
    }
    Json(settings)
}

/// Les champs qu'une modification touche, pour le journal d'audit.
fn changed_fields(patch: &InstancePatch) -> Vec<&'static str> {
    [
        ("setup_completed", patch.setup_completed.is_some()),
        ("name", patch.name.is_some()),
        ("language", patch.language.is_some()),
        ("timezone", patch.timezone.is_some()),
        ("oncall_enabled", patch.oncall_enabled.is_some()),
        ("accent", patch.accent.is_some()),
    ]
    .into_iter()
    .filter_map(|(name, set)| set.then_some(name))
    .collect()
}

/// Valide et applique une modification. Rien n'est écrit si un champ est refusé.
fn apply(value: &mut InstanceSettings, patch: InstancePatch) -> ApiResult<()> {
    if let Some(name) = patch.name {
        let name = name.trim();
        if name.chars().count() > MAX_NAME_LEN {
            return Err(ApiError::BadRequest(format!(
                "The instance name is limited to {MAX_NAME_LEN} characters."
            )));
        }
        if name.chars().any(char::is_control) {
            return Err(ApiError::BadRequest(
                "The instance name cannot contain control characters.".into(),
            ));
        }
        value.name = name.to_string();
    }
    if let Some(language) = patch.language {
        let language = language.trim();
        if !language.is_empty() && !LANGUAGES.contains(&language) {
            return Err(ApiError::BadRequest(format!(
                "Unknown language \"{language}\" (available: {})",
                LANGUAGES.join(", ")
            )));
        }
        value.language = language.to_string();
    }
    if let Some(timezone) = patch.timezone {
        let timezone = timezone.trim();
        if !timezone.is_empty() && crate::reports::schedule::parse_timezone(timezone).is_none() {
            return Err(ApiError::BadRequest(format!(
                "Unknown time zone \"{timezone}\" (expected an IANA name such as Europe/Paris)"
            )));
        }
        value.timezone = timezone.to_string();
    }
    if let Some(accent) = patch.accent {
        let accent = accent.trim();
        if !accent.is_empty() && !instance::is_valid_accent(accent) {
            return Err(ApiError::BadRequest(
                "The accent colour must be empty or a #rrggbb value.".into(),
            ));
        }
        value.accent = accent.to_ascii_lowercase();
    }
    if let Some(on) = patch.oncall_enabled {
        value.oncall_enabled = on;
    }
    if let Some(done) = patch.setup_completed {
        // L'assistant peut être relancé depuis les réglages : remettre à faux
        // est permis, et la date suit. Répéter la même valeur ne la change pas.
        if done != value.setup_completed {
            value.setup_completed_at = done.then(crate::db::now);
        }
        value.setup_completed = done;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_keeps_what_it_does_not_name() {
        let mut value = InstanceSettings { name: "Lab".into(), ..InstanceSettings::default() };
        let patch = InstancePatch { language: Some("fr".into()), ..Default::default() };
        assert!(apply(&mut value, patch).is_ok());
        assert_eq!(value.language, "fr");
        assert_eq!(value.name, "Lab");
    }

    #[test]
    fn the_accent_is_stored_in_lower_case_and_can_be_cleared() {
        let mut value = InstanceSettings::default();
        let set = InstancePatch { accent: Some(" #2563EB ".into()), ..Default::default() };
        assert!(apply(&mut value, set).is_ok());
        assert_eq!(value.accent, "#2563eb");
        let keep = InstancePatch { name: Some("Lab".into()), ..Default::default() };
        assert!(apply(&mut value, keep).is_ok());
        assert_eq!(value.accent, "#2563eb");
        let clear = InstancePatch { accent: Some(String::new()), ..Default::default() };
        assert!(apply(&mut value, clear).is_ok());
        assert!(value.accent.is_empty());
    }

    #[test]
    fn bad_values_are_refused() {
        let mut value = InstanceSettings::default();
        for patch in [
            InstancePatch { language: Some("klingon".into()), ..Default::default() },
            InstancePatch { timezone: Some("Mars/Olympus".into()), ..Default::default() },
            InstancePatch { name: Some("x".repeat(MAX_NAME_LEN + 1)), ..Default::default() },
            InstancePatch { accent: Some("blue".into()), ..Default::default() },
            InstancePatch { accent: Some("#12345".into()), ..Default::default() },
        ] {
            assert!(apply(&mut value, patch).is_err());
        }
        assert_eq!(value, InstanceSettings::default());
    }

    #[test]
    fn an_old_client_sending_the_product_mode_is_not_refused() {
        let patch: InstancePatch =
            serde_json::from_str(r#"{"product_mode":"homelab","name":"Lab"}"#).unwrap();
        let mut value = InstanceSettings::default();
        assert!(apply(&mut value, patch).is_ok());
        assert_eq!(value.name, "Lab");
    }

    #[test]
    fn finishing_the_wizard_stamps_a_date_and_reopening_clears_it() {
        let mut value = InstanceSettings::default();
        let done = InstancePatch { setup_completed: Some(true), ..Default::default() };
        assert!(apply(&mut value, done).is_ok());
        assert!(value.setup_completed_at.is_some());
        let reopen = InstancePatch { setup_completed: Some(false), ..Default::default() };
        assert!(apply(&mut value, reopen).is_ok());
        assert!(value.setup_completed_at.is_none());
    }
}
