//! Alertes actives, acquittement et historique.

use super::*;

// --------------------------------------------------------------------------
// Alertes actives et historique
// --------------------------------------------------------------------------

pub async fn list_active(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Query(query): Query<ActiveQuery>,
) -> ApiResult<Json<Vec<ActiveAlertView>>> {
    let now = Utc::now();
    // Compte restreint par ses équipes : seulement les alertes de ses
    // équipements (une alerte sans équipement n'appartient à aucun dossier).
    let visible = access.visible_targets(&state.pool).await?;
    let alerts = db::alerts::list_active(&state.pool).await?;
    let rules: HashMap<String, Rule> = db::alerts::list_rules(&state.pool)
        .await?
        .into_iter()
        .map(|rule| (rule.uid.clone(), rule))
        .collect();
    // Une alerte dont l'équipement a été supprimé ou mis en pause est déjà
    // éteinte pour l'utilisateur, même si le moteur ne l'a pas encore purgée :
    // l'interface ne doit jamais afficher une carte sans nom.
    let live: HashSet<i64> = db::alerts::list_target_nodes(&state.pool)
        .await?
        .into_iter()
        .filter(|node| node.enabled)
        .map(|node| node.id)
        .collect();

    // Une lecture pour toutes les alertes. Un échec ne doit pas vider la liste :
    // les liens de tickets sont un confort, pas l'objet de la page.
    let mut tickets =
        crate::notify::tickets::links_by_fingerprint(&state.pool).await.unwrap_or_default();
    // Une lecture, elle aussi : les repères qui ont pu précéder un déclenchement.
    let changes = crate::annotations::changes_before_alerts(&state.pool, &alerts).await;

    Ok(Json(
        alerts
            .into_iter()
            .filter(|alert| match alert.target_id {
                Some(id) => live.contains(&id),
                None => !crate::alerting::model::carries_target_id(&alert.labels),
            })
            // Le filtre `target_id` ne garde que les alertes de cet équipement
            // (une alerte sans équipement n'y répond jamais).
            .filter(|alert| query.target_id.is_none_or(|id| alert.target_id == Some(id)))
            .filter(|alert| {
                visible
                    .as_ref()
                    .is_none_or(|visible| alert.target_id.is_some_and(|id| visible.contains(&id)))
            })
            .filter(|alert| alert.state.is_dismissed() == query.dismissed.unwrap_or(false))
            .map(|alert| {
                let rule = rules.get(&alert.rule_uid);
                let links = tickets.remove(&alert.fingerprint).unwrap_or_default();
                let after_change = changes.cited_by(&alert);
                ActiveAlertView {
                    tickets: links,
                    after_change,
                    ..ActiveAlertView::new(alert, rule, now)
                }
            })
            .collect(),
    ))
}

/// Pose ou lève l'acquittement d'une alerte, au nom de `actor`.
///
/// Partagé par la route HTTP et l'outil MCP : la validation de la saisie est
/// faite par l'appelant, la vérification du rôle par le garde de la route.
pub async fn acknowledge(
    state: &AppState,
    fingerprint: &str,
    request: AckRequest,
    actor: &str,
) -> ApiResult<ActiveAlertView> {
    let ack = match request {
        AckRequest::Set { until, note } => {
            Some(db::alerts::Ack { until, by: actor.to_string(), note })
        }
        AckRequest::Clear => None,
    };
    if !db::alerts::set_ack(&state.pool, fingerprint, ack.as_ref()).await? {
        return Err(alert_not_found(fingerprint));
    }
    if let Some(ack) = &ack {
        db::alert_insights::note_acknowledged(&state.pool, fingerprint).await;
        // Les tickets ouverts de l'alerte en gardent la trace, sans faire attendre
        // l'utilisateur derrière un service de tickets lent.
        let mut text =
            format!("Acknowledged by {} until {}.", ack.by, ack.until.format("%Y-%m-%d %H:%M UTC"));
        if let Some(note) = &ack.note {
            text.push_str(&format!(" Note: {note}"));
        }
        let state = state.clone();
        let fingerprint = fingerprint.to_string();
        tokio::spawn(async move {
            crate::notify::tickets::on_acknowledged(
                &state.pool,
                &state.cipher,
                &fingerprint,
                &text,
            )
            .await;
        });
    }
    let alert = db::alerts::get_active(&state.pool, fingerprint)
        .await?
        .ok_or_else(|| alert_not_found(fingerprint))?;
    let rule = db::alerts::list_rules(&state.pool)
        .await?
        .into_iter()
        .find(|rule| rule.uid == alert.rule_uid);
    Ok(ActiveAlertView::new(alert, rule.as_ref(), Utc::now()))
}

pub(super) fn alert_not_found(fingerprint: &str) -> ApiError {
    ApiError::NotFound(format!("No active alert with fingerprint \"{fingerprint}\"."))
}

/// `POST /alerts/{fingerprint}/ack` : « je sais, ne me le rappelle plus ».
pub async fn ack_alert(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(fingerprint): Path<String>,
    payload: Option<Json<AckPayload>>,
) -> ApiResult<Json<ActiveAlertView>> {
    let payload = payload.map(|Json(payload)| payload).unwrap_or_default();
    let request = payload.validate(Utc::now())?;
    let action = match request {
        AckRequest::Set { .. } => "alert.acked",
        AckRequest::Clear => "alert.unacked",
    };
    let actor = principal.label();
    let view = acknowledge(&state, &fingerprint, request, &actor).await?;
    audit::record(&state.pool, Some(&actor), action, Some(&fingerprint), ip).await;
    Ok(Json(view))
}

/// Écarte (« j'ai compris ») ou rétablit une alerte, au nom de `actor`.
///
/// Partagé par la route HTTP, l'outil MCP et les liens des notifications.
pub async fn dismiss(
    state: &AppState,
    fingerprint: &str,
    dismissed: bool,
    actor: &str,
) -> ApiResult<ActiveAlertView> {
    let by = dismissed.then_some(actor);
    if !db::alerts::set_dismiss(&state.pool, fingerprint, by, Utc::now()).await? {
        return Err(alert_not_found(fingerprint));
    }
    if dismissed {
        db::alert_insights::note_acknowledged(&state.pool, fingerprint).await;
    }
    let alert = db::alerts::get_active(&state.pool, fingerprint)
        .await?
        .ok_or_else(|| alert_not_found(fingerprint))?;
    let rule = db::alerts::list_rules(&state.pool)
        .await?
        .into_iter()
        .find(|rule| rule.uid == alert.rule_uid);
    Ok(ActiveAlertView::new(alert, rule.as_ref(), Utc::now()))
}

/// `POST /alerts/{fingerprint}/dismiss` : « j'ai compris » — l'occurrence en
/// cours disparaît des vues actives et ne notifie plus rien.
pub async fn dismiss_alert(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(fingerprint): Path<String>,
) -> ApiResult<Json<ActiveAlertView>> {
    let actor = principal.label();
    let view = dismiss(&state, &fingerprint, true, &actor).await?;
    audit::record(&state.pool, Some(&actor), "alert.dismissed", Some(&fingerprint), ip).await;
    Ok(Json(view))
}

/// `DELETE /alerts/{fingerprint}/dismiss` : rétablit une alerte écartée par erreur.
pub async fn undismiss_alert(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(fingerprint): Path<String>,
) -> ApiResult<Json<ActiveAlertView>> {
    let actor = principal.label();
    let view = dismiss(&state, &fingerprint, false, &actor).await?;
    audit::record(&state.pool, Some(&actor), "alert.undismissed", Some(&fingerprint), ip).await;
    Ok(Json(view))
}

/// `DELETE /alerts/{fingerprint}/ack` : lève l'acquittement.
pub async fn unack_alert(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(fingerprint): Path<String>,
) -> ApiResult<Json<ActiveAlertView>> {
    let actor = principal.label();
    let view = acknowledge(&state, &fingerprint, AckRequest::Clear, &actor).await?;
    audit::record(&state.pool, Some(&actor), "alert.unacked", Some(&fingerprint), ip).await;
    Ok(Json(view))
}

/// Historique des transitions, de la plus récente à la plus ancienne.
///
/// La lecture est écrite ici et non dans [`crate::db::alerts`] : ce module n'expose
/// que les accès dont le moteur a besoin, et l'historique n'est lu que par l'API.
pub async fn history(
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Query(params): Query<HistoryQuery>,
) -> ApiResult<Json<Vec<HistoryEntryView>>> {
    let since = match params.since.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(raw) => {
            DateTime::parse_from_rfc3339(raw).map(|at| at.with_timezone(&Utc)).map_err(|_| {
                ApiError::BadRequest(format!(
                    "\"since\" must be an RFC 3339 date, for example \
                     2026-08-31T12:00:00Z (received: \"{raw}\")."
                ))
            })?
        }
        None => Utc::now() - TimeDelta::days(DEFAULT_HISTORY_DAYS),
    };

    let limit = match params.limit {
        Some(value) if value <= 0 => {
            return Err(ApiError::BadRequest(
                "\"limit\" must be a strictly positive integer.".into(),
            ));
        }
        Some(value) => value.min(MAX_HISTORY_LIMIT),
        None => DEFAULT_HISTORY_LIMIT,
    };

    let include_dismissed = params.dismissed.unwrap_or(false);

    // Compte restreint : le filtre est posé dans la requête, avant la limite,
    // pour que la page reste pleine. Les identifiants sont des entiers lus en
    // base, jamais une saisie.
    let scope_clause = match access.visible_targets(&state.pool).await? {
        None => String::new(),
        Some(visible) => {
            let ids = visible.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
            format!(" AND target_id IN ({ids})")
        }
    };
    let sql = format!(
        "SELECT id, fingerprint, rule_uid, target_id, from_phase, to_phase, severity, value,
                notified, reason, at, dismissed
         FROM alert_history
         WHERE at >= ? AND (dismissed = 0 OR ?){scope_clause}
         ORDER BY at DESC, id DESC
         LIMIT ?"
    );
    let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(since.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .bind(include_dismissed)
        .bind(limit)
        .fetch_all(&state.pool)
        .await
        .map_err(anyhow::Error::from)?;

    let entries = rows
        .iter()
        .map(|row| {
            let from_phase: String = row.try_get("from_phase")?;
            let to_phase: String = row.try_get("to_phase")?;
            let severity: String = row.try_get("severity")?;
            Ok(HistoryEntryView {
                id: row.try_get("id")?,
                fingerprint: row.try_get("fingerprint")?,
                rule_uid: row.try_get("rule_uid")?,
                target_id: row.try_get("target_id")?,
                from_phase: Phase::parse(&from_phase),
                to_phase: Phase::parse(&to_phase),
                severity: Severity::parse(&severity),
                value: row.try_get("value")?,
                notified: row.try_get::<i64, _>("notified")? != 0,
                reason: row.try_get("reason")?,
                at: row.try_get("at")?,
                dismissed: row.try_get::<i64, _>("dismissed")? != 0,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(anyhow::Error::from)?;

    Ok(Json(entries))
}

/// `POST /alerts/history/{id}/dismiss` : « j'ai vu, dégage-le des vues actives ».
///
/// Seule une transition vers `resolved` peut être effacée — c'est la seule que
/// cette fonctionnalité concerne ; une transition encore active n'a pas de sens
/// à faire disparaître puisque l'alerte, elle, est toujours là. La ligne reste
/// en base : c'est la mémoire de l'instance, et seule une suppression explicite
/// (non proposée ici) l'effacerait pour de bon.
pub async fn dismiss_history_entry(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
    Path(id): Path<i64>,
) -> ApiResult<StatusCode> {
    let updated = sqlx::query(
        "UPDATE alert_history SET dismissed = 1 WHERE id = ? AND to_phase = 'resolved'",
    )
    .bind(id)
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    if updated.rows_affected() == 0 {
        return Err(ApiError::NotFound(format!("No resolved history entry {id} to clear.")));
    }
    let actor = principal.label();
    audit::record(&state.pool, Some(&actor), "alert_history.dismissed", Some(&id.to_string()), ip)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// Combien de lignes une purge « Clear all resolved » a effacées des vues actives.
#[derive(Debug, Serialize)]
pub struct DismissedCount {
    pub dismissed: u64,
}

/// `POST /alerts/history/dismiss-resolved` : « j'ai tout vu, nettoie ».
pub async fn dismiss_resolved_history(
    State(state): State<AppState>,
    OperatorIdentity(principal): OperatorIdentity,
    ClientIp(ip): ClientIp,
) -> ApiResult<Json<DismissedCount>> {
    let updated = sqlx::query(
        "UPDATE alert_history SET dismissed = 1 WHERE to_phase = 'resolved' AND dismissed = 0",
    )
    .execute(&state.pool)
    .await
    .map_err(anyhow::Error::from)?;
    let count = updated.rows_affected();
    let actor = principal.label();
    audit::record(
        &state.pool,
        Some(&actor),
        "alert_history.dismissed_resolved",
        Some(&count.to_string()),
        ip,
    )
    .await;
    Ok(Json(DismissedCount { dismissed: count }))
}
