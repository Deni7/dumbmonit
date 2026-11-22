//! Journal d'audit des gestes de sécurité.
//!
//! Qui s'est connecté, d'où, qui a créé un jeton, qui a touché à un compte ou à
//! son second facteur. Ce n'est pas un journal applicatif — `tracing` s'en
//! charge — mais la trace que l'on relit dans les réglages quand quelque chose
//! semble anormal. Il est borné : au-delà de [`keep`] entrées
//! (`DUMBMONIT_AUDIT_KEEP`), les plus anciennes s'effacent, pour qu'une instance
//! qui vit dix ans ne traîne pas dix ans de connexions.

use std::net::IpAddr;

use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::{Row, SqlitePool};

/// Nombre d'entrées conservées par défaut.
pub const DEFAULT_KEEP: i64 = 5_000;
/// Bornes de `DUMBMONIT_AUDIT_KEEP`.
pub const MIN_KEEP: i64 = 100;
pub const MAX_KEEP: i64 = 1_000_000;

/// Interprète `DUMBMONIT_AUDIT_KEEP` : une erreur de syntaxe est signalée, une
/// valeur hors bornes est ramenée dans les bornes.
pub fn parse_keep(raw: Option<&str>) -> Result<i64, String> {
    match raw.map(str::trim).filter(|raw| !raw.is_empty()) {
        None => Ok(DEFAULT_KEEP),
        Some(raw) => raw
            .parse::<i64>()
            .map(|n| n.clamp(MIN_KEEP, MAX_KEEP))
            .map_err(|_| format!("DUMBMONIT_AUDIT_KEEP: `{raw}` is not a number")),
    }
}

/// Nombre d'entrées conservées (`DUMBMONIT_AUDIT_KEEP`, 100 à 1 000 000).
pub fn keep() -> i64 {
    parse_keep(crate::config::env_var("DUMBMONIT_AUDIT_KEEP").as_deref()).unwrap_or(DEFAULT_KEEP)
}

/// Une entrée, telle que l'interface la lit.
#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub id: i64,
    pub at: String,
    pub actor: Option<String>,
    pub action: String,
    pub subject: Option<String>,
    pub ip: Option<String>,
}

/// Enregistre un geste. Une erreur d'écriture est journalisée et non propagée :
/// le journal ne doit jamais empêcher le geste qu'il décrit.
pub async fn record(
    pool: &SqlitePool,
    actor: Option<&str>,
    action: &str,
    subject: Option<&str>,
    ip: Option<IpAddr>,
) {
    let ip = ip.map(|ip| ip.to_string());
    // Le même geste dans les journaux du serveur, marqué comme entrée d'audit :
    // il suit ainsi les fichiers de `/data/logs` et les destinations (`logs`).
    tracing::info!(
        target: crate::logs::AUDIT_TARGET,
        actor = actor.unwrap_or("-"),
        action,
        subject = subject.unwrap_or("-"),
        ip = ip.as_deref().unwrap_or("-"),
        "audit: {action}"
    );
    let result =
        sqlx::query("INSERT INTO audit_log (actor, action, subject, ip) VALUES (?, ?, ?, ?)")
            .bind(actor)
            .bind(action)
            .bind(subject)
            .bind(ip)
            .execute(pool)
            .await;
    if let Err(error) = result {
        tracing::warn!(?error, action, "écriture du journal d'audit impossible");
        return;
    }
    // Un ménage occasionnel suffit : une entrée sur cent, environ.
    if rand::random::<u8>() < 3 {
        let _ = sqlx::query(
            "DELETE FROM audit_log WHERE id NOT IN (SELECT id FROM audit_log ORDER BY id DESC LIMIT ?)",
        )
        .bind(keep())
        .execute(pool)
        .await;
    }
}

/// Filtres de lecture du journal. Tous facultatifs, combinés par « et ».
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Sous-chaîne du nom de l'acteur.
    pub actor: Option<String>,
    /// Geste exact (`login.failed`…).
    pub action: Option<String>,
    /// Adresse exacte.
    pub ip: Option<String>,
    /// Borne basse incluse, UTC, `AAAA-MM-JJ` ou `AAAA-MM-JJ HH:MM:SS`.
    pub since: Option<String>,
    /// Borne haute incluse (un jour seul couvre toute la journée).
    pub until: Option<String>,
    /// Curseur : seulement les entrées d'identifiant strictement inférieur.
    pub before: Option<i64>,
}

/// Valide une borne de période et la rend au format stocké (`…T…Z`).
pub fn normalise_bound(raw: &str, end_of_day: bool) -> Result<String, String> {
    let raw = raw.trim();
    let invalid = || format!("`{raw}` is not a date (use YYYY-MM-DD or YYYY-MM-DD HH:MM:SS)");
    if let Ok(day) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let time = if end_of_day { (23, 59, 59) } else { (0, 0, 0) };
        let at = day.and_hms_opt(time.0, time.1, time.2).ok_or_else(invalid)?;
        return Ok(at.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    }
    for format in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M:%SZ"] {
        if let Ok(at) = chrono::NaiveDateTime::parse_from_str(raw, format) {
            return Ok(at.format("%Y-%m-%dT%H:%M:%SZ").to_string());
        }
    }
    Err(invalid())
}

/// Échappe `%`, `_` et `\` pour un `LIKE ... ESCAPE '\'`.
fn like_escape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Les entrées correspondant au filtre, de la plus neuve à la plus ancienne.
/// `limit` est borné à 1 à `max`.
pub async fn search(
    pool: &SqlitePool,
    filter: &Filter,
    limit: i64,
    max: i64,
) -> Result<Vec<Entry>> {
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
        "SELECT id, at, actor, action, subject, ip FROM audit_log WHERE 1 = 1",
    );
    fn text(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|v| !v.is_empty())
    }
    if let Some(actor) = text(&filter.actor) {
        query.push(" AND actor LIKE ").push_bind(format!("%{}%", like_escape(actor)));
        query.push(" ESCAPE '\\'");
    }
    if let Some(action) = text(&filter.action) {
        query.push(" AND action = ").push_bind(action.to_string());
    }
    if let Some(ip) = text(&filter.ip) {
        query.push(" AND ip = ").push_bind(ip.to_string());
    }
    if let Some(since) = &filter.since {
        query.push(" AND at >= ").push_bind(since.clone());
    }
    if let Some(until) = &filter.until {
        query.push(" AND at <= ").push_bind(until.clone());
    }
    if let Some(before) = filter.before {
        query.push(" AND id < ").push_bind(before);
    }
    query.push(" ORDER BY id DESC LIMIT ").push_bind(limit.clamp(1, max));
    let rows = query.build().fetch_all(pool).await.context("lecture du journal d'audit")?;
    rows.into_iter().map(entry_from_row).collect()
}

fn entry_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Entry> {
    Ok(Entry {
        id: row.try_get("id")?,
        at: row.try_get("at")?,
        actor: row.try_get("actor")?,
        action: row.try_get("action")?,
        subject: row.try_get("subject")?,
        ip: row.try_get("ip")?,
    })
}

/// Une cellule CSV sûre : entre guillemets, et neutralisée si un tableur la
/// lirait comme une formule (`=`, `+`, `-`, `@`, tabulation, retour chariot).
pub fn csv_cell(raw: &str) -> String {
    let mut text = raw.to_string();
    if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        text.insert(0, '\'');
    }
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// Le journal au format CSV (RFC 4180, fins de ligne `\r\n`).
pub fn to_csv(entries: &[Entry]) -> String {
    let mut out = String::from("id,at,actor,action,subject,ip\r\n");
    for entry in entries {
        let cells = [
            entry.id.to_string(),
            csv_cell(&entry.at),
            csv_cell(entry.actor.as_deref().unwrap_or("")),
            csv_cell(&entry.action),
            csv_cell(entry.subject.as_deref().unwrap_or("")),
            csv_cell(entry.ip.as_deref().unwrap_or("")),
        ];
        out.push_str(&cells.join(","));
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_retention_est_bornee() {
        assert_eq!(parse_keep(None), Ok(5_000));
        assert_eq!(parse_keep(Some(" ")), Ok(5_000));
        assert_eq!(parse_keep(Some("10")), Ok(MIN_KEEP));
        assert_eq!(parse_keep(Some("20000")), Ok(20_000));
        assert_eq!(parse_keep(Some("99999999")), Ok(MAX_KEEP));
        assert!(parse_keep(Some("beaucoup")).is_err());
    }

    #[test]
    fn une_borne_de_periode_se_normalise() {
        assert_eq!(normalise_bound("2026-03-01", false).unwrap(), "2026-03-01T00:00:00Z");
        assert_eq!(normalise_bound("2026-03-01", true).unwrap(), "2026-03-01T23:59:59Z");
        assert_eq!(
            normalise_bound("2026-03-01 08:30:00", true).unwrap(),
            "2026-03-01T08:30:00Z"
        );
        assert!(normalise_bound("hier", false).is_err());
    }

    #[test]
    fn le_csv_neutralise_les_formules() {
        assert_eq!(csv_cell("=1+1"), "\"'=1+1\"");
        assert_eq!(csv_cell("@SUM(A1)"), "\"'@SUM(A1)\"");
        assert_eq!(csv_cell("-2"), "\"'-2\"");
        assert_eq!(csv_cell("dit \"oui\""), "\"dit \"\"oui\"\"\"");
        assert_eq!(csv_cell("normal"), "\"normal\"");
    }

    #[test]
    fn le_csv_a_un_en_tete_et_une_ligne_par_entree() {
        let entry = Entry {
            id: 7,
            at: "2026-03-01T08:00:00Z".into(),
            actor: Some("+admin".into()),
            action: "login".into(),
            subject: None,
            ip: Some("10.0.0.1".into()),
        };
        let csv = to_csv(&[entry]);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert_eq!(lines[0], "id,at,actor,action,subject,ip");
        assert_eq!(
            lines[1],
            "7,\"2026-03-01T08:00:00Z\",\"'+admin\",\"login\",\"\",\"10.0.0.1\""
        );
    }

    #[tokio::test]
    async fn les_filtres_et_le_curseur_se_combinent() {
        let dir = tempfile::tempdir().unwrap();
        let pool = crate::db::open(&dir.path().join("t.db")).await.unwrap();
        for (at, actor, action, ip) in [
            ("2026-03-01T08:00:00Z", "alice", "login", "10.0.0.1"),
            ("2026-03-02T08:00:00Z", "bob", "login.failed", "10.0.0.2"),
            ("2026-03-03T08:00:00Z", "alice", "token.created", "10.0.0.1"),
            ("2026-03-04T08:00:00Z", "a_ice", "login", "10.0.0.3"),
        ] {
            sqlx::query("INSERT INTO audit_log (at, actor, action, ip) VALUES (?, ?, ?, ?)")
                .bind(at)
                .bind(actor)
                .bind(action)
                .bind(ip)
                .execute(&pool)
                .await
                .unwrap();
        }
        let all = search(&pool, &Filter::default(), 100, 100).await.unwrap();
        assert_eq!(all.len(), 4);
        assert_eq!(all[0].actor.as_deref(), Some("a_ice"));

        let by_actor = Filter { actor: Some("alice".into()), ..Filter::default() };
        assert_eq!(search(&pool, &by_actor, 100, 100).await.unwrap().len(), 2);
        // `_` n'est pas un joker : « a_ice » ne trouve que lui.
        let underscore = Filter { actor: Some("a_ice".into()), ..Filter::default() };
        assert_eq!(search(&pool, &underscore, 100, 100).await.unwrap().len(), 1);

        let by_ip = Filter { ip: Some("10.0.0.1".into()), ..Filter::default() };
        assert_eq!(search(&pool, &by_ip, 100, 100).await.unwrap().len(), 2);

        let period = Filter {
            since: Some(normalise_bound("2026-03-02", false).unwrap()),
            until: Some(normalise_bound("2026-03-03", true).unwrap()),
            ..Filter::default()
        };
        assert_eq!(search(&pool, &period, 100, 100).await.unwrap().len(), 2);

        let first = search(&pool, &Filter::default(), 2, 100).await.unwrap();
        assert_eq!(first.len(), 2);
        let next = Filter { before: first.last().map(|e| e.id), ..Filter::default() };
        let second = search(&pool, &next, 2, 100).await.unwrap();
        assert_eq!(second.len(), 2);
        assert!(second[0].id < first[1].id);
    }
}
