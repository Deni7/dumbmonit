//! Aperçu des gabarits de message d'un canal, pour le formulaire : le titre
//! et le corps rendus sur une alerte d'exemple, dans le format du service, ou
//! l'erreur qui ferait refuser l'enregistrement (variable inconnue…).

use axum::Json;
use axum::extract::State;
use serde::Deserialize;

use crate::api::{ApiError, ApiResult};
use crate::notify::NotifyError;
use crate::notify::message_template::{self, Preview};
use crate::notify::policy_store;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct PreviewRequest {
    pub kind: String,
    #[serde(default)]
    pub title_template: Option<String>,
    #[serde(default)]
    pub body_template: Option<String>,
}

/// `POST /api/notify/template-preview`.
pub async fn preview(
    State(state): State<AppState>,
    Json(request): Json<PreviewRequest>,
) -> ApiResult<Json<Preview>> {
    let global = policy_store::load_global(&state.pool).await?;
    let env_url = crate::config::env_var("DUMBMONIT_PUBLIC_URL");
    let public_url = global.public_url(env_url.as_deref());
    message_template::preview(
        request.kind.trim(),
        request.title_template.as_deref(),
        request.body_template.as_deref(),
        public_url.as_deref(),
    )
    .map(Json)
    .map_err(|error| match error {
        NotifyError::Config(text) => ApiError::BadRequest(text),
        other => ApiError::BadRequest(other.to_string()),
    })
}
