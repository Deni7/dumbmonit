//! Pages de statut et incidents.

use super::*;

fn is_closed(status: &str) -> bool {
    matches!(status, "resolved" | "completed")
}

/// Dernier message du fil d'un incident.
pub(super) fn latest_update(view: &status_pages::IncidentView) -> Option<String> {
    view.updates.iter().max_by_key(|u| (u.created_at.clone(), u.id)).map(|u| u.body.clone())
}

pub(super) async fn list_status_pages(state: &AppState) -> ToolResult {
    let Json(pages) = status_pages::list_pages(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let Json(incidents) = status_pages::list_incidents(State(state.clone())).await?;

    let mut lines = Vec::new();
    if pages.is_empty() {
        lines.push("No status page.".to_string());
    } else {
        lines.push(format!("{}:", plural(pages.len(), "status page", "status pages")));
        for view in &pages {
            lines.push(format!(
                "- **{}** (#{}, /s/{}) — {}, {}",
                view.page.title,
                view.page.id,
                view.page.slug,
                if view.page.published { "published" } else { "draft" },
                plural(view.items.len(), "device", "devices")
            ));
        }
    }

    // Les incidents ouverts d'abord, puis les dix plus récents des autres.
    let mut ordered: Vec<&status_pages::IncidentView> = incidents.iter().collect();
    ordered.sort_by(|a, b| {
        is_closed(&a.incident.status)
            .cmp(&is_closed(&b.incident.status))
            .then_with(|| b.incident.updated_at.cmp(&a.incident.updated_at))
    });
    let open = ordered.iter().filter(|v| !is_closed(&v.incident.status)).count();
    let shown: Vec<&status_pages::IncidentView> = ordered.into_iter().take(open + 10).collect();
    if shown.is_empty() {
        lines.push("No incident.".to_string());
    } else {
        lines.push(format!("Incidents ({open} open):"));
        for view in &shown {
            let i = &view.incident;
            let mut line = format!(
                "- #{} [{}] {} — {} ({}), since {}",
                i.id, i.kind, i.title, i.status, i.severity, i.starts_at
            );
            if let Some(page) = i.page_id.and_then(|id| pages.iter().find(|p| p.page.id == id)) {
                line.push_str(&format!(" on {}", page.page.title));
            }
            if let Some(update) = latest_update(view) {
                line.push_str(&format!("; latest: \"{update}\""));
            }
            lines.push(line);
        }
    }

    Ok(ToolOutput::new(
        lines.join("\n"),
        json!({
            "pages": pages.iter().map(|v| json!({
                "id": v.page.id,
                "title": v.page.title,
                "slug": v.page.slug,
                "path": format!("/s/{}", v.page.slug),
                "published": v.page.published,
                "devices": v.items.len(),
            })).collect::<Vec<_>>(),
            "incidents": shown.iter().map(|v| json!({
                "id": v.incident.id,
                "page_id": v.incident.page_id,
                "title": v.incident.title,
                "kind": v.incident.kind,
                "status": v.incident.status,
                "severity": v.incident.severity,
                "open": !is_closed(&v.incident.status),
                "starts_at": v.incident.starts_at,
                "ends_at": v.incident.ends_at,
                "latest_update": latest_update(v),
            })).collect::<Vec<_>>(),
        }),
    ))
}

pub(super) async fn post_incident(state: &AppState, args: &Args) -> ToolResult {
    let body = args.str("body").ok_or_else(|| failed("\"body\" (the message) is required."))?;
    let status = args.str("status").map(str::to_string);

    let (created, Json(view)) = if let Some(id) = args.i64("incident_id") {
        let payload =
            status_pages::UpdatePayload { status, body: body.to_string(), at: None, notify: None };
        let (_, view) =
            status_pages::add_update(State(state.clone()), Path(id), Json(payload)).await?;
        (false, view)
    } else {
        let title = args.str("title").ok_or_else(|| {
            failed("\"title\" is required to open an incident; pass incident_id to update one.")
        })?;
        let page_id = match args.0.get("page") {
            None | Some(Value::Null) => None,
            Some(reference) => {
                let Json(pages) = status_pages::list_pages(
                    crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
                    State(state.clone()),
                )
                .await?;
                let wanted = match reference {
                    Value::Number(n) => n.to_string(),
                    Value::String(s) => s.trim().to_string(),
                    _ => return Err(failed("\"page\" must be a page id, slug or title.")),
                };
                let found = pages.iter().find(|p| {
                    p.page.id.to_string() == wanted
                        || p.page.slug.eq_ignore_ascii_case(&wanted)
                        || p.page.title.eq_ignore_ascii_case(&wanted)
                });
                match found {
                    Some(page) => Some(page.page.id),
                    None => {
                        return Err(failed(format!(
                            "No status page matches \"{wanted}\". Pages: {}.",
                            pages
                                .iter()
                                .map(|p| format!(
                                    "{} (#{}, {})",
                                    p.page.title, p.page.id, p.page.slug
                                ))
                                .collect::<Vec<_>>()
                                .join(", ")
                        )));
                    }
                }
            }
        };
        let payload = status_pages::IncidentPayload {
            title: title.to_string(),
            kind: args.str("kind").map(str::to_string),
            status,
            severity: args.str("severity").map(str::to_string),
            page_id,
            starts_at: None,
            ends_at: None,
            body: Some(body.to_string()),
            updates: None,
            components: None,
        };
        let (_, view) = status_pages::create_incident(State(state.clone()), Json(payload)).await?;
        (true, view)
    };

    let i = &view.incident;
    let text = if created {
        format!(
            "Opened {} #{} \"{}\" ({}, {}) with its first message; subscribers of the page are \
             notified. Post updates with incident_id {}.",
            i.kind, i.id, i.title, i.status, i.severity, i.id
        )
    } else {
        format!("Update posted on {} #{} \"{}\": now {}.", i.kind, i.id, i.title, i.status)
    };
    Ok(ToolOutput::new(
        text,
        json!({
            "id": i.id,
            "title": i.title,
            "kind": i.kind,
            "status": i.status,
            "severity": i.severity,
            "page_id": i.page_id,
            "updates": view.updates.len(),
        }),
    ))
}
