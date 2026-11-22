//! Clients et rattachements.

use super::*;

/// Clients (mode MSP) : créés ou mis à jour par nom. Les membres du lot sont
/// ajoutés (un compte absent est signalé), sans retirer ceux de l'instance.
pub(super) async fn restore_clients(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("clients");
    if let Some(settings) = &bundle.msp_settings
        && serde_json::from_value::<crate::auth::clients::MspSettings>(settings.clone()).is_ok()
    {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(crate::auth::clients::SETTINGS_KEY)
        .bind(serde_json::to_vec(settings)?)
        .execute(&mut **tx)
        .await
        .context("réglage du mode MSP")?;
    }
    for client in &bundle.clients {
        let Ok(name) = crate::auth::clients::validate_name(&client.name) else {
            report.skipped += 1;
            report.notes.push(format!("client \"{}\" has no valid name", client.name));
            continue;
        };
        // Une sauvegarde éditée à la main ne contourne pas les contrôles de l'API.
        let contacts = crate::reports::normalise_recipients(&client.contacts).unwrap_or_default();
        let quota = client.monitor_quota.filter(|q| (0..=1_000_000).contains(q));
        let objective = client.sla_objective.filter(|v| v.is_finite() && *v > 0.0 && *v <= 100.0);
        let valid_tz = crate::reports::schedule::parse_timezone(&client.report_timezone).is_some();
        let timezone =
            if valid_tz { client.report_timezone.trim().to_string() } else { "UTC".to_string() };
        let logo = match (&client.logo, &client.logo_type) {
            (Some(data), Some(_)) => BASE64
                .decode(data)
                .ok()
                .filter(|bytes| bytes.len() <= crate::auth::clients::MAX_LOGO_BYTES)
                .and_then(|bytes| {
                    crate::api::status_pages::branding::sniff(&bytes).map(|kind| (kind, bytes))
                }),
            _ => None,
        };
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM clients WHERE name = ? COLLATE NOCASE")
                .bind(&name)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche du client")?;
        let id: i64 = match existing {
            Some(id) => id,
            None => {
                report.count(RestoreOutcome::Created);
                sqlx::query_scalar("INSERT INTO clients (name) VALUES (?) RETURNING id")
                    .bind(&name)
                    .fetch_one(&mut **tx)
                    .await
                    .context("création du client")?
            }
        };
        let changed = sqlx::query(
            "UPDATE clients SET description = ?1, contacts = ?2, monitor_quota = ?3, idp_groups = ?4,
                    sla_report = ?5, sla_objective = ?6, report_timezone = ?7,
                    logo_type = ?8, logo = ?9
             WHERE id = ?10
               AND (description IS NOT ?1 OR contacts IS NOT ?2 OR monitor_quota IS NOT ?3
                    OR idp_groups IS NOT ?4 OR sla_report IS NOT ?5 OR sla_objective IS NOT ?6
                    OR report_timezone IS NOT ?7 OR logo_type IS NOT ?8 OR logo IS NOT ?9)",
        )
        .bind(&client.description)
        .bind(serde_json::to_string(&contacts)?)
        .bind(quota)
        .bind(serde_json::to_string(&client.idp_groups)?)
        .bind(i64::from(client.sla_report))
        .bind(objective)
        .bind(&timezone)
        .bind(logo.as_ref().map(|(kind, _)| *kind))
        .bind(logo.as_ref().map(|(_, bytes)| bytes.clone()))
        .bind(id)
        .execute(&mut **tx)
        .await
        .context("mise à jour du client")?
        .rows_affected();
        // Un client déjà identique ne compte pas comme modifié : une seconde
        // restauration du même lot ne change rien.
        if existing.is_some() {
            report.count(if changed > 0 {
                RestoreOutcome::Updated
            } else {
                RestoreOutcome::Skipped
            });
        }
        for member in &client.members {
            let source = match member.source.as_str() {
                "idp" | "scim" => member.source.as_str(),
                _ => "manual",
            };
            let user: Option<i64> =
                sqlx::query_scalar("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
                    .bind(&member.username)
                    .fetch_optional(&mut **tx)
                    .await
                    .context("membre du client")?;
            let Some(user) = user else {
                report.notes.push(format!(
                    "client \"{name}\": account \"{}\" not found, membership skipped",
                    member.username
                ));
                continue;
            };
            sqlx::query(
                "INSERT INTO client_members (client_id, user_id, source) VALUES (?, ?, ?)
                 ON CONFLICT(client_id, user_id) DO NOTHING",
            )
            .bind(id)
            .bind(user)
            .bind(source)
            .execute(&mut **tx)
            .await
            .context("membre du client")?;
        }
    }
    Ok(report)
}

/// Rattachements aux clients, une fois tous les objets restaurés. Les liens
/// qui franchiraient une frontière entre clients (parent, relais, composant de
/// page…) sont ensuite coupés, comme après un déplacement.
pub(super) async fn restore_client_assignments(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    report: &mut SectionReport,
) -> Result<()> {
    use crate::auth::clients::Kind;
    if bundle.client_assignments.is_empty() {
        return Ok(());
    }
    let names: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM clients")
        .fetch_all(&mut **tx)
        .await
        .context("relecture des clients")?;
    let by_name: HashMap<String, i64> =
        names.into_iter().map(|(id, name)| (name.to_lowercase(), id)).collect();
    let refs = target_ids(tx).await?;
    for assignment in &bundle.client_assignments {
        let Some(client) = by_name.get(&assignment.client.to_lowercase()).copied() else {
            report.notes.push(format!(
                "client \"{}\" is not in this backup: \"{}\" left where it is",
                assignment.client, assignment.key
            ));
            continue;
        };
        let done = match assignment.kind {
            Kind::Target => match refs.get(&assignment.key) {
                Some(id) => sqlx::query("UPDATE targets SET client_id = ? WHERE id = ?")
                    .bind(client)
                    .bind(*id)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected(),
                None => 0,
            },
            Kind::StatusPage => sqlx::query("UPDATE status_pages SET client_id = ? WHERE slug = ?")
                .bind(client)
                .bind(&assignment.key)
                .execute(&mut **tx)
                .await?
                .rows_affected(),
            Kind::AgentToken => {
                sqlx::query("UPDATE agent_tokens SET client_id = ? WHERE token_hash = ?")
                    .bind(client)
                    .bind(&assignment.key)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
            Kind::Channel => {
                sqlx::query("UPDATE notification_channels SET client_id = ? WHERE name = ?")
                    .bind(client)
                    .bind(&assignment.key)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
            Kind::Rule => sqlx::query("UPDATE alert_rules SET client_id = ? WHERE uid = ?")
                .bind(client)
                .bind(&assignment.key)
                .execute(&mut **tx)
                .await?
                .rows_affected(),
            Kind::Report => sqlx::query("UPDATE report_schedules SET client_id = ? WHERE name = ?")
                .bind(client)
                .bind(&assignment.key)
                .execute(&mut **tx)
                .await?
                .rows_affected(),
            Kind::Silence => {
                let (name, target) =
                    assignment.key.split_once('\n').unwrap_or((assignment.key.as_str(), ""));
                let target = if target.is_empty() { None } else { refs.get(target).copied() };
                sqlx::query("UPDATE silences SET client_id = ? WHERE name = ? AND target_id IS ?")
                    .bind(client)
                    .bind(name)
                    .bind(target)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
            Kind::Schedule => {
                sqlx::query("UPDATE oncall_schedules SET client_id = ? WHERE name = ?")
                    .bind(client)
                    .bind(&assignment.key)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
            Kind::EscalationPolicy => {
                sqlx::query("UPDATE oncall_policies SET client_id = ? WHERE name = ?")
                    .bind(client)
                    .bind(&assignment.key)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
            Kind::Runbook => sqlx::query("UPDATE runbooks SET client_id = ? WHERE title = ?")
                .bind(client)
                .bind(&assignment.key)
                .execute(&mut **tx)
                .await?
                .rows_affected(),
            Kind::RemediationAction => {
                sqlx::query("UPDATE remediation_actions SET client_id = ? WHERE name = ?")
                    .bind(client)
                    .bind(&assignment.key)
                    .execute(&mut **tx)
                    .await?
                    .rows_affected()
            }
        };
        if done == 0 {
            report.notes.push(format!(
                "\"{}\" ({}) is not in this instance: its client was not restored",
                assignment.key,
                assignment.kind.key()
            ));
        }
    }
    crate::auth::clients::sever_cross_client_links(tx).await?;
    Ok(())
}
