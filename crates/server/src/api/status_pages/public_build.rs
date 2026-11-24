//! Assemblage du document public d'une page.

use super::*;

/// Document d'une page tel que le public le lit. `preview` : pour l'éditeur
/// (administration), chaque service porte en plus son identifiant de
/// surveillance ; ce document-là n'est jamais mis en cache ni servi au public.
pub(super) async fn build_public(
    state: &AppState,
    page: &StatusPage,
    preview: bool,
) -> ApiResult<PublicStatus> {
    let now = Utc::now();
    // Fenêtre adaptative : l'âge des données (plus ancien appareil montré), borné
    // par le réglage de la page, `MIN_HISTORY_DAYS` au moins.
    let cap = page.show_uptime_days.clamp(MIN_HISTORY_DAYS, MAX_HISTORY_DAYS);
    let oldest = db::status_pages::oldest_target_created_at(&state.pool, page.id)
        .await?
        .as_deref()
        .and_then(parse_stored);
    let age_days = oldest.map_or(0, |created| (now - created).num_days() + 1);
    let age_secs = oldest.map_or(0, |created| (now - created).num_seconds().max(0));
    let days = age_days.clamp(MIN_HISTORY_DAYS, cap);
    // Période courte (24 h, 7 jours) ou historique de moins d'une semaine : la
    // frise passe à la maille fine, sur ce que les données couvrent vraiment.
    let slot_plan = timeline::plan(page.show_uptime_days, age_secs);

    let items = db::status_pages::effective_items(&state.pool, page.id).await?;
    let targets: HashMap<TargetId, Target> = db::targets::list(&state.pool, &state.cipher)
        .await?
        .into_iter()
        .map(|target| (target.id, target))
        .collect();
    let statuses = db::targets::statuses(&state.pool).await?;
    let shown: Vec<&Target> =
        items.iter().filter_map(|item| targets.get(&item.target_id)).collect();
    // Le calendrier montre toujours 90 jours, quelle que soit la fenêtre des barres.
    let metrics_days = if page.show_calendar { MAX_HISTORY_DAYS } else { days };
    // Appareils qui ne sondent plus : désactivés ou jamais sondés (état « inconnu »
    // de `item_state`). Leur silence n'est pas une panne.
    let inactive: HashSet<TargetId> = shown
        .iter()
        .filter(|t| {
            !t.enabled
                || statuses
                    .get(&t.id)
                    .and_then(|s| s.last_probe_at.as_deref())
                    .and_then(parse_stored)
                    .is_none()
        })
        .map(|t| t.id)
        .collect();
    let slot_starts = slot_plan.map(|p| timeline::slot_starts(p, now.timestamp()));
    let slots = match (slot_plan, &slot_starts) {
        (Some(plan), Some(starts)) => {
            timeline::collect(
                &state.victoria,
                &shown,
                starts,
                plan.step_secs,
                now.timestamp(),
                &inactive,
            )
            .await
        }
        _ => HashMap::new(),
    };
    let metrics = collect_metrics(
        &state.victoria,
        &shown,
        metrics_days,
        days,
        now,
        page.show_latency,
        &inactive,
    )
    .await;

    // Maintenance planifiée : une fenêtre de maintenance posée dans l'alerting
    // vaut annonce publique. Sans cela, la page montre du rouge pendant que
    // l'opérateur est sciemment en train de redémarrer la machine, et les
    // lecteurs apprennent à ignorer la page.
    let silences = db::alerts::list_maintenance_silences(&state.pool).await?;
    let planned: HashSet<TargetId> = shown
        .iter()
        .filter(|target| {
            let labels = state.collectors.base_labels(target);
            crate::alerting::silence::first_match(&silences, now, Some(target.id), &labels)
                .is_some()
        })
        .map(|target| target.id)
        .collect();

    // Incidents : ceux de la page et les globaux, depuis le début de l'historique
    // affiché (au moins trente jours, pour la liste des incidents passés).
    let since = now - TimeDelta::days(days.max(PAST_INCIDENTS_DAYS));
    let incidents =
        db::status_pages::incidents_for_page(&state.pool, page.id, &format_timestamp(since))
            .await?;
    let ids: Vec<i64> = incidents.iter().map(|incident| incident.id).collect();
    let mut updates: HashMap<i64, Vec<IncidentUpdate>> = HashMap::new();
    for update in db::status_pages::list_updates_for(&state.pool, &ids).await? {
        updates.entry(update.incident_id).or_default().push(update);
    }
    // Post-mortems publiés : une seule lecture, le filtre par incident suit.
    let mut postmortems = db::status_pages::published_postmortems(&state.pool).await?;

    // Fenêtres d'annonces, telles que vues maintenant.
    let mut maintenance_active = false;
    let mut major_incident = false;
    let mut open_incident = false;
    // Intervalles des incidents (hors maintenance), pour le compte par jour.
    let mut incident_spans: Vec<(i64, i64)> = Vec::new();
    for incident in &incidents {
        let starts = parse_stored(&incident.starts_at).unwrap_or(now).timestamp();
        let ends = incident.ends_at.as_deref().and_then(parse_stored).map(|at| at.timestamp());
        if incident.kind == "maintenance" {
            let in_window =
                starts <= now.timestamp() && ends.is_none_or(|end| end > now.timestamp());
            if incident.status == "in_progress" || (incident.status == "scheduled" && in_window) {
                maintenance_active = true;
            }
        } else {
            if !is_closing(&incident.status) {
                open_incident = true;
                if incident.severity == "major" {
                    major_incident = true;
                }
            }
            incident_spans.push((starts, ends.unwrap_or(now.timestamp())));
        }
    }

    // Jours affichés, du plus ancien au plus récent.
    let today_start = now.timestamp().div_euclid(86_400) * 86_400;
    let day_starts: Vec<i64> = (0..days).map(|i| today_start - (days - 1 - i) * 86_400).collect();
    let incidents_per_day: Vec<u32> = day_starts
        .iter()
        .map(|&day| {
            incident_spans
                .iter()
                .filter(|(starts, ends)| *starts < day + 86_400 && *ends >= day)
                .count() as u32
        })
        .collect();
    let empty_daily = HashMap::new();
    // Pire disponibilité de chaque jour du calendrier (indexée par début de jour).
    let mut worst_daily: HashMap<i64, f64> = HashMap::new();
    let mut keys: HashSet<String> = HashSet::new();

    let mut groups: Vec<PublicGroup> = Vec::new();
    let mut n_down = 0usize;
    let mut n_degraded = 0usize;
    let mut n_maintenance = 0usize;
    let mut n_items = 0usize;
    // Clé publique de chaque service, pour dire quels services touche une annonce.
    let mut key_of: HashMap<TargetId, String> = HashMap::new();
    for item in items {
        let Some(target) = targets.get(&item.target_id) else { continue };
        let mut state_word = item_state(target, statuses.get(&target.id), &metrics, now);
        // Pendant une maintenance, une panne est attendue : on le dit plutôt que
        // d'afficher une alerte que personne ne doit traiter. Une maintenance
        // déclarée sur la page vaut pour tous ses services ; une fenêtre de
        // maintenance de l'alerting ne vaut que pour l'équipement qu'elle vise.
        let under_maintenance = maintenance_active || planned.contains(&target.id);
        if under_maintenance && matches!(state_word, "down" | "degraded") {
            state_word = "maintenance";
        }
        n_items += 1;
        match state_word {
            "down" => n_down += 1,
            "degraded" => n_degraded += 1,
            "maintenance" => n_maintenance += 1,
            _ => {}
        }
        let daily = metrics.daily.get(&target.id).unwrap_or(&empty_daily);
        let daily_down = metrics.daily_down.get(&target.id).unwrap_or(&empty_daily);
        if page.show_calendar {
            for (&day, &value) in daily {
                worst_daily.entry(day).and_modify(|w| *w = w.min(value)).or_insert(value);
            }
        }
        let history: Vec<DayBucket> = if let (Some(plan), Some(starts)) = (slot_plan, &slot_starts)
        {
            let own = slots.get(&target.id);
            starts
                .iter()
                .map(|&start| {
                    let slot = own.and_then(|own| own.get(&start));
                    DayBucket {
                        date: DateTime::<Utc>::from_timestamp(start, 0)
                            .map(format_timestamp)
                            .unwrap_or_default(),
                        uptime_pct: slot.map(|s| round(s.uptime_pct)),
                        down_minutes: slot.map(|s| s.down_minutes.round().max(0.0) as u32),
                        incidents: incident_spans
                            .iter()
                            .filter(|(from, to)| *from < start + plan.step_secs && *to >= start)
                            .count() as u32,
                    }
                })
                .collect()
        } else {
            day_starts
                .iter()
                .zip(&incidents_per_day)
                .map(|(&day, &count)| DayBucket {
                    date: NaiveDate::from_ymd_opt(1970, 1, 1)
                        .and_then(|epoch| epoch.checked_add_signed(TimeDelta::seconds(day)))
                        .map(|date| date.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    uptime_pct: daily.get(&day).map(|v| round(*v)),
                    // Une journée sans mesure n'a pas de minutes de panne : elle est
                    // inconnue, pas parfaite.
                    down_minutes: daily.get(&day).map(|_| {
                        daily_down.get(&day).map_or(0, |minutes| minutes.round().max(0.0) as u32)
                    }),
                    incidents: count,
                })
                .collect()
        };
        let key = unique_key(&item.label, &mut keys);
        key_of.insert(target.id, key.clone());
        let public_item = PublicItem {
            key,
            target_id: preview.then_some(target.id),
            label: item.label,
            state: state_word,
            uptime_24h: metrics.uptime_24h.get(&target.id).map(|v| round(*v)),
            uptime_7d: metrics.uptime_7d.get(&target.id).map(|v| round(*v)),
            uptime_30d: metrics.uptime_30d.get(&target.id).map(|v| round(*v)),
            uptime_90d: (days >= 90)
                .then(|| metrics.uptime_90d.get(&target.id).map(|v| round(*v)))
                .flatten(),
            uptime_history: if slot_plan.is_some() {
                slots.get(&target.id).and_then(timeline::period_uptime).map(round)
            } else {
                metrics.uptime_history.get(&target.id).map(|v| round(*v))
            },
            latency_ms: metrics.latency_ms.get(&target.id).map(|v| round(*v)),
            latency_series: match (
                metrics.latency_24h.get(&target.id),
                metrics.latency_7d.get(&target.id),
            ) {
                (None, None) => None,
                (h24, d7) => Some(LatencySeries {
                    h24: h24.cloned().unwrap_or_default(),
                    d7: d7.cloned().unwrap_or_default(),
                }),
            },
            history,
            external: target.kind == "provider",
        };
        match groups.iter_mut().find(|group| group.name == item.group_name) {
            Some(group) => group.items.push(public_item),
            None => groups.push(PublicGroup { name: item.group_name, items: vec![public_item] }),
        }
    }

    // « Maintenance » en tête de page dès qu'une maintenance est déclarée, ou
    // que tout ce qui ne va pas est couvert par une fenêtre : sinon un service
    // réellement en panne serait maquillé en maintenance.
    let overall = if maintenance_active
        || (n_maintenance > 0 && n_down == 0 && n_degraded == 0 && !open_incident)
    {
        "maintenance"
    } else if major_incident || (n_down > 0 && n_down == n_items) {
        "major"
    } else if n_down > 0 || n_degraded > 0 || open_incident {
        "degraded"
    } else {
        "operational"
    };

    let mut incident_keys: HashMap<i64, Vec<String>> = HashMap::new();
    for (incident_id, target_id) in db::status_pages::incident_component_rows(&state.pool).await? {
        if let Some(key) = key_of.get(&target_id) {
            incident_keys.entry(incident_id).or_default().push(key.clone());
        }
    }
    let (maintenance, past): (Vec<Incident>, Vec<Incident>) =
        incidents.into_iter().partition(|incident| incident.kind == "maintenance");
    let cutoff = format_timestamp(now - TimeDelta::days(PAST_INCIDENTS_DAYS));
    let public_incidents = past
        .into_iter()
        .filter(|incident| {
            !is_closing(&incident.status)
                || incident.ends_at.as_deref().is_none_or(|end| end >= cutoff.as_str())
        })
        .map(|incident| {
            let updates = updates.remove(&incident.id).unwrap_or_default();
            let postmortem = postmortems.remove(&incident.id);
            let components = incident_keys.remove(&incident.id).unwrap_or_default();
            to_public_incident(incident, updates, postmortem, components)
        })
        .collect();
    let public_maintenance = maintenance
        .into_iter()
        .filter(|incident| {
            !is_closing(&incident.status)
                || incident.ends_at.as_deref().is_none_or(|end| end >= cutoff.as_str())
        })
        .map(|incident| {
            let updates = updates.remove(&incident.id).unwrap_or_default();
            let postmortem = postmortems.remove(&incident.id);
            let components = incident_keys.remove(&incident.id).unwrap_or_default();
            to_public_incident(incident, updates, postmortem, components)
        })
        .collect();

    let calendar = page.show_calendar.then(|| {
        let cal_days = MAX_HISTORY_DAYS;
        (0..cal_days)
            .map(|i| {
                let day = today_start - (cal_days - 1 - i) * 86_400;
                let worst = worst_daily.get(&day).map(|v| round(*v));
                let incidents = incident_spans
                    .iter()
                    .filter(|(starts, ends)| *starts < day + 86_400 && *ends >= day)
                    .count() as u32;
                CalendarDay {
                    date: NaiveDate::from_ymd_opt(1970, 1, 1)
                        .and_then(|epoch| epoch.checked_add_signed(TimeDelta::seconds(day)))
                        .map(|date| date.format("%Y-%m-%d").to_string())
                        .unwrap_or_default(),
                    state: calendar_state(worst, incidents),
                    uptime_pct: worst,
                    incidents,
                }
            })
            .collect()
    });

    let subscribe_kinds = subscribers::available_kinds(state, page).await;
    let brand = crate::auth::clients::branding_of_page(&state.pool, page.id).await.ok().flatten();
    let logo_url = match (&page.logo_type, &brand) {
        (Some(_), _) => Some(format!(
            "/api/public/status/{}/logo?v={}",
            page.slug,
            version_tag(&page.updated_at)
        )),
        (None, Some(client)) if client.has_logo => {
            Some(format!("/api/public/status/{}/logo?v=c{}", page.slug, client.id))
        }
        _ => None,
    };
    Ok(PublicStatus {
        page: PublicPage {
            slug: page.slug.clone(),
            title: page.title.clone(),
            description: page.description.clone(),
            theme: page.theme.clone(),
            show_uptime_days: cap,
            history_days: days,
            window_secs: slot_plan.map_or(days * 86_400, |p| p.window_secs),
            step_secs: slot_plan.map_or(86_400, |p| p.step_secs),
            updated_at: page.updated_at.clone(),
            accent: page.accent.clone(),
            scenes: split_scenes(&page.scenes),
            scene_rotation: page.scene_rotation.clone(),
            simple: page.simple,
            show_calendar: page.show_calendar,
            show_latency: page.show_latency,
            footer_text: page.footer_text.clone(),
            homepage_url: page.homepage_url.clone(),
            logo_url,
            brand_name: brand.map(|client| client.name),
            subscribe: !subscribe_kinds.is_empty(),
            subscribe_kinds,
            seo: crate::status_seo::resolve(page),
            hide_branding: page.hide_branding,
        },
        overall,
        generated_at: format_timestamp(now),
        groups,
        calendar,
        incidents: public_incidents,
        maintenance: public_maintenance,
    })
}
