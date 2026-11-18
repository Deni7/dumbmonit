//! Rapports périodiques par courriel : calendriers (CRUD) et aperçu immédiat.
//!
//! Réservé aux administrateurs, lecture comprise : la liste des destinataires
//! est une liste d'adresses de l'équipe.

use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::api::status_pages::branding;
use crate::api::{ApiError, ApiResult};
use crate::auth::clients;
use crate::auth::middleware::{AdminIdentity, Identity};
use crate::auth::teams::TeamAccess;
use crate::db;
use crate::reports::layout::{Layout, LogoSource, Paper, Scope, Sections, Template};
use crate::reports::schedule::{self, Frequency, Period, parse_timezone};
use crate::reports::store::{self, ScheduleInput, StoredSchedule};
use crate::reports::{self, MAX_SCHEDULES};
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/reports/schedules", get(list).post(create))
        .route("/reports/schedules/{id}", get(get_one).put(update).delete(remove))
        .route("/reports/schedules/{id}/send-test", post(send_test))
        .route("/reports/sla", get(download_sla))
        .route(
            "/reports/logo",
            get(report_logo).put(upload_report_logo).delete(delete_report_logo),
        )
}

#[derive(Debug, Serialize)]
pub struct ScheduleView {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub frequency: String,
    /// 0 = lundi.
    pub weekday: u32,
    pub day_of_month: u32,
    pub hour: u32,
    pub timezone: String,
    pub recipients: Vec<String>,
    pub channel_id: Option<i64>,
    pub last_sent_at: Option<String>,
    pub last_error: Option<String>,
    pub next_run_at: Option<String>,
    /// `digest` ou `sla`.
    pub kind: String,
    pub attach_pdf: bool,
    pub attach_csv: bool,
    pub page_id: Option<i64>,
    pub objective: Option<f64>,
    /// Mise en page du PDF SLA.
    pub layout: Layout,
    /// Périmètre du rapport SLA.
    pub scope: Scope,
}

fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn view(stored: &StoredSchedule, now: DateTime<Utc>) -> ScheduleView {
    ScheduleView {
        id: stored.id,
        name: stored.name.clone(),
        enabled: stored.enabled,
        frequency: stored.frequency.clone(),
        weekday: stored.weekday,
        day_of_month: stored.day_of_month,
        hour: stored.hour,
        timezone: stored.timezone.clone(),
        recipients: stored.recipients.clone(),
        channel_id: stored.channel_id,
        last_sent_at: stored.last_sent_at.map(iso),
        last_error: stored.last_error.clone(),
        next_run_at: reports::next_run(stored, now).map(iso),
        kind: stored.kind.clone(),
        attach_pdf: stored.attach_pdf,
        attach_csv: stored.attach_csv,
        page_id: stored.page_id,
        objective: stored.objective,
        layout: stored.layout.clone(),
        scope: stored.scope.clone(),
    }
}

/// Calendrier soumis. Les champs absents prennent la valeur par défaut :
/// hebdomadaire, le lundi à 8 h, en UTC.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SchedulePayload {
    pub name: String,
    pub enabled: bool,
    pub frequency: String,
    pub weekday: u32,
    pub day_of_month: u32,
    pub hour: u32,
    pub timezone: String,
    pub recipients: Vec<String>,
    pub channel_id: Option<i64>,
    /// `digest` (résumé glissant) ou `sla` (mois civil précédent, mensuel).
    pub kind: String,
    pub attach_pdf: bool,
    pub attach_csv: bool,
    pub page_id: Option<i64>,
    pub objective: Option<f64>,
    /// Mise en page du PDF SLA ; absente, le rapport détaillé en A4.
    pub layout: Layout,
    /// Périmètre ; absent, toute l'instance (ou tout le client).
    pub scope: Scope,
}

impl Default for SchedulePayload {
    fn default() -> Self {
        Self {
            name: "Team report".to_string(),
            enabled: true,
            frequency: "weekly".to_string(),
            weekday: 0,
            day_of_month: 1,
            hour: 8,
            timezone: "UTC".to_string(),
            recipients: Vec::new(),
            channel_id: None,
            kind: "digest".to_string(),
            attach_pdf: true,
            attach_csv: true,
            page_id: None,
            objective: None,
            layout: Layout::default(),
            scope: Scope::default(),
        }
    }
}

/// Objectif de disponibilité : un pourcentage entre 0 (exclu) et 100.
fn valid_objective(objective: Option<f64>) -> ApiResult<Option<f64>> {
    match objective {
        None => Ok(None),
        Some(value) if value.is_finite() && value > 0.0 && value <= 100.0 => Ok(Some(value)),
        Some(_) => Err(ApiError::BadRequest(
            "The availability objective must be a percentage above 0 and up to 100.".into(),
        )),
    }
}

impl SchedulePayload {
    async fn validate(self, state: &AppState) -> ApiResult<ScheduleInput> {
        let name = self.name.trim().to_string();
        if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
            return Err(ApiError::BadRequest(
                "The name must be 1 to 80 characters, without control characters.".into(),
            ));
        }
        let frequency = Frequency::parse(&self.frequency).ok_or_else(|| {
            ApiError::BadRequest(
                "The frequency must be \"daily\", \"weekly\" or \"monthly\".".into(),
            )
        })?;
        if self.weekday > 6 {
            return Err(ApiError::BadRequest(
                "The weekday must be between 0 (Monday) and 6 (Sunday).".into(),
            ));
        }
        if !(1..=28).contains(&self.day_of_month) {
            return Err(ApiError::BadRequest("The day of the month must be 1 to 28.".into()));
        }
        if self.hour > 23 {
            return Err(ApiError::BadRequest("The hour must be between 0 and 23.".into()));
        }
        let timezone = self.timezone.trim().to_string();
        if parse_timezone(&timezone).is_none() {
            return Err(ApiError::BadRequest(
                "The timezone must be an IANA name such as \"Europe/Paris\" or \"UTC\".".into(),
            ));
        }
        let recipients =
            reports::normalise_recipients(&self.recipients).map_err(ApiError::BadRequest)?;
        if self.enabled && recipients.is_empty() {
            return Err(ApiError::BadRequest(
                "Add at least one recipient before enabling the report.".into(),
            ));
        }
        if let Some(id) = self.channel_id {
            let channel = db::alerts::get_channel(&state.pool, &state.cipher, id).await?;
            if !channel.is_some_and(|channel| channel.kind == "smtp") {
                return Err(ApiError::BadRequest(
                    "The channel must be an existing email (SMTP) channel.".into(),
                ));
            }
        }
        let kind = self.kind.trim().to_string();
        if kind != "digest" && kind != "sla" {
            return Err(ApiError::BadRequest("The kind must be \"digest\" or \"sla\".".into()));
        }
        if kind == "sla" && frequency != Frequency::Monthly {
            return Err(ApiError::BadRequest(
                "An SLA report covers a calendar month: choose the monthly frequency.".into(),
            ));
        }
        if let Some(id) = self.page_id
            && db::status_pages::get_page(&state.pool, id).await?.is_none()
        {
            return Err(ApiError::BadRequest("The status page does not exist.".into()));
        }
        let objective = valid_objective(self.objective)?;
        let layout = self.layout.validate().map_err(ApiError::BadRequest)?;
        let scope = self.scope.validate().map_err(ApiError::BadRequest)?;
        Ok(ScheduleInput {
            name,
            enabled: self.enabled,
            frequency: frequency.as_str().to_string(),
            weekday: self.weekday,
            day_of_month: self.day_of_month,
            hour: self.hour,
            timezone,
            recipients,
            channel_id: self.channel_id,
            kind,
            attach_pdf: self.attach_pdf,
            attach_csv: self.attach_csv,
            page_id: self.page_id,
            objective,
            layout,
            scope,
        })
    }
}

fn not_found() -> ApiError {
    ApiError::NotFound("No such report schedule.".into())
}

pub async fn list(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    TeamAccess(access): TeamAccess,
) -> ApiResult<Json<Vec<ScheduleView>>> {
    let now = Utc::now();
    let mut schedules = store::list(&state.pool).await?;
    // Vue d'un client (mode MSP) : ses calendriers et ceux de l'instance.
    clients::retain_visible(
        &state.pool,
        &access,
        clients::Kind::Report,
        &mut schedules,
        |s| s.id,
        true,
    )
    .await?;
    Ok(Json(schedules.iter().map(|stored| view(stored, now)).collect()))
}

pub async fn get_one(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    Path(id): Path<i64>,
) -> ApiResult<Json<ScheduleView>> {
    let stored = store::get(&state.pool, id).await?.ok_or_else(not_found)?;
    Ok(Json(view(&stored, Utc::now())))
}

pub async fn create(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    TeamAccess(access): TeamAccess,
    Json(payload): Json<SchedulePayload>,
) -> ApiResult<(StatusCode, Json<ScheduleView>)> {
    let input = payload.validate(&state).await?;
    if store::count(&state.pool).await? >= MAX_SCHEDULES {
        return Err(ApiError::Conflict(format!(
            "At most {MAX_SCHEDULES} report schedules are allowed."
        )));
    }
    let now = Utc::now();
    let id = store::insert(&state.pool, &input, now).await?;
    // Créé dans la vue d'un client : il ne rapporte que sur ce client.
    clients::adopt(&state.pool, &access, clients::Kind::Report, id).await?;
    let stored = store::get(&state.pool, id).await?.ok_or_else(not_found)?;
    Ok((StatusCode::CREATED, Json(view(&stored, now))))
}

pub async fn update(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    Path(id): Path<i64>,
    Json(payload): Json<SchedulePayload>,
) -> ApiResult<Json<ScheduleView>> {
    let current = store::get(&state.pool, id).await?.ok_or_else(not_found)?;
    let input = payload.validate(&state).await?;
    let now = Utc::now();
    store::update(&state.pool, &current, &input, now).await?;
    let stored = store::get(&state.pool, id).await?.ok_or_else(not_found)?;
    Ok(Json(view(&stored, now)))
}

pub async fn remove(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if store::delete(&state.pool, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

#[derive(Debug, Serialize)]
pub struct SendTestReply {
    pub sent: usize,
    pub failed: usize,
}

/// Envoie tout de suite un aperçu du rapport aux destinataires enregistrés.
/// N'affecte pas le calendrier : ni dernier envoi, ni créneau consommé.
pub async fn send_test(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    Path(id): Path<i64>,
) -> ApiResult<Json<SendTestReply>> {
    let stored = store::get(&state.pool, id).await?.ok_or_else(not_found)?;
    if !reports::try_reserve_preview(id) {
        return Err(ApiError::Conflict(
            "A preview was just sent. Wait half a minute before sending another.".into(),
        ));
    }
    match reports::deliver(&state, &stored, true, Utc::now()).await {
        Ok(delivery) => Ok(Json(SendTestReply { sent: delivery.sent, failed: delivery.failed })),
        Err(message) => Err(ApiError::BadRequest(message)),
    }
}

/// `GET /api/reports/logo` : le logo téléversé pour les rapports SLA. Lisible
/// par tout compte connecté, comme l'aperçu du rapport qui l'affiche.
pub async fn report_logo(State(state): State<AppState>, _who: Identity) -> ApiResult<Response> {
    let not_found = || ApiError::NotFound("No logo has been uploaded for reports.".into());
    let (kind, bytes) = reports::logo::stored(&state.pool).await?.ok_or_else(not_found)?;
    branding::serve_bytes(bytes, &kind, "private, no-cache")
}

/// `PUT /api/reports/logo` : même dépôt que le logo d'une page de statut
/// (base64 ou `data:`, 256 Kio au plus, type reconnu à sa signature), limité
/// aux images que le PDF sait incorporer : PNG 8 bits non entrelacé ou JPEG.
pub async fn upload_report_logo(
    State(state): State<AppState>,
    _admin: AdminIdentity,
    Json(payload): Json<branding::LogoPayload>,
) -> ApiResult<StatusCode> {
    let bytes = branding::decode(&payload.data)?;
    if bytes.is_empty() {
        return Err(ApiError::BadRequest("The logo file is empty.".into()));
    }
    if bytes.len() > branding::MAX_LOGO_BYTES {
        return Err(ApiError::BadRequest(format!(
            "The logo is limited to {} KiB.",
            branding::MAX_LOGO_BYTES / 1024
        )));
    }
    // `for_pdf` ne lit que le PNG et le JPEG : le WebP est refusé ici.
    let embeddable = reports::logo::for_pdf(&bytes).is_some();
    let Some(kind) = branding::sniff(&bytes).filter(|_| embeddable) else {
        return Err(ApiError::BadRequest(
            "The report logo must be a PNG (8-bit, not interlaced) or a JPEG image.".into(),
        ));
    };
    reports::logo::set_stored(&state.pool, Some((kind, bytes.as_slice()))).await?;
    tracing::info!(kind, size = bytes.len(), "report logo stored");
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/reports/logo`
pub async fn delete_report_logo(
    State(state): State<AppState>,
    _admin: AdminIdentity,
) -> ApiResult<StatusCode> {
    reports::logo::set_stored(&state.pool, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Paramètres de `GET /api/reports/sla`.
#[derive(Debug, Deserialize)]
pub struct SlaQuery {
    /// Premier jour inclus, `AAAA-MM-JJ`. Absent : le mois civil précédent.
    pub from: Option<String>,
    /// Dernier jour inclus. Absent avec `from` : aujourd'hui.
    pub to: Option<String>,
    /// Fuseau IANA de la période et de l'affichage. Défaut : UTC.
    pub tz: Option<String>,
    /// `pdf` (défaut), `csv` (moniteurs) ou `incidents_csv`.
    pub format: Option<String>,
    /// Objectif de disponibilité, en pour cent.
    pub objective: Option<f64>,
    /// Page de statut dont le nom habille le rapport.
    pub page: Option<i64>,
    /// Titre du rapport (nom affiché sous l'en-tête).
    pub title: Option<String>,
    /// Mode MSP : le client dont on veut le rapport (ses équipements seuls, son
    /// nom et son logo). Obligatoire pour un membre de plusieurs clients.
    pub client: Option<i64>,
    /// `detailed` (défaut) ou `summary`.
    pub template: Option<String>,
    /// `a4` (défaut) ou `letter`.
    pub paper: Option<String>,
    /// Sections incluses, séparées par des virgules ; absent : toutes.
    pub sections: Option<String>,
    /// Notes libres, imprimées dans la section « Notes ».
    pub notes: Option<String>,
    /// Origine du logo : `auto` (défaut), `client`, `page`, `custom` ou
    /// `none` ; `true` et `false` (anciennes adresses) valent `auto` et `none`.
    pub logo: Option<String>,
    /// Périmètre en JSON : `{"targets":[1,2],"groups":["Edge"]}`.
    pub scope: Option<String>,
}

impl SlaQuery {
    /// Mise en page et périmètre demandés, validés.
    fn layout(&self, title: Option<&str>) -> ApiResult<(Layout, Scope)> {
        let template = match self.template.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            None => Template::default(),
            Some(raw) => Template::parse(raw).ok_or_else(|| {
                ApiError::BadRequest("The template must be \"detailed\" or \"summary\".".into())
            })?,
        };
        let paper = match self.paper.as_deref().map(str::trim).filter(|p| !p.is_empty()) {
            None => Paper::default(),
            Some(raw) => Paper::parse(raw).ok_or_else(|| {
                ApiError::BadRequest("The paper must be \"a4\" or \"letter\".".into())
            })?,
        };
        let sections = Sections::parse_list(self.sections.as_deref().unwrap_or(""))
            .map_err(ApiError::BadRequest)?;
        let logo = LogoSource::parse(self.logo.as_deref().unwrap_or("")).ok_or_else(|| {
            ApiError::BadRequest(
                "The logo must be \"auto\", \"client\", \"page\", \"custom\" or \"none\".".into(),
            )
        })?;
        let layout = Layout {
            template,
            paper,
            sections,
            title: title.map(str::to_string),
            notes: self.notes.clone().unwrap_or_default(),
            show_logo: true,
            logo,
        }
        .validate()
        .map_err(ApiError::BadRequest)?;
        let scope = match self.scope.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            None => Scope::default(),
            Some(raw) => serde_json::from_str::<Scope>(raw).map_err(|_| {
                ApiError::BadRequest(
                    "The scope must be JSON like {\"targets\":[1],\"groups\":[\"Edge\"]}.".into(),
                )
            })?,
        }
        .validate()
        .map_err(ApiError::BadRequest)?;
        Ok((layout, scope))
    }
}

/// Plus longue période demandable : une année (bissextile comprise).
const MAX_SLA_DAYS: i64 = 366;

fn parse_day(label: &str, raw: &str) -> ApiResult<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
        .map_err(|_| ApiError::BadRequest(format!("\"{label}\" must be a date like 2026-10-31.")))
}

/// `GET /api/reports/sla` : le rapport de disponibilité d'une période choisie,
/// à la demande, en PDF ou en CSV.
pub async fn download_sla(
    State(state): State<AppState>,
    Identity(who): Identity,
    TeamAccess(access): TeamAccess,
    Query(query): Query<SlaQuery>,
) -> ApiResult<Response> {
    use crate::reports::{collect, sla};
    // Mode MSP : un membre d'un client lit le rapport de ses clients, et de
    // rien d'autre ; hors de ce cas, le rapport reste à l'administrateur.
    let client = match (access.clients(), query.client) {
        // L'administrateur MSP, même dans la vue d'un client, lit tout client.
        (_, Some(client)) if who.is_admin() => {
            if !clients::exists(&state.pool, client).await? {
                return Err(ApiError::NotFound("Client not found.".into()));
            }
            Some(client)
        }
        (Some(allowed), Some(client)) if allowed.contains(&client) => Some(client),
        (Some(_), Some(_)) => return Err(ApiError::NotFound("Client not found.".into())),
        (Some([only]), None) => Some(*only),
        (Some(_), None) => {
            return Err(ApiError::BadRequest("Choose the client of the report.".into()));
        }
        (None, client) => {
            if !who.is_admin() {
                return Err(ApiError::Forbidden(
                    "The availability report is reserved to administrators.".into(),
                ));
            }
            if let Some(client) = client
                && !clients::exists(&state.pool, client).await?
            {
                return Err(ApiError::NotFound("Client not found.".into()));
            }
            client
        }
    };
    if query.page.is_some() && !who.is_admin() {
        return Err(ApiError::Forbidden("Only an administrator can brand by status page.".into()));
    }
    let tz_name = query.tz.as_deref().map(str::trim).filter(|t| !t.is_empty()).unwrap_or("UTC");
    let tz = parse_timezone(tz_name).ok_or_else(|| {
        ApiError::BadRequest("The timezone must be an IANA name such as \"Europe/Paris\".".into())
    })?;
    let now = Utc::now();
    let period = match (query.from.as_deref(), query.to.as_deref()) {
        (None, None) => Period::previous_calendar_month(now, tz)
            .ok_or_else(|| ApiError::BadRequest("Cannot work out the month.".into()))?,
        (from, to) => {
            let today = now.with_timezone(&tz).date_naive();
            let first = match from {
                Some(raw) => parse_day("from", raw)?,
                None => {
                    return Err(ApiError::BadRequest("Give \"from\" together with \"to\".".into()));
                }
            };
            let last = match to {
                Some(raw) => parse_day("to", raw)?,
                None => today,
            };
            if last < first {
                return Err(ApiError::BadRequest("\"to\" must not be before \"from\".".into()));
            }
            if (last - first).num_days() + 1 > MAX_SLA_DAYS {
                return Err(ApiError::BadRequest(format!(
                    "A report covers at most {MAX_SLA_DAYS} days."
                )));
            }
            let start = schedule::local_midnight(first, tz)
                .ok_or_else(|| ApiError::BadRequest("\"from\" is not a valid date.".into()))?;
            let end = schedule::local_midnight(last.succ_opt().unwrap_or(last), tz)
                .ok_or_else(|| ApiError::BadRequest("\"to\" is not a valid date.".into()))?;
            if start >= now {
                return Err(ApiError::BadRequest("The period starts in the future.".into()));
            }
            // Une période en cours s'arrête à maintenant.
            Period::custom(start, end.min(now))
        }
    };

    let title = query.title.as_deref().map(str::trim).filter(|t| !t.is_empty());
    let name = title.unwrap_or("Availability report");
    if name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err(ApiError::BadRequest(
            "The title must be 120 characters at most, without control characters.".into(),
        ));
    }
    let objective = valid_objective(query.objective)?;
    if let Some(id) = query.page
        && db::status_pages::get_page(&state.pool, id).await?.is_none()
    {
        return Err(ApiError::BadRequest("The status page does not exist.".into()));
    }
    let (layout, scope) = query.layout(title)?;
    let format = query.format.as_deref().unwrap_or("pdf");
    if !["pdf", "csv", "incidents_csv", "json"].contains(&format) {
        return Err(ApiError::BadRequest(format!(
            "Unknown format \"{}\" (expected: pdf, csv, incidents_csv, json).",
            format.chars().filter(|c| !c.is_control()).take(40).collect::<String>()
        )));
    }

    let data =
        collect::collect_scoped(&state, name, Frequency::Monthly, period, now, client, &scope)
            .await
            .map_err(|error| ApiError::Internal(error.context("collecte du rapport SLA")))?;
    let options = reports::sla_options(&state, query.page, client, objective, layout, now).await;
    if format == "json" {
        return Ok(Json(sla_view(&data, tz, tz_name, &options)).into_response());
    }
    let stem = sla::file_stem(&data, tz);
    let (body, content_type, filename) = match format {
        "csv" => (
            sla::csv_monitors(&data, objective).into_bytes(),
            reports::CSV_TYPE,
            format!("{stem}-monitors.csv"),
        ),
        "incidents_csv" => (
            sla::csv_incidents(&data).into_bytes(),
            reports::CSV_TYPE,
            format!("{stem}-incidents.csv"),
        ),
        _ => (sla::pdf(&data, tz, &options), "application/pdf", format!("{stem}.pdf")),
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
            (header::CACHE_CONTROL, "private, no-store".to_string()),
        ],
        body,
    )
        .into_response())
}

// --------------------------------------------------------------------------
// Vue JSON du rapport SLA (`format=json`) : la page web imprimable
// --------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SlaReportView {
    /// Organisation, page de statut ou « DumbMonit ».
    pub brand: String,
    /// Titre choisi, sinon « Availability report ».
    pub title: String,
    pub period_label: String,
    pub period_start: String,
    pub period_end: String,
    pub timezone: String,
    pub generated_at: String,
    pub objective: Option<f64>,
    pub layout: Layout,
    /// Adresse du logo en tête (relative à `/api`), `None` sans logo.
    pub logo_url: Option<String>,
    pub availability_known: bool,
    pub fleet_uptime: Option<f64>,
    pub fleet_previous_uptime: Option<f64>,
    /// `Met`, `Missed`, `Healthy`, `Degraded`, `Poor` ou `No data`.
    pub fleet_status: String,
    pub downtime_secs: i64,
    pub incident_count: usize,
    pub previous_incident_count: usize,
    /// Vrai quand les moniteurs sont rangés en groupes.
    pub grouped: bool,
    pub groups: Vec<SlaGroupView>,
    pub monitors: Vec<SlaMonitorView>,
    pub incidents: Vec<SlaIncidentView>,
    pub maintenance: Vec<SlaMaintenanceView>,
}

#[derive(Debug, Serialize)]
pub struct SlaGroupView {
    pub name: String,
    pub monitors: usize,
    pub uptime: Option<f64>,
    pub previous_uptime: Option<f64>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct SlaMonitorView {
    pub name: String,
    pub group: String,
    pub uptime: Option<f64>,
    pub previous_uptime: Option<f64>,
    pub status: String,
    pub incidents: usize,
    pub downtime_secs: i64,
    pub latency_avg_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct SlaIncidentView {
    pub monitor: String,
    pub rule: String,
    pub severity: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_secs: i64,
    pub began_before_period: bool,
    /// Parmi les incidents majeurs de la synthèse.
    pub major: bool,
}

#[derive(Debug, Serialize)]
pub struct SlaMaintenanceView {
    pub name: String,
    pub monitor: Option<String>,
    pub starts_at: String,
    pub ends_at: String,
    pub recurring: bool,
}

fn sla_view(
    data: &crate::reports::collect::ReportData,
    tz: chrono_tz::Tz,
    tz_name: &str,
    options: &crate::reports::sla::Options,
) -> SlaReportView {
    use crate::reports::collect::UNGROUPED;
    use crate::reports::sla;
    let objective = options.objective;
    let major = sla::major_incidents(data);
    let grouped = data.groups.len() > 1 || data.groups.first().is_some_and(|g| g.name != UNGROUPED);
    SlaReportView {
        brand: options.brand.clone(),
        title: sla::heading_title(&options.layout),
        period_label: sla::period_label(data, tz),
        period_start: iso(data.period.start),
        period_end: iso(data.period.end),
        timezone: tz_name.to_string(),
        generated_at: iso(options.generated),
        objective,
        layout: options.layout.clone(),
        logo_url: options.logo_url.clone(),
        availability_known: data.availability_known,
        fleet_uptime: data.fleet_uptime,
        fleet_previous_uptime: data.fleet_previous_uptime,
        fleet_status: sla::status_word(data.fleet_uptime, objective).to_string(),
        downtime_secs: data.downtime_secs,
        incident_count: data.incidents.len(),
        previous_incident_count: data.previous_incident_count,
        grouped,
        groups: data
            .groups
            .iter()
            .map(|g| SlaGroupView {
                name: g.name.clone(),
                monitors: g.devices,
                uptime: g.uptime,
                previous_uptime: g.previous_uptime,
                status: sla::status_word(g.uptime, objective).to_string(),
            })
            .collect(),
        monitors: data
            .devices
            .iter()
            .take(sla::MAX_PDF_MONITORS)
            .map(|d| SlaMonitorView {
                name: d.name.clone(),
                group: d.group.clone(),
                uptime: d.uptime,
                previous_uptime: d.previous_uptime,
                status: sla::status_word(d.uptime, objective).to_string(),
                incidents: d.incidents,
                downtime_secs: d.downtime_secs,
                latency_avg_ms: d.latency_avg_ms,
                latency_p95_ms: d.latency_p95_ms,
            })
            .collect(),
        incidents: data
            .incidents
            .iter()
            .take(sla::MAX_PDF_INCIDENTS)
            .map(|i| SlaIncidentView {
                monitor: i.device.clone(),
                rule: i.rule.clone(),
                severity: i.severity.clone(),
                started_at: iso(i.started),
                ended_at: i.ended.map(iso),
                duration_secs: i.duration(data.period.end).num_seconds().max(0),
                began_before_period: i.began_before_period,
                major: major.iter().any(|m| std::ptr::eq(*m, i)),
            })
            .collect(),
        maintenance: data
            .maintenance
            .iter()
            .map(|w| SlaMaintenanceView {
                name: w.name.clone(),
                monitor: w.monitor.clone(),
                starts_at: iso(w.starts),
                ends_at: iso(w.ends),
                recurring: w.recurring,
            })
            .collect(),
    }
}
