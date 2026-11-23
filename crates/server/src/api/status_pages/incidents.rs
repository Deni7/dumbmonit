//! Incidents, maintenances et leurs mises à jour.

use super::*;

// --------------------------------------------------------------------------
// Incidents
// --------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct IncidentPayload {
    pub title: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub page_id: Option<i64>,
    #[serde(default)]
    pub starts_at: Option<String>,
    #[serde(default)]
    pub ends_at: Option<String>,
    /// Premier message du fil, à la création seulement.
    #[serde(default)]
    pub body: Option<String>,
    /// Messages datés du fil, à la création seulement : reprise d'un historique.
    #[serde(default)]
    pub updates: Option<Vec<NewUpdate>>,
    /// Équipements touchés, parmi les services de la page : les abonnés qui ne
    /// suivent que d'autres services ne sont pas prévenus. Absent : inchangé
    /// (toute la page à la création).
    #[serde(default)]
    pub components: Option<Vec<i64>>,
}

/// Un message d'un fil saisi à la création d'une annonce.
#[derive(Debug, Deserialize)]
pub struct NewUpdate {
    pub status: String,
    pub body: String,
    /// Date du message ; absent : celle du message précédent (le début pour le
    /// premier).
    #[serde(default)]
    pub at: Option<String>,
}

/// Tolérance sur l'horloge du navigateur : un incident « maintenant » saisi
/// avec quelques secondes d'avance n'est pas un incident futur.
pub(super) const FUTURE_SKEW_MINUTES: i64 = 5;

/// Statuts admis pour un genre d'annonce, et celui par défaut.
pub(super) fn statuses_for(kind: &str) -> (&'static [&'static str], &'static str) {
    if kind == "maintenance" {
        (&MAINTENANCE_STATUSES, "scheduled")
    } else {
        (&INCIDENT_STATUSES, "investigating")
    }
}

/// Un statut de fin : l'annonce quitte la liste des annonces actives.
pub(super) fn is_closing(status: &str) -> bool {
    matches!(status, "resolved" | "completed")
}

/// Horodatage saisi par l'utilisateur (RFC 3339, ou le format de la base),
/// normalisé au format de la base.
pub(super) fn parse_timestamp(raw: &str, field: &str) -> ApiResult<DateTime<Utc>> {
    let raw = raw.trim();
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Ok(parsed.with_timezone(&Utc));
    }
    // `2026-09-15T10:00` (saisie d'un champ datetime-local, sans fuseau) est lu
    // comme du temps universel.
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Ok(naive.and_utc());
        }
    }
    Err(ApiError::BadRequest(format!(
        "{field} must be a date and time (for example 2026-09-15T10:00:00Z)."
    )))
}

pub(super) fn format_timestamp(at: DateTime<Utc>) -> String {
    at.format(TIMESTAMP_FORMAT).to_string()
}

pub(crate) fn parse_stored(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw).map(|at| at.with_timezone(&Utc)).ok().or_else(|| {
        chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|naive| naive.and_utc())
    })
}

impl IncidentPayload {
    /// Valide la saisie. `existing` porte le genre d'une annonce déjà créée, que
    /// l'on ne change pas en cours de route.
    pub(super) async fn validate(
        self,
        state: &AppState,
        existing: Option<&Incident>,
    ) -> ApiResult<IncidentInput> {
        let title = self.title.trim().to_string();
        if title.is_empty() {
            return Err(ApiError::BadRequest("Title is required.".into()));
        }
        if title.chars().count() > MAX_INCIDENT_TITLE_LEN {
            return Err(ApiError::BadRequest(format!(
                "Title is limited to {MAX_INCIDENT_TITLE_LEN} characters."
            )));
        }
        let kind = match existing {
            Some(incident) => incident.kind.clone(),
            None => self.kind.unwrap_or_else(|| "incident".into()),
        };
        if kind != "incident" && kind != "maintenance" {
            return Err(ApiError::BadRequest(format!(
                "Unknown kind \"{kind}\" (expected: incident, maintenance)."
            )));
        }
        let (allowed, default_status) = statuses_for(&kind);
        let status = match self.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(status) => status.to_string(),
            None => match existing {
                Some(incident) => incident.status.clone(),
                None => default_status.to_string(),
            },
        };
        if !allowed.contains(&status.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Status \"{status}\" is not valid for a {kind} (expected: {}).",
                allowed.join(", ")
            )));
        }
        let severity = match self.severity.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(severity) => severity.to_string(),
            None => match existing {
                Some(incident) => incident.severity.clone(),
                None => "minor".to_string(),
            },
        };
        if !SEVERITIES.contains(&severity.as_str()) {
            return Err(ApiError::BadRequest(format!(
                "Unknown severity \"{severity}\" (expected: minor, major)."
            )));
        }
        if let Some(page_id) = self.page_id
            && db::status_pages::get_page(&state.pool, page_id).await?.is_none()
        {
            return Err(page_not_found(page_id));
        }
        let starts_at = match self.starts_at.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => parse_timestamp(raw, "Start")?,
            None => match existing.and_then(|incident| parse_stored(&incident.starts_at)) {
                Some(at) => at,
                None => Utc::now(),
            },
        };
        let ends_at = match self.ends_at.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => Some(parse_timestamp(raw, "End")?),
            None => None,
        };
        if let Some(ends_at) = ends_at
            && ends_at < starts_at
        {
            return Err(ApiError::BadRequest("End cannot come before start.".into()));
        }
        // Un incident se saisit après coup : jamais dans le futur (une maintenance,
        // elle, se planifie). Contrôlé à la création ou quand la date change.
        if kind == "incident" {
            let limit = Utc::now() + TimeDelta::minutes(FUTURE_SKEW_MINUTES);
            let start_text = format_timestamp(starts_at);
            let start_moved = existing.is_none_or(|e| e.starts_at != start_text);
            if starts_at > limit && start_moved {
                return Err(ApiError::BadRequest(
                    "An incident cannot start in the future: schedule a maintenance instead."
                        .into(),
                ));
            }
            let end_text = ends_at.map(format_timestamp);
            let end_moved = existing.is_none_or(|e| e.ends_at != end_text);
            if ends_at.is_some_and(|end| end > limit) && end_moved {
                return Err(ApiError::BadRequest("An incident cannot end in the future.".into()));
            }
        }
        if kind == "maintenance" && ends_at.is_none() && !is_closing(&status) {
            return Err(ApiError::BadRequest("A maintenance window needs an end.".into()));
        }
        if let Some(body) = &self.body
            && body.chars().count() > MAX_BODY_LEN
        {
            return Err(ApiError::BadRequest(format!(
                "Messages are limited to {MAX_BODY_LEN} characters."
            )));
        }
        Ok(IncidentInput {
            page_id: self.page_id,
            title,
            kind,
            status,
            severity,
            starts_at: format_timestamp(starts_at),
            ends_at: ends_at.map(format_timestamp),
        })
    }
}

pub(super) async fn incident_views(
    state: &AppState,
    incidents: Vec<Incident>,
) -> ApiResult<Vec<IncidentView>> {
    let ids: Vec<i64> = incidents.iter().map(|incident| incident.id).collect();
    let mut updates: HashMap<i64, Vec<IncidentUpdate>> = HashMap::new();
    for update in db::status_pages::list_updates_for(&state.pool, &ids).await? {
        updates.entry(update.incident_id).or_default().push(update);
    }
    let mut components: HashMap<i64, Vec<i64>> = HashMap::new();
    for (incident, target) in db::status_pages::incident_component_rows(&state.pool).await? {
        components.entry(incident).or_default().push(target);
    }
    let mut postmortems = db::status_pages::postmortems_for(&state.pool, &ids).await?;
    Ok(incidents
        .into_iter()
        .map(|incident| {
            let updates = updates.remove(&incident.id).unwrap_or_default();
            let components = components.remove(&incident.id).unwrap_or_default();
            let postmortem = postmortems.remove(&incident.id);
            IncidentView { incident, updates, components, postmortem }
        })
        .collect())
}

pub async fn list_incidents(State(state): State<AppState>) -> ApiResult<Json<Vec<IncidentView>>> {
    let incidents = db::status_pages::list_incidents(&state.pool).await?;
    Ok(Json(incident_views(&state, incidents).await?))
}

/// Fil d'une annonce qu'on crée : messages datés (ou le seul premier message),
/// triés dans le temps et contrôlés. Rend le fil, le statut final et la fin.
pub(super) struct PlannedThread {
    pub(super) entries: Vec<db::status_pages::ThreadEntry>,
    pub(super) status: String,
    pub(super) ends_at: Option<DateTime<Utc>>,
}

pub(super) fn plan_thread(
    input: &IncidentInput,
    first_body: &str,
    drafts: Vec<NewUpdate>,
    now: DateTime<Utc>,
) -> ApiResult<PlannedThread> {
    let bad = |message: String| Err(ApiError::BadRequest(message));
    let starts = parse_stored(&input.starts_at).unwrap_or(now);
    let ends = input.ends_at.as_deref().and_then(parse_stored);
    let (allowed, default_status) = statuses_for(&input.kind);
    if drafts.len() > MAX_THREAD {
        return bad(format!("An announcement takes at most {MAX_THREAD} updates."));
    }

    // (date, statut, message), dans l'ordre de saisie d'abord.
    let mut raw: Vec<(DateTime<Utc>, String, String)> = Vec::new();
    let first_status = if drafts.is_empty() { input.status.as_str() } else { default_status };
    if !first_body.is_empty() {
        raw.push((starts.min(now), first_status.to_string(), first_body.to_string()));
    }
    let mut previous = starts;
    for draft in drafts {
        let status = draft.status.trim().to_string();
        if !allowed.contains(&status.as_str()) {
            return bad(format!(
                "Status \"{status}\" is not valid for a {} (expected: {}).",
                input.kind,
                allowed.join(", ")
            ));
        }
        let body = draft.body.trim().to_string();
        if body.is_empty() {
            return bad("Every update needs a message.".into());
        }
        if body.chars().count() > MAX_BODY_LEN {
            return bad(format!("Messages are limited to {MAX_BODY_LEN} characters."));
        }
        let at = match draft.at.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(text) => parse_timestamp(text, "Update date")?,
            None => previous,
        };
        previous = at;
        raw.push((at, status, body));
    }
    // Chronologique ; à égalité, l'ordre de saisie est gardé (tri stable).
    raw.sort_by_key(|entry| entry.0);

    let limit = now + TimeDelta::minutes(FUTURE_SKEW_MINUTES);
    // Une maintenance s'annonce avant son début et se clôt après sa fin prévue :
    // seuls les incidents ont leur fil enfermé entre leur début et leur fin.
    if input.kind == "incident" {
        for (at, _, _) in &raw {
            if *at < starts || ends.is_some_and(|end| *at > end) {
                return bad("Updates must fall between the start and the end.".into());
            }
            if *at > limit {
                return bad("An update cannot be dated in the future.".into());
            }
        }
    }
    if let Some(position) = raw.iter().position(|(_, status, _)| is_closing(status))
        && position + 1 < raw.len()
    {
        return bad("No update can follow the resolution.".into());
    }

    let mut status = raw.last().map_or_else(|| input.status.clone(), |entry| entry.1.clone());
    let mut ends_at = ends;
    if is_closing(&status) && ends_at.is_none() {
        ends_at = Some(raw.last().map_or(now.max(starts), |entry| entry.0));
    }
    // Une fin saisie pour un incident dit qu'il est résolu.
    if input.kind == "incident" && ends_at.is_some() && !is_closing(&status) {
        status = "resolved".to_string();
    }
    Ok(PlannedThread {
        entries: raw
            .into_iter()
            .map(|(at, status, body)| db::status_pages::ThreadEntry {
                status,
                body,
                at: format_timestamp(at),
            })
            .collect(),
        status,
        ends_at,
    })
}

pub async fn create_incident(
    State(state): State<AppState>,
    Json(mut payload): Json<IncidentPayload>,
) -> ApiResult<(StatusCode, Json<IncidentView>)> {
    let body = payload.body.clone().unwrap_or_default().trim().to_string();
    let drafts = payload.updates.take().unwrap_or_default();
    let components = payload.components.take();
    let mut input = payload.validate(&state, None).await?;
    // Le premier message ouvre le fil avec le statut initial ; sans message,
    // le fil reste vide et l'annonce se résume à son titre.
    let plan = plan_thread(&input, &body, drafts, Utc::now())?;
    input.status = plan.status;
    input.ends_at = plan.ends_at.map(format_timestamp);
    let id =
        db::status_pages::create_incident_with_thread(&state.pool, &input, &plan.entries).await?;
    if let Some(components) = components {
        db::status_pages::set_incident_components(&state.pool, id, &components).await?;
    }
    invalidate_cache();
    let incident = db::status_pages::get_incident(&state.pool, id)
        .await?
        .ok_or_else(|| incident_not_found(id))?;
    tracing::info!(incident = id, kind = %incident.kind, "incident created");
    // Un incident saisi déjà résolu est de l'histoire : personne n'est prévenu.
    if !is_closing(&incident.status) {
        subscribers::announce(&state, incident.clone(), Some(body));
    }
    let mut views = incident_views(&state, vec![incident]).await?;
    Ok((StatusCode::CREATED, Json(views.remove(0))))
}

pub async fn update_incident(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(mut payload): Json<IncidentPayload>,
) -> ApiResult<Json<IncidentView>> {
    let components = payload.components.take();
    let existing = db::status_pages::get_incident(&state.pool, id)
        .await?
        .ok_or_else(|| incident_not_found(id))?;
    let mut input = payload.validate(&state, Some(&existing)).await?;
    // Une fin explicite est conservée ; passer à un statut de fin sans en donner
    // fixe la fin à maintenant, comme le ferait un message de résolution.
    if input.ends_at.is_none() {
        input.ends_at = if is_closing(&input.status) {
            existing.ends_at.clone().or_else(|| Some(format_timestamp(Utc::now())))
        } else {
            existing.ends_at.clone()
        };
    }
    // Les dates déplacées ne doivent pas laisser un message hors de l'annonce.
    if input.starts_at != existing.starts_at || input.ends_at != existing.ends_at {
        let start = parse_stored(&input.starts_at);
        let end = input.ends_at.as_deref().and_then(parse_stored);
        for update in db::status_pages::list_updates(&state.pool, id).await? {
            let at = parse_stored(&update.created_at);
            if at < start || (end.is_some() && at > end) {
                return Err(ApiError::BadRequest(
                    "Updates must fall between the start and the end: move or delete them first."
                        .into(),
                ));
            }
        }
    }
    if !db::status_pages::update_incident(&state.pool, id, &input).await? {
        return Err(incident_not_found(id));
    }
    if let Some(components) = components {
        db::status_pages::set_incident_components(&state.pool, id, &components).await?;
    }
    invalidate_cache();
    let incident = db::status_pages::get_incident(&state.pool, id)
        .await?
        .ok_or_else(|| incident_not_found(id))?;
    // Un changement de statut sans message se dit aussi aux abonnés.
    if incident.status != existing.status {
        subscribers::announce(&state, incident.clone(), None);
    }
    let mut views = incident_views(&state, vec![incident]).await?;
    Ok(Json(views.remove(0)))
}

pub async fn delete_incident(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    if db::status_pages::delete_incident(&state.pool, id).await? {
        invalidate_cache();
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(incident_not_found(id))
    }
}

pub async fn list_updates(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<IncidentUpdate>>> {
    db::status_pages::get_incident(&state.pool, id).await?.ok_or_else(|| incident_not_found(id))?;
    Ok(Json(db::status_pages::list_updates(&state.pool, id).await?))
}

#[derive(Debug, Deserialize)]
pub struct UpdatePayload {
    #[serde(default)]
    pub status: Option<String>,
    pub body: String,
    /// Date du message, pour compléter un historique ; absent : maintenant, et
    /// les abonnés sont prévenus. Daté, le message est de l'histoire : il ne
    /// part à personne.
    #[serde(default)]
    pub at: Option<String>,
    /// `false` : le message est publié sans prévenir les abonnés. Absent : ils
    /// le sont (sauf message daté).
    #[serde(default)]
    pub notify: Option<bool>,
}

/// Poste un message et fait avancer le statut de l'annonce.
pub async fn add_update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(payload): Json<UpdatePayload>,
) -> ApiResult<(StatusCode, Json<IncidentView>)> {
    let incident = db::status_pages::get_incident(&state.pool, id)
        .await?
        .ok_or_else(|| incident_not_found(id))?;
    let body = payload.body.trim().to_string();
    if body.is_empty() {
        return Err(ApiError::BadRequest("Write a message.".into()));
    }
    if body.chars().count() > MAX_BODY_LEN {
        return Err(ApiError::BadRequest(format!(
            "Messages are limited to {MAX_BODY_LEN} characters."
        )));
    }
    let (allowed, _) = statuses_for(&incident.kind);
    let status = match payload.status.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(status) => status.to_string(),
        None => incident.status.clone(),
    };
    if !allowed.contains(&status.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "Status \"{status}\" is not valid for a {} (expected: {}).",
            incident.kind,
            allowed.join(", ")
        )));
    }
    let at = match payload.at.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(text) => {
            let at = parse_timestamp(text, "Update date")?;
            let starts = parse_stored(&incident.starts_at);
            let ends = incident.ends_at.as_deref().and_then(parse_stored);
            if starts.is_some_and(|start| at < start) || ends.is_some_and(|end| at > end) {
                return Err(ApiError::BadRequest(
                    "Updates must fall between the start and the end.".into(),
                ));
            }
            if incident.kind == "incident"
                && at > Utc::now() + TimeDelta::minutes(FUTURE_SKEW_MINUTES)
            {
                return Err(ApiError::BadRequest(
                    "An update cannot be dated in the future.".into(),
                ));
            }
            Some(format_timestamp(at))
        }
        None => None,
    };
    db::status_pages::add_update_at(
        &state.pool,
        id,
        &status,
        &body,
        is_closing(&status),
        at.as_deref(),
    )
    .await?;
    invalidate_cache();
    let incident = db::status_pages::get_incident(&state.pool, id)
        .await?
        .ok_or_else(|| incident_not_found(id))?;
    if at.is_none() && payload.notify.unwrap_or(true) {
        subscribers::announce(&state, incident.clone(), Some(body));
    }
    let mut views = incident_views(&state, vec![incident]).await?;
    Ok((StatusCode::CREATED, Json(views.remove(0))))
}
