//! Envoi des notifications personnelles, appelé par le moteur d'alerting une
//! fois par cycle, après les canaux (voir `alerting::engine`).
//!
//! Un message par équipement, sans fenêtre de regroupement : une personne veut
//! savoir tout de suite que son NAS est tombé. Un échec d'envoi est consigné
//! dans le journal du serveur et n'est pas retenté : le cycle a déjà marqué
//! l'alerte comme annoncée, et la liste des alertes reste la référence.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use chrono::{DateTime, TimeDelta, Utc};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use tracing::{debug, warn};

use super::{Prefs, Scope, Sent, TargetFacts, in_scope, select_items};
use crate::alerting::group::{AlertGroup, GroupItem, NotifyReason};
use crate::alerting::notify_policy::Digest;
use crate::auth::users::{self, User};
use crate::crypto::Cipher;
use crate::notify::ChannelConfig;
use crate::notify::webpush::{self, Audience};

/// Mémoire des envois : un mois, pour qu'une panne longue puisse encore
/// annoncer son retour à la normale.
const LOG_DAYS: i64 = 30;

/// Ce dont l'envoi a besoin, fourni par le moteur.
pub struct Ctx<'a> {
    pub pool: &'a SqlitePool,
    pub cipher: &'a Cipher,
    pub http: &'a reqwest::Client,
    /// Base des liens vers l'équipement dans les messages.
    pub public_url: Option<&'a str>,
    /// Plafond de messages par compte et par heure (politique globale). 0 :
    /// sans limite.
    pub max_per_hour: u32,
}

/// Envoie à chaque compte ce que ses choix retiennent des groupes du cycle.
/// Ne propage jamais d'erreur : le cycle d'alerting n'en dépend pas.
pub async fn dispatch(ctx: &Ctx<'_>, groups: &[AlertGroup], now: DateTime<Utc>) {
    if groups.is_empty() || crate::notify::sending_disabled() {
        return;
    }
    if let Err(error) = run(ctx, groups, now).await {
        warn!(?error, "personal notifications not sent");
    }
}

async fn run(ctx: &Ctx<'_>, groups: &[AlertGroup], now: DateTime<Utc>) -> Result<()> {
    let subscribers = super::all_active(ctx.pool).await?;
    if subscribers.is_empty() {
        return Ok(());
    }
    purge(ctx.pool, now).await?;
    let held = flap_holds(ctx.pool, now).await?;
    let folders = target_folders(ctx.pool).await?;
    let mail = if subscribers.iter().any(|(_, prefs)| prefs.email) {
        crate::notify::mail_server::channel(ctx.pool, ctx.cipher).await
    } else {
        None
    };
    let shared = Shared { groups, held: &held, folders: &folders, mail: mail.as_ref(), now };
    for (user_id, prefs) in &subscribers {
        let Some(user) = users::get(ctx.pool, *user_id).await? else { continue };
        if user.disabled || user.role.is_wall() {
            continue;
        }
        if let Err(error) = deliver_to(ctx, &shared, &user, prefs).await {
            warn!(user = %user.username, ?error, "personal notification failed");
        }
    }
    Ok(())
}

/// Ce que tous les comptes d'un cycle partagent.
struct Shared<'a> {
    groups: &'a [AlertGroup],
    held: &'a HashSet<String>,
    folders: &'a HashMap<i64, String>,
    mail: Option<&'a ChannelConfig>,
    now: DateTime<Utc>,
}

async fn deliver_to(ctx: &Ctx<'_>, shared: &Shared<'_>, user: &User, prefs: &Prefs) -> Result<()> {
    let pool = ctx.pool;
    let now = shared.now;
    let email = match (prefs.email, user.email.as_deref(), shared.mail) {
        (true, Some(address), Some(mail)) => Some(personal_mail(mail, address, &user.username)),
        _ => None,
    };
    let push = prefs.webpush && webpush::store::count_for_user(pool, user.id).await? > 0;
    if email.is_none() && !push {
        return Ok(());
    }
    // Ce que ses équipes ne lui montrent pas ne lui parvient jamais.
    let access = crate::auth::teams::access_for_user(pool, user).await?;
    let visible = access.visible_targets(pool).await?;
    let team_folders = match prefs.scope {
        Scope::Teams => folders_of_teams(pool, user.id).await?,
        _ => Vec::new(),
    };
    let mut log = load_log(pool, user.id, now).await?;
    let mut this_hour = messages_since(pool, user.id, now - TimeDelta::hours(1)).await?;
    let push_client = push.then(webpush::client);

    for group in shared.groups {
        if let Some(visible) = &visible
            && group.target_id.is_none_or(|id| !visible.contains(&id))
        {
            continue;
        }
        let facts = TargetFacts {
            id: group.target_id,
            folder: group.target_id.and_then(|id| shared.folders.get(&id)).map(String::as_str),
            tags: &group.target_tags,
        };
        if !in_scope(prefs, &team_folders, &facts) {
            continue;
        }
        let items = select_items(prefs, &group.items, &log, shared.held, now);
        if items.is_empty() {
            continue;
        }
        if ctx.max_per_hour > 0 && this_hour >= i64::from(ctx.max_per_hour) {
            debug!(user = %user.username, "personal notifications: hourly cap reached");
            break;
        }
        let message = render(group, items.clone(), ctx.public_url, now);
        let mut delivered = false;
        if let Some(channel) = &email {
            delivered |= crate::notify::deliver(ctx.http, channel, &message).await.is_success();
        }
        if let Some(client) = &push_client {
            let store = webpush::Store { pool, cipher: ctx.cipher };
            let settings = webpush::Settings {
                ttl: std::time::Duration::from_secs(webpush::DEFAULT_TTL_SECS),
                ..webpush::Settings::for_test()
            };
            let audience = Audience::User { user_id: user.id, subscription: None };
            match webpush::send(store, client, &audience, &settings, &message).await {
                Ok(outcome) => delivered |= outcome.delivered > 0,
                Err(error) => warn!(user = %user.username, %error, "personal push failed"),
            }
        }
        if delivered {
            record(pool, user.id, group.target_id, &items, now).await?;
            this_hour += 1;
            log.extend(items.iter().map(|item| Sent {
                fingerprint: item.fingerprint.clone(),
                reason: item.reason,
                at: now,
            }));
        }
    }
    Ok(())
}

/// Le serveur de courriel de l'instance, adressé au compte.
fn personal_mail(mail: &ChannelConfig, address: &str, username: &str) -> ChannelConfig {
    let mut channel = mail.clone();
    channel.settings["to"] = json!([address]);
    channel.name = format!("Personal email of {username}");
    channel
}

/// Le message d'un équipement, réduit aux lignes retenues pour ce compte.
fn render(
    group: &AlertGroup,
    items: Vec<GroupItem>,
    public_url: Option<&str>,
    now: DateTime<Utc>,
) -> crate::notify::Message {
    let mut one = group.clone();
    one.severity = items.iter().map(|item| item.severity).max().unwrap_or(group.severity);
    one.items = items;
    let digest = Digest { groups: vec![one], resolved_meanwhile: Vec::new(), hold: None, at: now };
    crate::notify::render_digest(&digest, public_url)
}

fn stamp(at: DateTime<Utc>) -> String {
    crate::db::timestamp(at)
}

fn read_stamp(raw: &str) -> Option<DateTime<Utc>> {
    chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").ok().map(|at| at.and_utc())
}

async fn purge(pool: &SqlitePool, now: DateTime<Utc>) -> Result<()> {
    sqlx::query("DELETE FROM user_notify_log WHERE at < ?")
        .bind(stamp(now - TimeDelta::days(LOG_DAYS)))
        .execute(pool)
        .await
        .context("purge du journal des notifications personnelles")?;
    Ok(())
}

async fn load_log(pool: &SqlitePool, user_id: i64, now: DateTime<Utc>) -> Result<Vec<Sent>> {
    let rows = sqlx::query(
        "SELECT fingerprint, reason, at FROM user_notify_log
         WHERE user_id = ? AND at >= ? ORDER BY at, id",
    )
    .bind(user_id)
    .bind(stamp(now - TimeDelta::days(LOG_DAYS)))
    .fetch_all(pool)
    .await
    .context("journal des notifications personnelles")?;
    let mut log = Vec::with_capacity(rows.len());
    for row in &rows {
        let reason: String = row.try_get("reason")?;
        let at: String = row.try_get("at")?;
        let (Some(reason), Some(at)) = (NotifyReason::parse(&reason), read_stamp(&at)) else {
            continue;
        };
        log.push(Sent { fingerprint: row.try_get("fingerprint")?, reason, at });
    }
    Ok(log)
}

/// Messages partis vers le compte depuis `since` : un par équipement et par
/// cycle.
async fn messages_since(pool: &SqlitePool, user_id: i64, since: DateTime<Utc>) -> Result<i64> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM (SELECT DISTINCT at, target_id FROM user_notify_log
                               WHERE user_id = ? AND at > ?)",
    )
    .bind(user_id)
    .bind(stamp(since))
    .fetch_one(pool)
    .await
    .context("messages personnels de la dernière heure")
}

async fn record(
    pool: &SqlitePool,
    user_id: i64,
    target_id: Option<i64>,
    items: &[GroupItem],
    now: DateTime<Utc>,
) -> Result<()> {
    let mut tx = pool.begin().await.context("ouverture de la transaction")?;
    for item in items {
        sqlx::query(
            "INSERT INTO user_notify_log (user_id, target_id, fingerprint, reason, at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(target_id)
        .bind(&item.fingerprint)
        .bind(item.reason.as_str())
        .bind(stamp(now))
        .execute(&mut *tx)
        .await
        .context("journal des notifications personnelles")?;
    }
    tx.commit().await.context("validation du journal")?;
    Ok(())
}

/// Empreintes retenues pour battement par la politique de notification.
async fn flap_holds(pool: &SqlitePool, now: DateTime<Utc>) -> Result<HashSet<String>> {
    let rows = sqlx::query("SELECT fingerprint, held_until FROM notify_flap")
        .fetch_all(pool)
        .await
        .context("retenues de battement")?;
    let mut held = HashSet::new();
    for row in &rows {
        let until: String = row.try_get("held_until")?;
        let active = DateTime::parse_from_rfc3339(&until)
            .map(|at| at.with_timezone(&Utc))
            .ok()
            .or_else(|| read_stamp(&until))
            .is_none_or(|at| at > now);
        if active {
            held.insert(row.try_get("fingerprint")?);
        }
    }
    Ok(held)
}

/// Dossier de chaque équipement.
async fn target_folders(pool: &SqlitePool) -> Result<HashMap<i64, String>> {
    let rows = sqlx::query("SELECT id, group_name FROM targets")
        .fetch_all(pool)
        .await
        .context("dossiers des équipements")?;
    rows.iter().map(|row| Ok((row.try_get("id")?, row.try_get("group_name")?))).collect()
}

/// Dossiers ouverts par les équipes d'un compte, quel qu'en soit le niveau.
pub async fn folders_of_teams(pool: &SqlitePool, user_id: i64) -> Result<Vec<String>> {
    sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT g.folder FROM team_folder_grants g
         JOIN team_members m ON m.team_id = g.team_id
         WHERE m.user_id = ? ORDER BY g.folder",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("dossiers des équipes d'un compte")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_personal_mail_goes_to_the_account_only() {
        let server = crate::notify::mail_server::MailServer {
            host: "smtp.example.org".into(),
            from_address: "monit@example.org".into(),
            ..Default::default()
        };
        let channel = personal_mail(&server.channel(None), "jane@example.org", "jane");
        assert_eq!(channel.settings["to"], json!(["jane@example.org"]));
        assert_eq!(channel.kind, "smtp");
        assert!(crate::notify::smtp::Smtp::new(&channel).is_ok());
    }

    #[test]
    fn stored_timestamps_read_back() {
        let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        assert_eq!(read_stamp(&stamp(now)), Some(now));
        assert_eq!(read_stamp("garbage"), None);
    }
}
