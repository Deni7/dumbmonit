//! Modification groupée des moniteurs (`PATCH /api/targets/bulk`) et canaux de
//! notification propres à un moniteur (migration 0282).
//!
//! Une seule requête pour une sélection : période, canaux (remplacer, ajouter,
//! retirer), étiquettes libres (ajouter, retirer) et dossier. Chaque moniteur
//! est vérifié comme le serait sa modification seule — existence, droits
//! d'équipe, gestion par étiquettes ou par fichier, client des canaux — et
//! reçoit son propre verdict. Ceux qui passent sont écrits ensemble, dans une
//! seule transaction : la base ne garde jamais une sélection à moitié modifiée.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::Context;
use axum::extract::State;
use axum::routing::patch;
use axum::{Json, Router};
use dumbmonit_proto::TargetId;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::api::targets::{TargetPayload, TargetView};
use crate::api::{ApiError, ApiResult};
use crate::auth::audit;
use crate::auth::client_ip::ClientIp;
use crate::auth::clients;
use crate::auth::middleware::Identity;
use crate::auth::teams::{Level, TeamAccess};
use crate::db;
use crate::state::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/targets/bulk", patch(apply))
}

/// Mêmes bornes que la modification d'un seul moniteur (`api/targets.rs`).
const MIN_INTERVAL_SECS: u64 = 10;
const MAX_INTERVAL_SECS: u64 = 7 * 24 * 3600;
const MAX_GROUP_NAME_CHARS: usize = 60;
/// Une sélection plus grande que la flotte de l'instance la plus fournie.
const MAX_IDS: usize = 20_000;
const MAX_TAG_KEY_CHARS: usize = 64;
const MAX_TAG_VALUE_CHARS: usize = 200;

/// Corps de `POST /api/targets` et `PUT /api/targets/{id}` : la cible et,
/// à part, ses canaux. Absents à la modification : inchangés ; `[]` : ceux
/// de ses règles, comme avant.
#[derive(Debug, Deserialize)]
pub struct WithChannels {
    #[serde(flatten)]
    pub target: TargetPayload,
    #[serde(default)]
    pub channels: Option<Vec<i64>>,
}

impl TargetView {
    pub(super) fn with_channels(mut self, channels: Vec<i64>) -> Self {
        self.channels = channels;
        self
    }
}

/// Vérifie les canaux demandés pour un moniteur du client `client` et les rend
/// dédoublonnés et triés ; `None` : absents, inchangés.
pub(super) async fn check_channels(
    state: &AppState,
    client: i64,
    channels: Option<&[i64]>,
) -> ApiResult<Option<Vec<i64>>> {
    let Some(channels) = channels else { return Ok(None) };
    let channels = db::target_channels::normalise(channels);
    if channels.is_empty() {
        return Ok(Some(channels));
    }
    let known = db::target_channels::known_channels(&state.pool).await?;
    let owners = clients::owners(&state.pool, clients::Kind::Channel).await?;
    for channel in &channels {
        if !known.contains(channel) {
            return Err(ApiError::BadRequest(format!("Notification channel {channel} not found.")));
        }
        if let Some(reason) = foreign_channel(&owners, *channel, client) {
            return Err(ApiError::BadRequest(reason));
        }
    }
    Ok(Some(channels))
}

/// Mode MSP : le canal d'un client ne parle que des équipements de ce client.
fn foreign_channel(owners: &HashMap<i64, i64>, channel: i64, client: i64) -> Option<String> {
    owners
        .get(&channel)
        .is_some_and(|owner| *owner != client)
        .then(|| format!("Notification channel {channel} belongs to another client."))
}

/// Canaux d'un lot : remplacer, ajouter ou retirer.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelsPatch {
    Set(Vec<i64>),
    Add(Vec<i64>),
    Remove(Vec<i64>),
}

impl ChannelsPatch {
    fn ids(&self) -> &[i64] {
        match self {
            Self::Set(ids) | Self::Add(ids) | Self::Remove(ids) => ids,
        }
    }

    fn apply(&self, current: &[i64]) -> Vec<i64> {
        match self {
            Self::Set(ids) => db::target_channels::normalise(ids),
            Self::Add(ids) => db::target_channels::normalise(&[current, ids.as_slice()].concat()),
            Self::Remove(ids) => current.iter().copied().filter(|id| !ids.contains(id)).collect(),
        }
    }
}

/// Étiquettes libres d'un lot. Les options du type ne sont jamais touchées :
/// ajouter une clé qui en est une est refusé, la retirer n'a pas d'effet.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TagsPatch {
    #[serde(default)]
    pub add: BTreeMap<String, String>,
    #[serde(default)]
    pub remove: Vec<String>,
}

/// Ce que le lot change ; un champ absent reste tel quel.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BulkPatch {
    #[serde(default)]
    pub interval_secs: Option<u64>,
    #[serde(default)]
    pub channels: Option<ChannelsPatch>,
    #[serde(default)]
    pub tags: Option<TagsPatch>,
    /// Dossier d'arrivée ; vide : aucun dossier.
    #[serde(default)]
    pub group_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkRequest {
    pub ids: Vec<TargetId>,
    pub patch: BulkPatch,
}

/// Verdict d'un moniteur du lot.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BulkItem {
    pub id: TargetId,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BulkReport {
    pub updated: usize,
    pub failed: usize,
    /// Dans l'ordre de la requête, un identifiant en double compté une fois.
    pub results: Vec<BulkItem>,
}

/// Ce que la base sait d'un moniteur, pour le lot.
#[derive(Debug, Clone)]
struct Current {
    kind: String,
    interval_secs: u64,
    tags: BTreeMap<String, String>,
    group_name: String,
    channels: Vec<i64>,
    managed_by: Option<String>,
    client: i64,
}

/// Ses valeurs après le lot.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Planned {
    interval_secs: u64,
    tags: BTreeMap<String, String>,
    group_name: String,
    channels: Vec<i64>,
}

/// Le lot appliqué à un moniteur, ou la raison de son refus. `options` : les
/// clés d'options de son type.
fn plan_one(
    patch: &BulkPatch,
    current: &Current,
    options: &HashSet<String>,
) -> Result<Planned, String> {
    let mut tags = current.tags.clone();
    if let Some(change) = &patch.tags {
        for key in &change.remove {
            let key = key.trim();
            if !options.contains(key) {
                tags.remove(key);
            }
        }
        for (key, value) in &change.add {
            let key = key.trim();
            if options.contains(key) {
                return Err(format!("\"{key}\" is a setting of this monitor type, not a tag."));
            }
            tags.insert(key.to_string(), value.trim().to_string());
        }
    }
    Ok(Planned {
        interval_secs: patch.interval_secs.unwrap_or(current.interval_secs),
        tags,
        group_name: patch
            .group_name
            .as_deref()
            .map_or_else(|| current.group_name.clone(), |g| g.trim().to_string()),
        channels: match &patch.channels {
            Some(change) => change.apply(&current.channels),
            None => current.channels.clone(),
        },
    })
}

/// Refus de la requête entière : rien à faire, ou un réglage hors bornes.
fn validate(ids: &[TargetId], patch: &BulkPatch) -> ApiResult<()> {
    if ids.is_empty() {
        return Err(ApiError::BadRequest("Select at least one monitor.".into()));
    }
    if ids.len() > MAX_IDS {
        return Err(ApiError::BadRequest(format!("At most {MAX_IDS} monitors at once.")));
    }
    let tags_empty = patch.tags.as_ref().is_none_or(|t| t.add.is_empty() && t.remove.is_empty());
    if patch.interval_secs.is_none()
        && patch.channels.is_none()
        && tags_empty
        && patch.group_name.is_none()
    {
        return Err(ApiError::BadRequest("Nothing to change.".into()));
    }
    if let Some(secs) = patch.interval_secs
        && !(MIN_INTERVAL_SECS..=MAX_INTERVAL_SECS).contains(&secs)
    {
        return Err(ApiError::BadRequest(format!(
            "The polling interval must be between {MIN_INTERVAL_SECS} seconds and 7 days."
        )));
    }
    if let Some(group) = &patch.group_name
        && group.trim().chars().count() > MAX_GROUP_NAME_CHARS
    {
        return Err(ApiError::BadRequest(format!(
            "The folder name must be at most {MAX_GROUP_NAME_CHARS} characters."
        )));
    }
    if let Some(tags) = &patch.tags {
        for (key, value) in &tags.add {
            let key = key.trim();
            if key.is_empty() || key.chars().count() > MAX_TAG_KEY_CHARS || key.contains('=') {
                return Err(ApiError::BadRequest(format!(
                    "A tag name must be 1 to {MAX_TAG_KEY_CHARS} characters, without \"=\"."
                )));
            }
            if value.chars().count() > MAX_TAG_VALUE_CHARS || value.contains(['\n', '\r']) {
                return Err(ApiError::BadRequest(format!(
                    "A tag value must be one line of at most {MAX_TAG_VALUE_CHARS} characters."
                )));
            }
        }
    }
    Ok(())
}

/// Les moniteurs du lot, d'une requête — sans leurs identifiants chiffrés,
/// dont le lot n'a pas besoin.
async fn load(state: &AppState, ids: &HashSet<TargetId>) -> ApiResult<HashMap<TargetId, Current>> {
    let rows = sqlx::query(
        "SELECT id, kind, interval_secs, tags, group_name, managed_by, client_id FROM targets",
    )
    .fetch_all(&state.pool)
    .await
    .context("lecture des moniteurs du lot")?;
    let mut channels = db::target_channels::all(&state.pool).await?;
    let mut out = HashMap::with_capacity(ids.len());
    for row in rows {
        let id: TargetId = row.try_get("id")?;
        if !ids.contains(&id) {
            continue;
        }
        let tags: String = row.try_get("tags")?;
        out.insert(
            id,
            Current {
                kind: row.try_get("kind")?,
                interval_secs: row.try_get::<i64, _>("interval_secs")?.max(0) as u64,
                tags: serde_json::from_str(&tags)
                    .with_context(|| format!("étiquettes illisibles pour la cible {id}"))?,
                group_name: row.try_get("group_name")?,
                channels: channels.remove(&id).unwrap_or_default(),
                managed_by: row.try_get("managed_by")?,
                client: row.try_get("client_id")?,
            },
        );
    }
    Ok(out)
}

/// Clés d'options d'un type : celles des tables du serveur, et celles que
/// décrit son collecteur (un paquet d'intégration).
fn option_keys(state: &AppState, kind: &str) -> HashSet<String> {
    let mut keys: HashSet<String> = crate::api::collector_option_keys(kind).into_iter().collect();
    if let Some(description) = state.collectors.get(kind).and_then(|c| c.description()) {
        keys.extend(description.options.iter().map(|option| option.key.to_string()));
    }
    keys
}

/// Le verdict d'un moniteur visible : droits d'équipe, gestion, puis le lot.
fn check_one(
    id: TargetId,
    target: &Current,
    patch: &BulkPatch,
    options: &HashSet<String>,
    manageable: Option<&HashSet<TargetId>>,
    owners: &HashMap<i64, i64>,
) -> Result<Planned, String> {
    if manageable.is_some_and(|m| !m.contains(&id)) {
        return Err("Your teams do not let you edit monitors in this folder.".to_string());
    }
    if let Some(by) = &target.managed_by {
        let managed = db::targets::Managed {
            by: by.clone(),
            source: None,
            key: String::new(),
            missing_since: None,
        };
        return Err(super::label_monitors::managed_message(&managed).to_string());
    }
    let next = plan_one(patch, target, options)?;
    let foreign = |channel: &i64| foreign_channel(owners, *channel, target.client);
    if patch.channels.is_some()
        && let Some(reason) = next.channels.iter().find_map(foreign)
    {
        return Err(reason);
    }
    Ok(next)
}

/// `PATCH /api/targets/bulk`
pub async fn apply(
    Identity(who): Identity,
    ClientIp(ip): ClientIp,
    TeamAccess(access): TeamAccess,
    State(state): State<AppState>,
    Json(request): Json<BulkRequest>,
) -> ApiResult<Json<BulkReport>> {
    let BulkRequest { ids, patch } = request;
    validate(&ids, &patch)?;
    if let Some(change) = &patch.channels {
        let known = db::target_channels::known_channels(&state.pool).await?;
        if let Some(missing) = change.ids().iter().find(|id| !known.contains(*id)) {
            return Err(ApiError::BadRequest(format!("Notification channel {missing} not found.")));
        }
    }
    // Un compte restreint ne range que dans un dossier qu'il gère.
    if let Some(group) = &patch.group_name
        && access.folder_level(group.trim()) < Some(Level::Manage)
    {
        return Err(ApiError::Forbidden(
            "Your teams do not let you put targets in this folder.".into(),
        ));
    }

    let mut seen = HashSet::new();
    let ids: Vec<TargetId> = ids.into_iter().filter(|id| seen.insert(*id)).collect();
    let current = load(&state, &seen).await?;
    let visible = access.visible_targets(&state.pool).await?;
    let manageable = access.targets_at(&state.pool, Level::Manage).await?;
    let owners = match &patch.channels {
        Some(_) => clients::owners(&state.pool, clients::Kind::Channel).await?,
        None => HashMap::new(),
    };
    let mut options: HashMap<String, HashSet<String>> = HashMap::new();

    let mut results = Vec::with_capacity(ids.len());
    let mut planned = Vec::new();
    for id in ids {
        let verdict = match current.get(&id) {
            Some(target) if visible.as_ref().is_none_or(|v| v.contains(&id)) => {
                let keys = options
                    .entry(target.kind.clone())
                    .or_insert_with(|| option_keys(&state, &target.kind));
                check_one(id, target, &patch, keys, manageable.as_ref(), &owners)
            }
            _ => Err(format!("Monitor {id} not found.")),
        };
        match verdict {
            Ok(next) => {
                results.push(BulkItem { id, ok: true, error: None });
                planned.push((id, next));
            }
            Err(error) => results.push(BulkItem { id, ok: false, error: Some(error) }),
        }
    }

    if !planned.is_empty() {
        let mut tx = state.pool.begin().await.context("ouverture de la transaction du lot")?;
        for (id, next) in &planned {
            sqlx::query(
                "UPDATE targets SET interval_secs = ?, tags = ?, group_name = ?,
                     updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                 WHERE id = ?",
            )
            .bind(next.interval_secs as i64)
            .bind(serde_json::to_string(&next.tags)?)
            .bind(&next.group_name)
            .bind(id)
            .execute(&mut *tx)
            .await
            .context("modification groupée d'un moniteur")?;
            if patch.channels.is_some() {
                db::target_channels::set_in(&mut tx, *id, &next.channels).await?;
            }
        }
        tx.commit().await.context("validation du lot")?;
        // Nouvelle période : prise en compte au tour suivant du planificateur.
        crate::scheduler::wake();
        audit::record(
            &state.pool,
            Some(&who.label()),
            "targets.bulk_update",
            Some(&audit_subject(&patch, planned.len())),
            ip,
        )
        .await;
    }

    let updated = planned.len();
    let failed = results.len() - updated;
    Ok(Json(BulkReport { updated, failed, results }))
}

/// « 12 monitors: interval, channels » : ce que le journal d'audit retient.
fn audit_subject(patch: &BulkPatch, count: usize) -> String {
    let mut fields = Vec::new();
    if patch.interval_secs.is_some() {
        fields.push("interval");
    }
    if patch.channels.is_some() {
        fields.push("channels");
    }
    if patch.tags.is_some() {
        fields.push("tags");
    }
    if patch.group_name.is_some() {
        fields.push("folder");
    }
    format!("{count} monitors: {}", fields.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> Current {
        Current {
            kind: "http".into(),
            interval_secs: 60,
            tags: BTreeMap::from([
                ("port".to_string(), "8443".to_string()),
                ("env".to_string(), "lab".to_string()),
            ]),
            group_name: "Web".into(),
            channels: vec![2, 5],
            managed_by: None,
            client: 1,
        }
    }

    fn options() -> HashSet<String> {
        HashSet::from(["port".to_string()])
    }

    #[test]
    fn les_canaux_se_remplacent_s_ajoutent_et_se_retirent() {
        assert_eq!(ChannelsPatch::Set(vec![9, 3, 9]).apply(&[2, 5]), vec![3, 9]);
        assert_eq!(ChannelsPatch::Add(vec![3, 5]).apply(&[2, 5]), vec![2, 3, 5]);
        assert_eq!(ChannelsPatch::Remove(vec![5, 7]).apply(&[2, 5]), vec![2]);
        let parsed: ChannelsPatch = serde_json::from_str(r#"{"add":[1]}"#).unwrap();
        assert_eq!(parsed, ChannelsPatch::Add(vec![1]));
    }

    /// Les étiquettes libres changent ; les options du type, jamais.
    #[test]
    fn les_options_du_type_ne_sont_pas_des_etiquettes() {
        let patch = BulkPatch {
            tags: Some(TagsPatch {
                add: BTreeMap::from([("team".to_string(), " ops ".to_string())]),
                remove: vec!["env".into(), "port".into()],
            }),
            ..BulkPatch::default()
        };
        let next = plan_one(&patch, &current(), &options()).unwrap();
        assert_eq!(next.tags.get("team").map(String::as_str), Some("ops"));
        assert!(!next.tags.contains_key("env"));
        assert_eq!(next.tags.get("port").map(String::as_str), Some("8443"), "an option stays");
        assert_eq!((next.interval_secs, next.group_name.as_str()), (60, "Web"));
        assert_eq!(next.channels, vec![2, 5]);

        let refused = BulkPatch {
            tags: Some(TagsPatch {
                add: BTreeMap::from([("port".to_string(), "80".to_string())]),
                remove: Vec::new(),
            }),
            ..BulkPatch::default()
        };
        assert!(plan_one(&refused, &current(), &options()).is_err());
    }

    #[test]
    fn periode_dossier_et_canaux_d_un_lot() {
        let patch = BulkPatch {
            interval_secs: Some(300),
            group_name: Some("  Edge ".into()),
            channels: Some(ChannelsPatch::Set(Vec::new())),
            ..BulkPatch::default()
        };
        let next = plan_one(&patch, &current(), &options()).unwrap();
        assert_eq!(next.interval_secs, 300);
        assert_eq!(next.group_name, "Edge");
        assert!(next.channels.is_empty(), "back to the channels of the rules");
    }

    #[test]
    fn une_requete_vide_ou_hors_bornes_est_refusee() {
        let every_minute = BulkPatch { interval_secs: Some(60), ..BulkPatch::default() };
        assert!(validate(&[], &every_minute).is_err(), "no monitor");
        assert!(validate(&[1], &BulkPatch::default()).is_err(), "nothing to change");
        let too_fast = BulkPatch { interval_secs: Some(5), ..BulkPatch::default() };
        assert!(validate(&[1], &too_fast).is_err());
        let bad_key = BulkPatch {
            tags: Some(TagsPatch {
                add: BTreeMap::from([("a=b".to_string(), "c".to_string())]),
                remove: Vec::new(),
            }),
            ..BulkPatch::default()
        };
        assert!(validate(&[1], &bad_key).is_err());
        let no_folder = BulkPatch { group_name: Some(String::new()), ..BulkPatch::default() };
        assert!(validate(&[1], &no_folder).is_ok(), "out of every folder");
    }

    #[test]
    fn un_canal_d_un_autre_client_est_refuse() {
        let owners = HashMap::from([(4, 2)]);
        assert!(foreign_channel(&owners, 4, 1).is_some());
        assert!(foreign_channel(&owners, 4, 2).is_none());
        assert!(foreign_channel(&owners, 7, 1).is_none(), "an instance channel");
    }
}
