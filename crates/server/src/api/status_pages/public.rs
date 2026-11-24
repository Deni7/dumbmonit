//! Document public d'une page : types, cache, requêtes de métriques et état des services.

use super::*;

// --------------------------------------------------------------------------
// Document public
// --------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub(super) struct PublicStatus {
    pub(super) page: PublicPage,
    /// `operational`, `degraded`, `major` ou `maintenance`.
    pub(super) overall: &'static str,
    pub(super) generated_at: String,
    pub(super) groups: Vec<PublicGroup>,
    /// Calendrier de 90 jours ; absent quand la page ne le montre pas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) calendar: Option<Vec<CalendarDay>>,
    /// Incidents en cours et ceux des trente derniers jours.
    pub(super) incidents: Vec<PublicIncident>,
    /// Maintenances prévues ou en cours.
    pub(super) maintenance: Vec<PublicIncident>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicPage {
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) theme: String,
    /// Réglage de la page : la fenêtre la plus longue qu'elle montre.
    pub(super) show_uptime_days: i64,
    /// Fenêtre réellement montrée : l'âge des données, de 7 jours au réglage.
    pub(super) history_days: i64,
    /// Durée réellement affichée par la frise, en secondes : la période choisie,
    /// ou moins quand les données sont plus courtes.
    pub(super) window_secs: i64,
    /// Durée d'une barre de la frise, en secondes (86 400 : une par jour).
    pub(super) step_secs: i64,
    pub(super) updated_at: String,
    /// Teinte d'accent choisie parmi [`ACCENTS`].
    pub(super) accent: String,
    /// Scènes de la bannière, dans l'ordre, parmi [`SCENES`] ; vide : aucune.
    pub(super) scenes: Vec<String>,
    /// Rythme de changement de scène, parmi [`SCENE_ROTATIONS`].
    pub(super) scene_rotation: String,
    /// Mode simple : le public n'affiche ni scène, ni pigeon, ni animation.
    pub(super) simple: bool,
    /// Présents seulement quand l'administrateur les a activés.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) show_calendar: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) show_latency: bool,
    pub(super) footer_text: String,
    /// Site de l'organisation, vide si la page n'en cite pas.
    pub(super) homepage_url: String,
    /// Adresse relative du logo, versionnée pour les caches ; `None` sans logo.
    /// Mode MSP : à défaut de logo propre, celui du client de la page.
    pub(super) logo_url: Option<String>,
    /// Mode MSP : nom du client de la page, pour la marque légère.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) brand_name: Option<String>,
    /// La page propose au moins un abonnement.
    pub(super) subscribe: bool,
    /// Abonnements ouverts : `email`, `sms`, `webhook`, `slack`.
    pub(super) subscribe_kinds: Vec<&'static str>,
    /// Valeurs de référencement effectives (réglages, ou défauts tirés du nom).
    pub(super) seo: crate::status_seo::Resolved,
    /// Marque blanche : l'interface ne montre ni l'icône ni le nom de DumbMonit.
    pub(super) hide_branding: bool,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicGroup {
    pub(super) name: String,
    pub(super) items: Vec<PublicItem>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicItem {
    /// Clé stable du service dans les adresses publiques (badges), dérivée de
    /// son libellé public — jamais de l'identifiant interne.
    pub(super) key: String,
    /// Aperçu d'administration seulement : la surveillance du service, pour que
    /// l'éditeur retrouve ses mesures. Jamais dans le document public.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) target_id: Option<TargetId>,
    pub(super) label: String,
    /// `up`, `down`, `degraded`, `maintenance` ou `unknown`.
    pub(super) state: &'static str,
    pub(super) uptime_24h: Option<f64>,
    pub(super) uptime_7d: Option<f64>,
    pub(super) uptime_30d: Option<f64>,
    /// Seulement quand la page montre 90 jours d'historique.
    pub(super) uptime_90d: Option<f64>,
    /// Sur les `history_days` jours des barres : le chiffre affiché à côté
    /// d'elles doit couvrir les mêmes jours.
    pub(super) uptime_history: Option<f64>,
    pub(super) latency_ms: Option<f64>,
    /// Temps de réponse agrégés ; seulement si la page les montre et que le
    /// service en a.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) latency_series: Option<LatencySeries>,
    pub(super) history: Vec<DayBucket>,
    /// Fournisseur tiers (sonde `provider`) : la page l'affiche comme composant externe.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) external: bool,
}

/// Points `[horodatage (s), millisecondes]`, moyennés par tranche.
#[derive(Debug, Serialize)]
pub(super) struct LatencySeries {
    pub(super) h24: Vec<[f64; 2]>,
    pub(super) d7: Vec<[f64; 2]>,
}

/// Un jour du calendrier de la page, tous services confondus.
#[derive(Debug, Serialize)]
pub(super) struct CalendarDay {
    pub(super) date: String,
    /// `operational`, `degraded`, `outage` ou `nodata`.
    pub(super) state: &'static str,
    /// Disponibilité du service le moins disponible ce jour-là.
    pub(super) uptime_pct: Option<f64>,
    pub(super) incidents: u32,
}

/// État d'un jour : le plus mauvais des services mesurés ; un incident annoncé
/// ce jour-là suffit à ne pas le dire parfait.
pub(super) fn calendar_state(worst_uptime: Option<f64>, incidents: u32) -> &'static str {
    match worst_uptime {
        None => "nodata",
        Some(pct) if pct < 95.0 => "outage",
        Some(pct) if pct < 99.9 || incidents > 0 => "degraded",
        Some(_) => "operational",
    }
}

#[derive(Debug, Serialize)]
pub(super) struct DayBucket {
    pub(super) date: String,
    pub(super) uptime_pct: Option<f64>,
    /// Minutes d'indisponibilité mesurées ce jour-là ; `None` sans mesure.
    pub(super) down_minutes: Option<u32>,
    pub(super) incidents: u32,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicIncident {
    /// Ancre de l'annonce (`#incident-<anchor>`), lien permanent y compris dans
    /// le flux RSS : début et titre en slug, jamais l'identifiant interne.
    pub(super) anchor: String,
    pub(super) title: String,
    pub(super) kind: String,
    pub(super) status: String,
    pub(super) severity: String,
    pub(super) starts_at: String,
    pub(super) ends_at: Option<String>,
    pub(super) updates: Vec<PublicUpdate>,
    /// Clés publiques des services touchés ; vide : toute la page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(super) components: Vec<String>,
    /// Post-mortem publié d'un incident résolu ; absent sinon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) postmortem: Option<postmortem::PublicPostmortem>,
}

#[derive(Debug, Serialize)]
pub(super) struct PublicUpdate {
    pub(super) status: String,
    pub(super) body: String,
    pub(super) created_at: String,
}

pub(super) struct CachedStatus {
    pub(super) at: Instant,
    /// Horodatage de la page au moment du calcul : une modification invalide
    /// l'entrée même si le cache n'a pas été vidé.
    pub(super) page_updated_at: String,
    pub(super) body: Arc<Value>,
}

pub(super) static CACHE: LazyLock<Mutex<HashMap<String, CachedStatus>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Vide le cache public. Appelé après toute écriture d'administration, pour que
/// l'annonce d'un incident soit visible sans attendre.
pub(crate) fn invalidate_cache() {
    CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clear();
    super::discovery::invalidate();
    // Le domaine ou le slug d'une page a pu changer.
    crate::status_host::invalidate();
}

pub(super) fn public_not_found() -> ApiError {
    ApiError::NotFound("This status page does not exist.".into())
}

/// Document public d'une page, depuis le cache ou recalculé, après contrôle
/// d'accès. Le booléen dit si la page est privée : ses réponses ne doivent
/// alors se mettre en cache ni chez un intermédiaire ni chez un tiers.
pub(super) async fn load_public_checked(
    state: &AppState,
    slug: &str,
    headers: &HeaderMap,
) -> ApiResult<(Arc<Value>, bool)> {
    let page = db::status_pages::get_page_by_slug(&state.pool, slug).await?;
    // Une page non publiée est indiscernable d'une page absente : le public n'a
    // pas à savoir qu'elle se prépare.
    let page = match page {
        Some(page) if page.published => page,
        _ => return Err(public_not_found()),
    };
    // Le contrôle précède le cache : un document en mémoire ne se sert qu'à qui y a droit.
    access::authorize(state, &page, headers).await?;
    let private = !page.is_public();

    {
        let cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = cache.get(slug)
            && entry.at.elapsed() < CACHE_TTL
            && entry.page_updated_at == page.updated_at
        {
            return Ok((Arc::clone(&entry.body), private));
        }
    }

    let status = build_public(state, &page, false).await?;
    let body = Arc::new(serde_json::to_value(&status)?);
    let mut cache = CACHE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    cache.insert(
        slug.to_string(),
        CachedStatus {
            at: Instant::now(),
            page_updated_at: page.updated_at,
            body: Arc::clone(&body),
        },
    );
    Ok((body, private))
}

/// `?lang=` : la langue de l'interface du visiteur, pour les textes traduits.
#[derive(Debug, Deserialize)]
pub struct LangQuery {
    #[serde(default)]
    pub(super) lang: Option<String>,
}

pub async fn public_status(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<LangQuery>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let (body, private) = load_public_checked(&state, &slug, &headers).await?;
    let body = lang::localise(&state, &slug, &headers, query.lang.as_deref(), &body).await?;
    // Document public et sans cookie : lisible depuis le site de n'importe qui
    // (bandeau d'état). Pas de `Allow-Credentials`, donc aucune session jointe.
    Ok(access::seal(
        (
            [
                (header::CACHE_CONTROL, "public, max-age=30"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
                (header::VARY, "Accept-Language"),
            ],
            Json(body),
        )
            .into_response(),
        private,
    ))
}

/// Script du bandeau d'état, à inclure par un `<script>` sur un site tiers.
/// Statique : il déduit l'adresse du document de sa propre URL.
pub(super) async fn public_banner_js() -> Response {
    (
        [
            (header::CONTENT_TYPE, "application/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "public, max-age=3600"),
            (HeaderName::from_static("cross-origin-resource-policy"), "cross-origin"),
        ],
        include_str!("../../../assets/banner.js"),
    )
        .into_response()
}

/// Où en est chaque cible, lu dans VictoriaMetrics. Vide si la base de séries
/// ne répond pas : la page s'affiche alors sans chiffres plutôt que pas du tout.
#[derive(Default)]
pub(super) struct Metrics {
    /// Dernier `probe_success` (sondes seulement).
    pub(super) probe_last: HashMap<TargetId, f64>,
    /// Disponibilité sur la dernière heure, en pour cent (sondes seulement).
    pub(super) probe_recent: HashMap<TargetId, f64>,
    pub(super) latency_ms: HashMap<TargetId, f64>,
    pub(super) uptime_24h: HashMap<TargetId, f64>,
    pub(super) uptime_7d: HashMap<TargetId, f64>,
    pub(super) uptime_30d: HashMap<TargetId, f64>,
    pub(super) uptime_90d: HashMap<TargetId, f64>,
    /// Disponibilité sur la fenêtre des barres (`history_days`).
    pub(super) uptime_history: HashMap<TargetId, f64>,
    /// Par cible, disponibilité de chaque jour indexée par début de jour (s).
    pub(super) daily: HashMap<TargetId, HashMap<i64, f64>>,
    /// Par cible, minutes d'indisponibilité de chaque jour, même index.
    pub(super) daily_down: HashMap<TargetId, HashMap<i64, f64>>,
    /// Par sonde, temps de réponse moyen par quart d'heure sur 24 h.
    pub(super) latency_24h: HashMap<TargetId, Vec<[f64; 2]>>,
    /// Par sonde, temps de réponse moyen par tranche de deux heures sur 7 jours.
    pub(super) latency_7d: HashMap<TargetId, Vec<[f64; 2]>>,
}

pub(crate) fn target_of(metric: &BTreeMap<String, String>) -> Option<TargetId> {
    metric.get("target")?.parse().ok()
}

pub(crate) fn instant_map(series: Vec<InstantSeries>) -> HashMap<TargetId, f64> {
    series
        .into_iter()
        .filter_map(|item| {
            let id = target_of(&item.metric)?;
            let value: f64 = item.value.1.parse().ok()?;
            value.is_finite().then_some((id, value))
        })
        .collect()
}

pub(crate) fn selector(ids: &[TargetId]) -> String {
    let joined: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
    format!("target=~\"{}\"", joined.join("|"))
}

/// Part (en %) des sondes réussies de chaque cible sur `window`, décalée de
/// `offset` (vide pour « maintenant »).
///
/// Comme pour `up` (voir [`presence_slots`]), une cible dont les étiquettes ont
/// changé a plusieurs séries `probe_success` : la somme des succès sur le
/// nombre de mesures, par cible, les compte toutes, chacune avec son poids.
pub(crate) fn probe_uptime_query(sel: &str, window: &str, offset: &str) -> String {
    let range = format!("dumbmonit_probe_success{{{sel}}}[{window}]{offset}");
    format!(
        "sum by (target) (sum_over_time({range})) / sum by (target) (count_over_time({range})) * 100"
    )
}

/// Sondes échouées de chaque cible sur `window`, toutes séries confondues :
/// nombre de mesures moins leur somme (1 par succès).
pub(super) fn probe_failed_query(sel: &str, window: &str) -> String {
    let range = format!("dumbmonit_probe_success{{{sel}}}[{window}]");
    format!("sum by (target) (count_over_time({range})) - sum by (target) (sum_over_time({range}))")
}

/// Temps de réponse moyen (ms) de chaque cible sur `window`, toutes séries
/// confondues : après un changement d'étiquettes, l'ancienne série couvre les
/// jours d'avant et la nouvelle ceux d'après ; sans `avg by (target)`, une seule
/// était gardée et la courbe sur 7 jours perdait l'une des deux parties.
pub(super) fn latency_query(sel: &str, window: &str) -> String {
    format!(
        "avg by (target) (avg_over_time(dumbmonit_probe_duration_seconds{{{sel}}}[{window}])) * 1000"
    )
}

/// Tranches de cinq minutes vivantes (1) ou muettes (0) d'une cible qui n'émet
/// que `up` en cas de succès, sur `window`.
///
/// `count_over_time` par tranche dit si la cible a répondu au moins une fois ;
/// `default 0` compte les tranches muettes comme des absences.
///
/// Une cible peut avoir plusieurs séries `up` : toute modification de ses
/// étiquettes d'identité (nom, tags, ou en 0.1.0-alpha.7 les options du type
/// retirées des `tag_*`) ouvre une nouvelle série et fige l'ancienne. Le
/// `sum by (target)` les recoud en une seule ; sans lui, une seule des séries
/// était retenue et l'historique d'avant le changement disparaissait.
pub(crate) fn presence_slots(sel: &str, window: &str) -> String {
    format!(
        "((sum by (target) (count_over_time(dumbmonit_up{{{sel}}}[5m])) > bool 0) default 0)[{window}:5m]"
    )
}

/// Part (en %) des tranches vivantes sur `window` (voir [`presence_slots`]).
pub(super) fn presence_query(sel: &str, window: &str) -> String {
    format!("avg_over_time({}) * 100", presence_slots(sel, window))
}

/// Horodatage de la première mesure `up` de chaque cible, toutes séries
/// confondues (voir [`presence_slots`]) : c'est le début de la couverture.
pub(crate) fn first_up_query(sel: &str, window: &str) -> String {
    format!("min by (target) (tfirst_over_time(dumbmonit_up{{{sel}}}[{window}]))")
}

/// Horodatage de la dernière mesure `up` de chaque cible, toutes séries confondues.
pub(super) fn last_up_query(sel: &str, window: &str) -> String {
    format!("max by (target) (tlast_over_time(dumbmonit_up{{{sel}}}[{window}]))")
}

/// Ramène une mesure de présence à la part de la fenêtre réellement couverte
/// par des données : avant la première mesure, `default 0` compte des absences
/// qui n'en sont pas.
pub(crate) fn rescale(measured: f64, window_secs: f64, covered_secs: f64) -> Option<f64> {
    // Moins d'une tranche de présence (5 min) : la mesure ne repose que sur une
    // ou deux tranches, le quotient n'est que du bruit de quantification. Mieux
    // vaut « pas de données » qu'un pourcentage fantaisiste.
    if covered_secs < MIN_COVERED_SECS || window_secs <= 0.0 {
        return None;
    }
    Some((measured * window_secs / covered_secs).clamp(0.0, 100.0))
}

/// Durée minimale de données pour publier une disponibilité (une tranche).
pub(super) const MIN_COVERED_SECS: f64 = 300.0;

/// Secondes de la fenêtre `[start, end]` réellement couvertes par des données.
///
/// La couverture commence à la première mesure et s'arrête à « maintenant ».
/// Un appareil inactif (désactivé, ou jamais sondé) ne mesure plus rien : sa
/// couverture s'arrête à sa dernière mesure (`inactive_last` = `Some(dernière
/// mesure)`), sinon la pause serait comptée comme une panne. Un appareil actif
/// injoignable garde l'absence de données comme panne.
pub(super) fn device_covered(
    start: f64,
    end: f64,
    first: f64,
    inactive_last: Option<Option<f64>>,
    now: f64,
) -> f64 {
    let mut stop = end.min(now);
    if let Some(last) = inactive_last {
        // Sans aucune mesure connue, rien n'est couvert.
        stop = stop.min(last.unwrap_or(f64::NEG_INFINITY));
    }
    stop - start.max(first)
}

/// Séries de temps de réponse (ms) des sondes, agrégées côté serveur : le public
/// ne reçoit que des points, jamais la requête.
pub(super) async fn collect_latency(
    victoria: &Victoria,
    probes: &[TargetId],
    now: DateTime<Utc>,
    metrics: &mut Metrics,
) {
    if probes.is_empty() {
        return;
    }
    let sel = selector(probes);
    let end_ms = now.timestamp() * 1_000;
    let spans: [(i64, u64, &str); 2] = [(86_400, 900, "15m"), (7 * 86_400, 7_200, "2h")];
    for (index, (span, step, window)) in spans.into_iter().enumerate() {
        let query = latency_query(&sel, window);
        match victoria.query_range(&query, end_ms - span * 1_000, end_ms, step).await {
            Ok(series) => {
                for item in series {
                    let Some(id) = target_of(&item.metric) else { continue };
                    let points: Vec<[f64; 2]> = item
                        .values
                        .into_iter()
                        .filter_map(|(ts, raw)| {
                            let value: f64 = raw.parse().ok()?;
                            value.is_finite().then(|| [ts, round(value)])
                        })
                        .collect();
                    if points.is_empty() {
                        continue;
                    }
                    if index == 0 {
                        metrics.latency_24h.insert(id, points);
                    } else {
                        metrics.latency_7d.insert(id, points);
                    }
                }
            }
            Err(error) => log_query_failure(&error, "status page: latency metrics unavailable"),
        }
    }
}

pub(super) async fn collect_metrics(
    victoria: &Victoria,
    targets: &[&Target],
    days: i64,
    history_days: i64,
    now: DateTime<Utc>,
    with_latency: bool,
    inactive: &HashSet<TargetId>,
) -> Metrics {
    let probes: Vec<TargetId> =
        targets.iter().filter(|t| PROBE_KINDS.contains(&t.kind.as_str())).map(|t| t.id).collect();
    let devices: Vec<TargetId> =
        targets.iter().filter(|t| !PROBE_KINDS.contains(&t.kind.as_str())).map(|t| t.id).collect();

    // Bornes des jours affichés : de minuit (UTC) du premier jour à minuit du
    // lendemain d'aujourd'hui. Évaluée en fin de jour, la fenêtre `[1d]` couvre
    // exactement le jour écoulé.
    let now_secs = now.timestamp();
    let today_start = now_secs.div_euclid(86_400) * 86_400;
    let first_day = today_start - (days - 1) * 86_400;
    let range_start_ms = (first_day + 86_400) * 1_000;
    let range_end_ms = (today_start + 86_400) * 1_000;

    let mut metrics = Metrics::default();
    if with_latency {
        collect_latency(victoria, &probes, now, &mut metrics).await;
    }

    if !probes.is_empty() {
        let sel = selector(&probes);
        let q_last = format!("last_over_time(dumbmonit_probe_success{{{sel}}}[{STATE_WINDOW}])");
        let q_recent = probe_uptime_query(&sel, "1h", "");
        let q_day = probe_uptime_query(&sel, "24h", "");
        let q_week = probe_uptime_query(&sel, "7d", "");
        let q_month = probe_uptime_query(&sel, "30d", "");
        let q_failed = probe_failed_query(&sel, "1d");
        let q_quarter = probe_uptime_query(&sel, "90d", "");
        let q_history = probe_uptime_query(&sel, &format!("{history_days}d"), "");
        let q_latency = latency_query(&sel, "1h");
        let q_daily = probe_uptime_query(&sel, "1d", "");
        let result = tokio::try_join!(
            victoria.query(&q_last),
            victoria.query(&q_recent),
            victoria.query(&q_day),
            victoria.query(&q_week),
            victoria.query(&q_month),
            victoria.query(&q_quarter),
            victoria.query(&q_history),
            victoria.query(&q_latency),
            victoria.query_range(&q_daily, range_start_ms, range_end_ms, 86_400),
            victoria.query_range(&q_failed, range_start_ms, range_end_ms, 86_400),
        );
        match result {
            Ok((last, recent, day, week, month, quarter, history, latency, daily, failed)) => {
                metrics.probe_last = instant_map(last);
                metrics.probe_recent = instant_map(recent);
                metrics.uptime_24h = instant_map(day);
                metrics.uptime_7d = instant_map(week);
                metrics.uptime_30d = instant_map(month);
                metrics.uptime_90d = instant_map(quarter);
                metrics.uptime_history = instant_map(history);
                // Une mesure manquée vaut l'intervalle de la sonde : c'est la
                // durée pendant laquelle le service est resté réputé en panne.
                let intervals: HashMap<TargetId, f64> =
                    targets.iter().map(|t| (t.id, t.interval.as_secs_f64().max(1.0))).collect();
                for series in failed {
                    let Some(id) = target_of(&series.metric) else { continue };
                    let interval = intervals.get(&id).copied().unwrap_or(60.0);
                    let entry = metrics.daily_down.entry(id).or_default();
                    for (ts, raw) in series.values {
                        if let Ok(count) = raw.parse::<f64>()
                            && count.is_finite()
                        {
                            let minutes = (count.max(0.0) * interval / 60.0).min(1_440.0);
                            entry.insert(ts as i64 - 86_400, minutes);
                        }
                    }
                }
                metrics.latency_ms = instant_map(latency);
                for series in daily {
                    let Some(id) = target_of(&series.metric) else { continue };
                    let entry = metrics.daily.entry(id).or_default();
                    for (ts, raw) in series.values {
                        if let Ok(value) = raw.parse::<f64>()
                            && value.is_finite()
                        {
                            entry.insert(ts as i64 - 86_400, value.min(100.0));
                        }
                    }
                }
            }
            Err(error) => log_query_failure(&error, "status page: probe metrics unavailable"),
        }
    }

    if !devices.is_empty() {
        let sel = selector(&devices);
        let q_first = first_up_query(&sel, "90d");
        let q_last = last_up_query(&sel, "90d");
        let q_day = presence_query(&sel, "24h");
        let q_week = presence_query(&sel, "7d");
        let q_month = presence_query(&sel, "30d");
        let q_quarter = presence_query(&sel, "90d");
        let q_history = presence_query(&sel, &format!("{history_days}d"));
        let q_daily = presence_query(&sel, "1d");
        let result = tokio::try_join!(
            victoria.query(&q_first),
            victoria.query(&q_last),
            victoria.query(&q_day),
            victoria.query(&q_week),
            victoria.query(&q_month),
            victoria.query(&q_quarter),
            victoria.query(&q_history),
            victoria.query_range(&q_daily, range_start_ms, range_end_ms, 86_400),
        );
        match result {
            Ok((first, last, day, week, month, quarter, history, daily)) => {
                let first = instant_map(first);
                let last = instant_map(last);
                let windows: [(&str, Vec<InstantSeries>, f64); 5] = [
                    ("24h", day, 86_400.0),
                    ("7d", week, 7.0 * 86_400.0),
                    ("30d", month, 30.0 * 86_400.0),
                    ("90d", quarter, 90.0 * 86_400.0),
                    ("history", history, history_days as f64 * 86_400.0),
                ];
                for (name, series, window_secs) in windows {
                    for (id, measured) in instant_map(series) {
                        let Some(first_ts) = first.get(&id) else { continue };
                        let covered = device_covered(
                            now_secs as f64 - window_secs,
                            now_secs as f64,
                            *first_ts,
                            inactive.contains(&id).then(|| last.get(&id).copied()),
                            now_secs as f64,
                        );
                        let Some(value) = rescale(measured, window_secs, covered) else { continue };
                        let bucket = match name {
                            "24h" => &mut metrics.uptime_24h,
                            "7d" => &mut metrics.uptime_7d,
                            "30d" => &mut metrics.uptime_30d,
                            "90d" => &mut metrics.uptime_90d,
                            _ => &mut metrics.uptime_history,
                        };
                        bucket.insert(id, value);
                    }
                }
                for series in daily {
                    let Some(id) = target_of(&series.metric) else { continue };
                    let Some(first_ts) = first.get(&id).copied() else { continue };
                    let entry = metrics.daily.entry(id).or_default();
                    for (ts, raw) in series.values {
                        let Ok(measured) = raw.parse::<f64>() else { continue };
                        let day_start = ts as i64 - 86_400;
                        let day_end = ts as i64;
                        let covered = device_covered(
                            day_start as f64,
                            day_end as f64,
                            first_ts,
                            inactive.contains(&id).then(|| last.get(&id).copied()),
                            now_secs as f64,
                        );
                        if let Some(value) = rescale(measured, 86_400.0, covered) {
                            entry.insert(day_start, value);
                            // Tranches de cinq minutes muettes sur la part couverte.
                            let minutes = (100.0 - value).max(0.0) / 100.0 * covered / 60.0;
                            metrics
                                .daily_down
                                .entry(id)
                                .or_default()
                                .insert(day_start, minutes.min(1_440.0));
                        }
                    }
                }
            }
            Err(error) => log_query_failure(&error, "status page: device metrics unavailable"),
        }
    }

    metrics
}

/// État d'un service, calqué sur ce que l'interface affiche pour la cible.
pub(super) fn item_state(
    target: &Target,
    status: Option<&TargetStatus>,
    metrics: &Metrics,
    now: DateTime<Utc>,
) -> &'static str {
    if !target.enabled {
        return "unknown";
    }
    if status.and_then(|s| s.last_error.as_deref()).is_some_and(|e| !e.is_empty()) {
        return "down";
    }
    if PROBE_KINDS.contains(&target.kind.as_str()) {
        return match metrics.probe_last.get(&target.id) {
            None => "unknown",
            Some(value) if *value >= 1.0 => {
                // Un service qui répond mais a échoué dans l'heure est dégradé :
                // c'est le mot juste pour une panne intermittente.
                if metrics.probe_recent.get(&target.id).is_some_and(|recent| *recent < 100.0) {
                    "degraded"
                } else {
                    "up"
                }
            }
            Some(_) => "down",
        };
    }
    let Some(last) = status.and_then(|s| s.last_probe_at.as_deref()).and_then(parse_stored) else {
        return "unknown";
    };
    let tolerance = TimeDelta::seconds((target.interval.as_secs() as i64 * 3).max(90));
    if now - last > tolerance { "down" } else { "up" }
}

pub(super) fn to_public_incident(
    incident: Incident,
    updates: Vec<IncidentUpdate>,
    postmortem: Option<Postmortem>,
    components: Vec<String>,
) -> PublicIncident {
    let postmortem = postmortem::public(postmortem, &incident.status);
    PublicIncident {
        anchor: incident_anchor(&incident.starts_at, &incident.title),
        title: incident.title,
        kind: incident.kind,
        status: incident.status,
        severity: incident.severity,
        starts_at: incident.starts_at,
        ends_at: incident.ends_at,
        updates: updates
            .into_iter()
            .map(|update| PublicUpdate {
                status: update.status,
                body: update.body,
                created_at: update.created_at,
            })
            .collect(),
        components,
        postmortem,
    }
}

/// Ancre publique d'une annonce : `AAAAMMJJ-HHMM-<titre en slug>` (titre coupé à
/// 40 caractères). Stable tant que le titre ne change pas.
pub(super) fn incident_anchor(starts_at: &str, title: &str) -> String {
    let stamp: String = starts_at.chars().filter(char::is_ascii_digit).take(12).collect();
    let stamp = if stamp.len() > 8 { format!("{}-{}", &stamp[..8], &stamp[8..]) } else { stamp };
    let slug: String = slugify(title).chars().take(40).collect();
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() { stamp } else { format!("{stamp}-{slug}") }
}

/// Clé publique d'un service : son libellé en slug, suffixée en cas de doublon.
pub(super) fn unique_key(label: &str, taken: &mut HashSet<String>) -> String {
    let base = match slugify(label) {
        base if base.is_empty() => "service".to_string(),
        base => base,
    };
    let mut key = base.clone();
    let mut n = 2;
    while !taken.insert(key.clone()) {
        key = format!("{base}-{n}");
        n += 1;
    }
    key
}

/// Étiquette de version tirée d'un horodatage : chiffres seulement.
pub(super) fn version_tag(stamp: &str) -> String {
    stamp.chars().filter(char::is_ascii_digit).collect()
}

pub(super) fn round(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
