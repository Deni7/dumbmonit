//! Rapports : logo et planifications.

use super::*;


/// Logo des rapports : remplacé par celui du lot s'il se lit et s'incorpore au
/// PDF (mêmes contrôles que l'API). Un lot sans logo ne touche à rien.
pub(super) async fn restore_report_logo(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    use crate::api::status_pages::branding::{MAX_LOGO_BYTES, sniff};
    let mut report = SectionReport::new("report_logo");
    let Some(item) = &bundle.report_logo else { return Ok(report) };
    let decoded = BASE64
        .decode(item.data.trim())
        .ok()
        .filter(|bytes| !bytes.is_empty() && bytes.len() <= MAX_LOGO_BYTES)
        .filter(|bytes| crate::reports::logo::for_pdf(bytes).is_some())
        .and_then(|bytes| sniff(&bytes).map(|kind| (kind, bytes)));
    let Some((kind, bytes)) = decoded else {
        report.skipped += 1;
        report.notes.push("The report logo of the backup is not a usable PNG or JPEG".into());
        return Ok(report);
    };
    let existing: Option<(String, Vec<u8>)> =
        sqlx::query_as("SELECT logo_type, logo FROM report_logo WHERE id = 1")
            .fetch_optional(&mut **tx)
            .await
            .context("lecture du logo des rapports")?;
    match existing {
        Some((stored_kind, stored)) if stored_kind == kind && stored == bytes => {
            report.skipped += 1;
            return Ok(report);
        }
        Some(_) => report.count(RestoreOutcome::Updated),
        None => report.count(RestoreOutcome::Created),
    }
    sqlx::query(
        "INSERT INTO report_logo (id, logo_type, logo) VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET logo_type = excluded.logo_type, logo = excluded.logo,
             updated_at = strftime('%Y-%m-%d %H:%M:%S', 'now')",
    )
    .bind(kind)
    .bind(bytes.as_slice())
    .execute(&mut **tx)
    .await
    .context("écriture du logo des rapports")?;
    Ok(report)
}

pub(super) async fn restore_report_schedules(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    channels: &HashMap<String, i64>,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    use crate::reports::layout::Scope;
    use crate::reports::schedule::{Frequency, parse_timezone};

    let mut report = SectionReport::new("report_schedules");
    for item in &bundle.report_schedules {
        // Une sauvegarde éditée à la main ne doit pas contourner les contrôles de l'API.
        let name = item.name.trim();
        let kind = item.kind.trim();
        let valid = !name.is_empty()
            && name.chars().count() <= 80
            && !name.chars().any(char::is_control)
            && Frequency::parse(&item.frequency).is_some()
            && item.weekday <= 6
            && (1..=28).contains(&item.day_of_month)
            && item.hour <= 23
            && parse_timezone(&item.timezone).is_some()
            && (kind == "digest" || (kind == "sla" && item.frequency == "monthly"))
            && item.objective.is_none_or(|v| v.is_finite() && v > 0.0 && v <= 100.0);
        let recipients = crate::reports::normalise_recipients(&item.recipients);
        let layout = item.layout.clone().validate();
        let (true, Ok(recipients), Ok(layout)) = (valid, recipients, layout) else {
            report.skipped += 1;
            report
                .notes
                .push(format!("Report \"{}\" has invalid settings and was skipped.", item.name));
            continue;
        };
        let channel_id = match &item.channel {
            None => None,
            Some(channel) => match channels.get(channel) {
                Some(id) => Some(*id),
                None => {
                    report.notes.push(format!(
                        "Report \"{name}\": the channel \"{channel}\" is not in this backup; the \
                         first email channel will be used."
                    ));
                    None
                }
            },
        };
        let page_id = match &item.page {
            None => None,
            Some(slug) => {
                let found: Option<i64> =
                    sqlx::query_scalar("SELECT id FROM status_pages WHERE slug = ?")
                        .bind(slug)
                        .fetch_optional(&mut **tx)
                        .await
                        .context("recherche de la page de statut du rapport")?;
                if found.is_none() {
                    report.notes.push(format!(
                        "Report \"{name}\": the status page \"{slug}\" is not in this backup; \
                         the report is headed by DumbMonit."
                    ));
                }
                found
            }
        };
        let mut targets = Vec::new();
        for reference in &item.scope_targets {
            match refs.get(reference) {
                Some(id) => targets.push(*id),
                None => report.notes.push(format!(
                    "Report \"{name}\": the monitor \"{reference}\" of its scope is not in this \
                     backup and was left out."
                )),
            }
        }
        let Ok(scope) = Scope { targets, groups: item.scope_groups.clone() }.validate() else {
            report.skipped += 1;
            report.notes.push(format!("Report \"{name}\" has an invalid scope and was skipped."));
            continue;
        };
        let recipients_json = serde_json::to_string(&recipients)?;
        let layout_json = serde_json::to_string(&layout)?;
        let scope_json = serde_json::to_string(&scope)?;
        let enabled = item.enabled && !recipients.is_empty();

        let existing = sqlx::query(
            "SELECT id, enabled, frequency, weekday, day_of_month, hour, timezone, recipients,
                 channel_id, kind, attach_pdf, attach_csv, page_id, objective, layout, scope
             FROM report_schedules WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du rapport planifié")?;

        match existing {
            Some(row) => {
                let same = (row.try_get::<i64, _>("enabled")? != 0) == enabled
                    && row.try_get::<String, _>("frequency")? == item.frequency
                    && row.try_get::<i64, _>("weekday")? == i64::from(item.weekday)
                    && row.try_get::<i64, _>("day_of_month")? == i64::from(item.day_of_month)
                    && row.try_get::<i64, _>("hour")? == i64::from(item.hour)
                    && row.try_get::<String, _>("timezone")? == item.timezone.trim()
                    && row.try_get::<String, _>("recipients")? == recipients_json
                    && row.try_get::<Option<i64>, _>("channel_id")? == channel_id
                    && row.try_get::<String, _>("kind")? == kind
                    && (row.try_get::<i64, _>("attach_pdf")? != 0) == item.attach_pdf
                    && (row.try_get::<i64, _>("attach_csv")? != 0) == item.attach_csv
                    && row.try_get::<Option<i64>, _>("page_id")? == page_id
                    && row.try_get::<Option<f64>, _>("objective")? == item.objective
                    && row.try_get::<String, _>("layout")? == layout_json
                    && row.try_get::<String, _>("scope")? == scope_json;
                if same {
                    report.skipped += 1;
                    continue;
                }
                // Réarmé : un créneau antérieur à la restauration ne part pas après coup.
                sqlx::query(
                    "UPDATE report_schedules
                     SET enabled = ?, frequency = ?, weekday = ?, day_of_month = ?, hour = ?,
                         timezone = ?, recipients = ?, channel_id = ?, kind = ?, attach_pdf = ?,
                         attach_csv = ?, page_id = ?, objective = ?, layout = ?, scope = ?,
                         armed_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?",
                )
                .bind(i64::from(enabled))
                .bind(&item.frequency)
                .bind(i64::from(item.weekday))
                .bind(i64::from(item.day_of_month))
                .bind(i64::from(item.hour))
                .bind(item.timezone.trim())
                .bind(&recipients_json)
                .bind(channel_id)
                .bind(kind)
                .bind(i64::from(item.attach_pdf))
                .bind(i64::from(item.attach_csv))
                .bind(page_id)
                .bind(item.objective)
                .bind(&layout_json)
                .bind(&scope_json)
                .bind(row.try_get::<i64, _>("id")?)
                .execute(&mut **tx)
                .await
                .context("mise à jour du rapport planifié")?;
                report.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO report_schedules
                         (name, enabled, frequency, weekday, day_of_month, hour, timezone,
                          recipients, channel_id, kind, attach_pdf, attach_csv, page_id,
                          objective, layout, scope, armed_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
                )
                .bind(name)
                .bind(i64::from(enabled))
                .bind(&item.frequency)
                .bind(i64::from(item.weekday))
                .bind(i64::from(item.day_of_month))
                .bind(i64::from(item.hour))
                .bind(item.timezone.trim())
                .bind(&recipients_json)
                .bind(channel_id)
                .bind(kind)
                .bind(i64::from(item.attach_pdf))
                .bind(i64::from(item.attach_csv))
                .bind(page_id)
                .bind(item.objective)
                .bind(&layout_json)
                .bind(&scope_json)
                .execute(&mut **tx)
                .await
                .context("création du rapport planifié")?;
                report.created += 1;
            }
        }
    }
    Ok(report)
}
