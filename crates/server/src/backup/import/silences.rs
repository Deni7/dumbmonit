//! Fenêtres de silence.

use super::*;

pub(super) async fn restore_silences(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("silences");
    for silence in &bundle.silences {
        let target_id = match &silence.target {
            None => None,
            Some(reference) => match refs.get(reference) {
                Some(id) => Some(*id),
                None => {
                    report.skipped += 1;
                    report.notes.push(format!(
                        "Maintenance window \"{}\": its device ({reference}) is not in this \
                         backup.",
                        silence.name
                    ));
                    continue;
                }
            },
        };
        let matchers = serde_json::to_string(&silence.matchers)?;
        let schedule = serde_json::to_string(&silence.schedule)?;

        let existing = sqlx::query(
            "SELECT id, comment, matchers, schedule, enabled FROM silences
             WHERE name = ? AND target_id IS ?",
        )
        .bind(&silence.name)
        .bind(target_id)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche du silence")?;

        match existing {
            Some(row) => {
                let same = row.try_get::<String, _>("comment")? == silence.comment
                    && row.try_get::<String, _>("matchers")? == matchers
                    && row.try_get::<String, _>("schedule")? == schedule
                    && (row.try_get::<i64, _>("enabled")? != 0) == silence.enabled;
                if same {
                    report.skipped += 1;
                    continue;
                }
                sqlx::query(
                    "UPDATE silences SET comment = ?, matchers = ?, schedule = ?, enabled = ?
                     WHERE id = ?",
                )
                .bind(&silence.comment)
                .bind(&matchers)
                .bind(&schedule)
                .bind(i64::from(silence.enabled))
                .bind(row.try_get::<i64, _>("id")?)
                .execute(&mut **tx)
                .await
                .context("mise à jour du silence")?;
                report.updated += 1;
            }
            None => {
                sqlx::query(
                    "INSERT INTO silences (name, comment, target_id, matchers, schedule, enabled)
                     VALUES (?, ?, ?, ?, ?, ?)",
                )
                .bind(&silence.name)
                .bind(&silence.comment)
                .bind(target_id)
                .bind(&matchers)
                .bind(&schedule)
                .bind(i64::from(silence.enabled))
                .execute(&mut **tx)
                .await
                .context("création du silence")?;
                report.created += 1;
            }
        }
    }
    Ok(report)
}
