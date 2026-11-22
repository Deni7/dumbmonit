use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
    /// Commit dont est issu le binaire, raccourci ; absent d'une compilation locale.
    #[serde(skip_serializing_if = "Option::is_none")]
    build: Option<&'static str>,
    database: ComponentHealth,
    victoria: ComponentHealth,
    /// Mode démonstration publique (`DUMBMONIT_DEMO`) : lecture seule, parc fictif.
    demo: bool,
    /// Volume des données (`/data`) : place libre et taille. Session ouverte
    /// seulement, et absent quand le système ne sait pas le mesurer.
    #[serde(skip_serializing_if = "Option::is_none")]
    data_disk: Option<DataDisk>,
}

#[derive(Serialize)]
struct DataDisk {
    free_bytes: u64,
    total_bytes: u64,
    /// Taille de la base SQLite et de son journal.
    database_bytes: u64,
}

#[derive(Serialize)]
struct ComponentHealth {
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// VictoriaMetrics seulement : `true` quand le serveur le lance lui-même,
    /// `false` quand `DUMBMONIT_VM_URL` désigne une instance externe.
    #[serde(skip_serializing_if = "Option::is_none")]
    embedded: Option<bool>,
}

impl ComponentHealth {
    fn from(result: anyhow::Result<()>) -> Self {
        match result {
            Ok(()) => Self { ok: true, error: None, embedded: None },
            Err(error) => Self { ok: false, error: Some(error.to_string()), embedded: None },
        }
    }

    /// Version publique : l'état sans le détail de la panne.
    fn anonymous(self) -> Self {
        let error = (!self.ok).then(|| "unavailable (sign in for details)".to_string());
        Self { error, ..self }
    }

    fn embedded(self, embedded: bool) -> Self {
        Self { embedded: Some(embedded), ..self }
    }
}

/// Commit dont est issu le binaire : `DUMBMONIT_BUILD` à la compilation, posé par
/// le Dockerfile depuis la CI. Raccourci à 7 caractères comme `git log --oneline`.
fn build_id() -> Option<&'static str> {
    let build = option_env!("DUMBMONIT_BUILD")?.trim();
    (!build.is_empty()).then(|| build.get(..7).unwrap_or(build))
}

/// État de santé du serveur et de ses dépendances.
///
/// Répond toujours 200 : c'est un rapport de diagnostic, pas une sonde de vivacité.
/// Un composant en panne se lit dans le corps de la réponse, ce qui permet à
/// l'interface d'afficher précisément ce qui ne va pas.
///
/// Le texte d'une panne (message d'erreur de SQLite ou de `reqwest`, qui peut
/// recopier l'URL de `DUMBMONIT_VM_URL` et ses identifiants) n'est rendu qu'à une
/// session ouverte ; un appel anonyme ne lit que `ok`.
pub async fn health(State(state): State<AppState>, headers: HeaderMap) -> Json<Health> {
    let database =
        sqlx::query("SELECT 1").execute(&state.pool).await.map(|_| ()).map_err(anyhow::Error::from);

    let victoria = state.victoria.health().await;

    let all_ok = database.is_ok() && victoria.is_ok();
    let signed_in = matches!(
        crate::auth::middleware::current_session(&state.pool, &headers).await,
        Ok(Some(_))
    );
    let shown = |component: ComponentHealth| {
        if signed_in { component } else { component.anonymous() }
    };

    Json(Health {
        status: if all_ok { "ok" } else { "degraded" },
        version: env!("CARGO_PKG_VERSION"),
        build: build_id(),
        database: shown(ComponentHealth::from(database)),
        victoria: shown(ComponentHealth::from(victoria).embedded(state.config.vm_embedded())),
        demo: state.config.demo,
        data_disk: if signed_in { data_disk(&state).await } else { None },
    })
}

/// Place libre du volume des données : un seul `statvfs`, rien de plus.
async fn data_disk(state: &AppState) -> Option<DataDisk> {
    let (free_bytes, total_bytes) = crate::selfmon::volume(&state.config.data_dir)?;
    let database_bytes = crate::selfmon::database_bytes(&state.config.database_path()).await;
    Some(DataDisk { free_bytes, total_bytes, database_bytes })
}

/// Réponse de `GET /api/ready`.
#[derive(Serialize)]
pub struct Ready {
    ready: bool,
    database: bool,
    victoria: bool,
}

/// Sonde de disponibilité : 200 quand la base et VictoriaMetrics répondent,
/// 503 sinon. Publique comme `/api/health`, et sans le détail des pannes : un
/// orchestrateur n'a besoin que du code de retour. `/api/health` reste la sonde
/// de vivacité (elle répond toujours 200).
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<Ready>) {
    let database = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    let victoria = state.victoria.health().await.is_ok();
    let ready = database && victoria;
    let status = if ready { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(Ready { ready, database, victoria }))
}
