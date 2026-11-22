//! Pages de statut.

use super::*;


pub(super) async fn restore_status_pages(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
    refs: &HashMap<String, i64>,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("status_pages");
    for page in &bundle.status_pages {
        // Une sauvegarde éditée à la main ne doit pas contourner le jeu fermé.
        let mut kept: Vec<&str> = Vec::new();
        for scene in &page.scenes {
            // Une scène retirée se replie sur la plus proche conservée.
            if let Some(canonical) = crate::db::status_pages::canonical_scene(scene)
                && !kept.contains(&canonical)
            {
                kept.push(canonical);
            }
        }
        let scenes = kept.join(",");
        let scene_rotation =
            if crate::db::status_pages::SCENE_ROTATIONS.contains(&page.scene_rotation.as_str()) {
                page.scene_rotation.as_str()
            } else {
                "visit"
            };
        // Domaine : revalidé (sauvegarde éditée à la main), et abandonné avec une
        // note s'il désigne déjà une autre page de cette instance.
        let mut domain = match page.domain.as_deref() {
            None => None,
            Some(raw) => match crate::status_host::normalise_domain(raw) {
                Ok(domain) => domain,
                Err(_) => {
                    report.notes.push(format!(
                        "Status page \"{}\": the domain {raw} is not valid and was left out.",
                        page.slug
                    ));
                    None
                }
            },
        };
        if let Some(name) = domain.as_deref() {
            let taken: Option<(String,)> =
                sqlx::query_as("SELECT slug FROM status_pages WHERE domain = ? AND slug != ?")
                    .bind(name)
                    .bind(&page.slug)
                    .fetch_optional(&mut **tx)
                    .await
                    .context("recherche du domaine d'une page de statut")?;
            if let Some((other,)) = taken {
                report.notes.push(format!(
                    "Status page \"{}\": the domain {name} is already used by \"{other}\" \
                     and was left out.",
                    page.slug
                ));
                domain = None;
            }
        }
        // Le lien du site passe par la même règle que l'API (http/https, sans
        // identifiants) : un lot fabriqué ne pose pas un `javascript:` sur une
        // page publique.
        let homepage_url =
            match crate::api::status_pages::validate_homepage(page.homepage_url.trim()) {
                Ok(url) => url,
                Err(_) => {
                    report.notes.push(format!(
                        "Status page \"{}\": the website link is not an http(s) address and \
                         was left out.",
                        page.slug
                    ));
                    String::new()
                }
            };
        let existing = sqlx::query(
            "SELECT id, title, description, published, theme, show_uptime_days,
                 accent, scenes, scene_rotation, footer_text, homepage_url, domain
             FROM status_pages WHERE slug = ?",
        )
        .bind(&page.slug)
        .fetch_optional(&mut **tx)
        .await
        .context("recherche de la page de statut")?;

        let (id, mut outcome) = match existing {
            Some(row) => {
                let id: i64 = row.try_get("id")?;
                let same = row.try_get::<String, _>("title")? == page.title
                    && row.try_get::<String, _>("description")? == page.description
                    && (row.try_get::<i64, _>("published")? != 0) == page.published
                    && row.try_get::<String, _>("theme")? == page.theme
                    && row.try_get::<i64, _>("show_uptime_days")? == page.show_uptime_days
                    && row.try_get::<String, _>("accent")? == page.accent
                    && row.try_get::<String, _>("scenes")? == scenes
                    && row.try_get::<String, _>("scene_rotation")? == scene_rotation
                    && row.try_get::<String, _>("footer_text")? == page.footer_text
                    && row.try_get::<String, _>("homepage_url")? == homepage_url
                    && row.try_get::<Option<String>, _>("domain")? == domain;
                if !same {
                    sqlx::query(
                        "UPDATE status_pages SET title = ?, description = ?, published = ?,
                             theme = ?, show_uptime_days = ?, accent = ?, scenes = ?,
                             scene_rotation = ?, footer_text = ?, homepage_url = ?,
                             domain = ?, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                         WHERE id = ?",
                    )
                    .bind(&page.title)
                    .bind(&page.description)
                    .bind(i64::from(page.published))
                    .bind(&page.theme)
                    .bind(page.show_uptime_days)
                    .bind(&page.accent)
                    .bind(&scenes)
                    .bind(scene_rotation)
                    .bind(&page.footer_text)
                    .bind(&homepage_url)
                    .bind(&domain)
                    .bind(id)
                    .execute(&mut **tx)
                    .await
                    .context("mise à jour de la page de statut")?;
                }
                (id, if same { RestoreOutcome::Skipped } else { RestoreOutcome::Updated })
            }
            None => {
                let row = sqlx::query(
                    "INSERT INTO status_pages
                         (slug, title, description, published, theme, show_uptime_days,
                          accent, scenes, scene_rotation, footer_text, homepage_url, domain)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
                )
                .bind(&page.slug)
                .bind(&page.title)
                .bind(&page.description)
                .bind(i64::from(page.published))
                .bind(&page.theme)
                .bind(page.show_uptime_days)
                .bind(&page.accent)
                .bind(&scenes)
                .bind(scene_rotation)
                .bind(&page.footer_text)
                .bind(&homepage_url)
                .bind(&domain)
                .fetch_one(&mut **tx)
                .await
                .context("création de la page de statut")?;
                (row.try_get("id")?, RestoreOutcome::Created)
            }
        };
        sqlx::query("UPDATE status_pages SET simple = ? WHERE id = ? AND simple != ?")
            .bind(i64::from(page.simple))
            .bind(id)
            .bind(i64::from(page.simple))
            .execute(&mut **tx)
            .await
            .context("mode simple de la page de statut")?;

        // Marque blanche : absente (ancienne sauvegarde), la page garde son réglage.
        if let Some(hide) = page.hide_branding {
            let changed = sqlx::query(
                "UPDATE status_pages SET hide_branding = ? WHERE id = ? AND hide_branding != ?",
            )
            .bind(i64::from(hide))
            .bind(id)
            .bind(i64::from(hide))
            .execute(&mut **tx)
            .await
            .context("marque blanche de la page de statut")?;
            if changed.rows_affected() > 0 && outcome == RestoreOutcome::Skipped {
                outcome = RestoreOutcome::Updated;
            }
        }

        // Accès : revalidé contre le jeu fermé. Absent (page publique, ou ancienne
        // sauvegarde), la page garde son accès.
        if let Some(mode) = page.access_mode.as_deref()
            && crate::api::status_pages::access::MODES.contains(&mode)
        {
            // Sans empreinte, un mode « mot de passe » refuserait tout le monde.
            let mode = if mode == "password" && page.access_password_hash.is_none() {
                "users"
            } else {
                mode
            };
            sqlx::query(
                "UPDATE status_pages SET access_mode = ?, access_password_hash = ?
                 WHERE id = ? AND (access_mode != ? OR access_password_hash IS NOT ?)",
            )
            .bind(mode)
            .bind(&page.access_password_hash)
            .bind(id)
            .bind(mode)
            .bind(&page.access_password_hash)
            .execute(&mut **tx)
            .await
            .context("accès de la page de statut")?;
        }

        // Référencement : revalidé comme à la saisie ; absent (ancienne
        // sauvegarde), la page garde ses réglages.
        if let Some(seo) = &page.seo {
            match seo.clone().validate() {
                Ok(seo) => {
                    let current: String =
                        sqlx::query_scalar("SELECT seo FROM status_pages WHERE id = ?")
                            .bind(id)
                            .fetch_one(&mut **tx)
                            .await
                            .context("lecture du référencement de la page de statut")?;
                    if crate::status_seo::Seo::parse(&current) != seo {
                        sqlx::query("UPDATE status_pages SET seo = ? WHERE id = ?")
                            .bind(seo.to_json())
                            .bind(id)
                            .execute(&mut **tx)
                            .await
                            .context("référencement de la page de statut")?;
                        if outcome == RestoreOutcome::Skipped {
                            outcome = RestoreOutcome::Updated;
                        }
                    }
                }
                Err(why) => report.notes.push(format!(
                    "Status page \"{}\": the SEO settings were left out ({why})",
                    page.slug
                )),
            }
        }

        // Canaux d'abonnement : un canal de notification désigné par son nom
        // qui n'existe pas ici laisse le réglage tel qu'il est.
        if let Some(subscriptions) = &page.subscriptions {
            let channels = channel_ids(tx).await?;
            let named =
                |name: &Option<String>| name.as_ref().and_then(|n| channels.get(n).copied());
            let email = named(&subscriptions.email_channel);
            let sms = named(&subscriptions.sms_channel);
            for (wanted, got, label) in [
                (&subscriptions.email_channel, email, "email"),
                (&subscriptions.sms_channel, sms, "SMS"),
            ] {
                if wanted.is_some() && got.is_none() {
                    report.notes.push(format!(
                        "Status page \"{}\": the {label} subscription channel is not on this instance",
                        page.slug
                    ));
                }
            }
            // Une sauvegarde d'avant l'interrupteur (0246) ouvrait le courriel
            // en nommant un canal ; sans canal ni interrupteur, rien ne change.
            let email_open = subscriptions
                .email
                .or(subscriptions.email_channel.as_ref().map(|_| true))
                .map(i64::from);
            sqlx::query(
                "UPDATE status_pages SET subscribe_webhook = ?, subscribe_slack = ?,
                     subscribe_channel_id = COALESCE(?, subscribe_channel_id),
                     subscribe_sms_channel_id = COALESCE(?, subscribe_sms_channel_id),
                     subscribe_email = COALESCE(?, subscribe_email)
                 WHERE id = ?",
            )
            .bind(i64::from(subscriptions.webhook))
            .bind(i64::from(subscriptions.slack))
            .bind(email)
            .bind(sms)
            .bind(email_open)
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("canaux d'abonnement de la page de statut")?;
        }

        // Dossiers cochés : ajoutés ou remis à leur rang, jamais retirés.
        for (position, name) in page.groups.iter().enumerate() {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            let changed = sqlx::query(
                "INSERT INTO status_page_groups (page_id, group_name, position) VALUES (?, ?, ?)
                 ON CONFLICT(page_id, group_name) DO UPDATE SET position = excluded.position
                 WHERE position IS NOT excluded.position",
            )
            .bind(id)
            .bind(name)
            .bind(position as i64)
            .execute(&mut **tx)
            .await
            .context("enregistrement d'un groupe de page de statut")?;
            if changed.rows_affected() > 0 && outcome == RestoreOutcome::Skipped {
                outcome = RestoreOutcome::Updated;
            }
        }
        for entry in &page.group_labels {
            let Some(target_id) = refs.get(&entry.target).copied() else {
                report.notes.push(format!(
                    "Status page \"{}\": the monitor {} named in a group is not in this backup.",
                    page.slug, entry.target
                ));
                continue;
            };
            let changed = sqlx::query(
                "INSERT INTO status_page_group_labels (page_id, target_id, label) VALUES (?, ?, ?)
                 ON CONFLICT(page_id, target_id) DO UPDATE SET label = excluded.label
                 WHERE label IS NOT excluded.label",
            )
            .bind(id)
            .bind(target_id)
            .bind(&entry.label)
            .execute(&mut **tx)
            .await
            .context("enregistrement d'un nom public de groupe")?;
            if changed.rows_affected() > 0 && outcome == RestoreOutcome::Skipped {
                outcome = RestoreOutcome::Updated;
            }
        }

        for item in &page.items {
            let Some(target_id) = refs.get(&item.target).copied() else {
                report.notes.push(format!(
                    "Status page \"{}\": the monitor {} it listed is not in this backup.",
                    page.slug, item.target
                ));
                continue;
            };
            let changed = sqlx::query(
                "INSERT INTO status_page_items (page_id, target_id, label, group_name, position)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT(page_id, target_id) DO UPDATE SET
                     label = excluded.label, group_name = excluded.group_name,
                     position = excluded.position
                 WHERE label IS NOT excluded.label OR group_name IS NOT excluded.group_name
                    OR position IS NOT excluded.position",
            )
            .bind(id)
            .bind(target_id)
            .bind(&item.label)
            .bind(&item.group_name)
            .bind(item.position)
            .execute(&mut **tx)
            .await
            .context("enregistrement d'un service de page de statut")?;
            if changed.rows_affected() > 0 && outcome == RestoreOutcome::Skipped {
                outcome = RestoreOutcome::Updated;
            }
        }
        report.count(outcome);
    }
    Ok(report)
}
