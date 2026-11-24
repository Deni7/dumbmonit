//! Forme de la réponse (les noms de champs du contrat TypeScript), état des
//! moniteurs et regroupement par site.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::model::*;
use super::sites::{self, LOCAL_KEY, Monitor};
use super::{Range, parse_time};

fn keys(value: &Value) -> BTreeSet<String> {
    value.as_object().expect("objet").keys().cloned().collect()
}

fn expect(value: &Value, wanted: &[&str]) {
    let wanted: BTreeSet<String> = wanted.iter().map(|k| (*k).to_string()).collect();
    assert_eq!(keys(value), wanted, "{value}");
}

/// Une réponse dont chaque champ facultatif est rempli et chaque liste a un
/// élément : toutes les clés du contrat doivent y paraître.
fn full() -> OverviewWidgets {
    let entry = TopEntry {
        target_id: 1,
        label: "nas /srv".into(),
        site: "Home".into(),
        value: 81.5,
        unit: "%".into(),
        series: vec![80.0, 81.5],
        forecast_days: Some(12.0),
    };
    OverviewWidgets {
        sites: vec![WidgetSite {
            key: "local".into(),
            name: "Home".into(),
            monitors: vec![WidgetMonitor {
                id: 1,
                name: "nas".into(),
                kind: "Synology DSM".into(),
                category: Category::Storage,
                status: WidgetStatus::Degraded,
            }],
            wan_latency_ms: Some(vec![12.0]),
            next_maintenance: Some("2026-11-24 02:00:00".into()),
            last_backup: Some("2026-11-23 01:00:00".into()),
        }],
        top: TopConsumers {
            cpu: vec![entry.clone()],
            mem: vec![entry.clone()],
            disk: vec![entry.clone()],
            net: vec![entry],
        },
        backups: vec![BackupJob {
            job: "vm/100".into(),
            engine: "PBS".into(),
            detail: "pbs → main".into(),
            site: "Home".into(),
            target_id: 2,
            last_success: Some("2026-11-23 01:00:00".into()),
            duration_s: Some(300.0),
            changed_bytes: Some(1024.0),
            runs: vec![BackupRun { at: "2026-11-23 01:05:00".into(), ok: true }],
            status: BackupStatus::Ok,
        }],
        services: vec![ServiceCheck {
            target_id: 3,
            name: "Intranet".into(),
            kind: "HTTPS".into(),
            status: WidgetStatus::Up,
            checks: vec![Some(true), None, Some(false)],
            p95_ms: Some(120.0),
            availability_30d: Some(99.95),
        }],
        alert_stats: AlertStats {
            days: vec![AlertDay { day: "2026-11-23".into(), critical: 1, warning: 2, info: 0 }],
            total: 3,
            previous_total: 4,
            mtta_s: Some(60),
            previous_mtta_s: Some(90),
            mttr_s: Some(600),
            previous_mttr_s: Some(700),
            acked_within_15m_pct: Some(50.0),
            noisiest: Some(Noisiest {
                rule: "Host down".into(),
                target: "nas".into(),
                target_id: Some(1),
                count: 2,
            }),
        },
        agents: AgentSummary {
            total: 1,
            versions: vec![AgentVersionCount { version: "0.20.0".into(), count: 1, latest: true }],
            relays: vec![RelaySummary {
                name: "paris-relay".into(),
                connected: true,
                last_seen: Some("2026-11-23 10:00:00".into()),
            }],
            server_update: Some(ServerUpdate { version: "0.21.0".into() }),
        },
        events: vec![WidgetEvent {
            at: "2026-11-23 10:00:00".into(),
            kind: EventKind::Ack,
            text: "Host down acknowledged on nas".into(),
            target_id: Some(1),
        }],
        availability: AvailabilitySeries {
            days: vec![DayPct { day: "2026-11-23".into(), pct: Some(99.9) }],
            avg_30d: Some(99.9),
            previous_avg_30d: Some(99.8),
            slo_target: 99.9,
            budget_left_pct: Some(10.0),
        },
        alerts_opened: AlertsOpened {
            days: vec![DayCount { day: "2026-11-23".into(), count: 3 }],
            this_week: 3,
            last_week: 1,
            last_resolved_at: Some("2026-11-23 09:00:00".into()),
        },
    }
}

#[test]
fn la_reponse_a_exactement_les_champs_du_contrat() {
    let value = serde_json::to_value(full()).unwrap();
    expect(
        &value,
        &[
            "sites",
            "top",
            "backups",
            "services",
            "alert_stats",
            "agents",
            "events",
            "availability",
            "alerts_opened",
        ],
    );
    let site = &value["sites"][0];
    expect(site, &["key", "name", "monitors", "wan_latency_ms", "next_maintenance", "last_backup"]);
    expect(&site["monitors"][0], &["id", "name", "kind", "category", "status"]);
    expect(&value["top"], &["cpu", "mem", "disk", "net"]);
    for family in ["cpu", "mem", "disk", "net"] {
        expect(
            &value["top"][family][0],
            &["target_id", "label", "site", "value", "unit", "series", "forecast_days"],
        );
    }
    let backup = &value["backups"][0];
    expect(
        backup,
        &[
            "job",
            "engine",
            "detail",
            "site",
            "target_id",
            "last_success",
            "duration_s",
            "changed_bytes",
            "runs",
            "status",
        ],
    );
    expect(&backup["runs"][0], &["at", "ok"]);
    expect(
        &value["services"][0],
        &["target_id", "name", "kind", "status", "checks", "p95_ms", "availability_30d"],
    );
    let stats = &value["alert_stats"];
    expect(
        stats,
        &[
            "days",
            "total",
            "previous_total",
            "mtta_s",
            "previous_mtta_s",
            "mttr_s",
            "previous_mttr_s",
            "acked_within_15m_pct",
            "noisiest",
        ],
    );
    expect(&stats["days"][0], &["day", "critical", "warning", "info"]);
    expect(&stats["noisiest"], &["rule", "target", "target_id", "count"]);
    let agents = &value["agents"];
    expect(agents, &["total", "versions", "relays", "server_update"]);
    expect(&agents["versions"][0], &["version", "count", "latest"]);
    expect(&agents["relays"][0], &["name", "connected", "last_seen"]);
    expect(&agents["server_update"], &["version"]);
    expect(&value["events"][0], &["at", "kind", "text", "target_id"]);
    let availability = &value["availability"];
    expect(availability, &["days", "avg_30d", "previous_avg_30d", "slo_target", "budget_left_pct"]);
    expect(&availability["days"][0], &["day", "pct"]);
    let opened = &value["alerts_opened"];
    expect(opened, &["days", "this_week", "last_week", "last_resolved_at"]);
    expect(&opened["days"][0], &["day", "count"]);
}

fn word(value: impl serde::Serialize) -> String {
    serde_json::to_value(value).unwrap().as_str().unwrap().to_string()
}

#[test]
fn les_valeurs_enumerees_sont_les_mots_du_contrat() {
    assert_eq!(word(Category::Servers), "servers");
    assert_eq!(word(Category::Storage), "storage");
    assert_eq!(word(Category::Network), "network");
    assert_eq!(word(Category::Services), "services");
    assert_eq!(word(Category::Backups), "backups");
    assert_eq!(word(Category::Other), "other");
    assert_eq!(word(WidgetStatus::Up), "up");
    assert_eq!(word(WidgetStatus::Down), "down");
    assert_eq!(word(WidgetStatus::Degraded), "degraded");
    assert_eq!(word(WidgetStatus::Paused), "paused");
    assert_eq!(word(BackupStatus::Ok), "ok");
    assert_eq!(word(BackupStatus::Failed), "failed");
    assert_eq!(word(BackupStatus::Running), "running");
    assert_eq!(word(BackupStatus::Stale), "stale");
    assert_eq!(word(EventKind::Crit), "crit");
    assert_eq!(word(EventKind::Warn), "warn");
    assert_eq!(word(EventKind::Ok), "ok");
    assert_eq!(word(EventKind::Info), "info");
    assert_eq!(word(EventKind::Maint), "maint");
    assert_eq!(word(EventKind::Ack), "ack");
}

#[test]
fn les_champs_absents_sont_null_et_non_omis() {
    let mut widgets = full();
    widgets.sites[0].wan_latency_ms = None;
    widgets.top.disk[0].forecast_days = None;
    widgets.alert_stats.noisiest = None;
    widgets.agents.server_update = None;
    let value = serde_json::to_value(widgets).unwrap();
    assert!(value["sites"][0]["wan_latency_ms"].is_null());
    assert!(value["sites"][0].as_object().unwrap().contains_key("wan_latency_ms"));
    assert!(value["top"]["disk"][0]["forecast_days"].is_null());
    assert!(value["alert_stats"]["noisiest"].is_null());
    assert!(value["agents"]["server_update"].is_null());
}

#[test]
fn la_fenetre_vaut_24_h_par_defaut_et_refuse_le_reste() {
    assert_eq!(Range::parse(None), Some(Range::Day));
    assert_eq!(Range::parse(Some("")), Some(Range::Day));
    assert_eq!(Range::parse(Some("1h")).map(Range::seconds), Some(3_600));
    assert_eq!(Range::parse(Some("7d")).map(Range::seconds), Some(7 * 86_400));
    assert_eq!(Range::parse(Some("30d")).map(Range::seconds), Some(30 * 86_400));
    assert_eq!(Range::parse(Some("2d")), None);
}

#[test]
fn les_horodatages_se_lisent_avec_ou_sans_suffixe() {
    let expected = now();
    for raw in ["2026-11-23T10:00:00Z", "2026-11-23T10:00:00.000Z", "2026-11-23 10:00:00"] {
        assert_eq!(parse_time(raw), Some(expected), "{raw}");
    }
    assert_eq!(parse_time("hier"), None);
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-23T10:00:00Z").unwrap().with_timezone(&Utc)
}

#[test]
fn l_etat_suit_la_console() {
    let now = now();
    let fresh = Some("2026-11-23T09:59:00Z");
    let status = |kind, enabled, error, probe_at, probe, firing| {
        sites::status_of(kind, enabled, error, probe_at, 60, probe, firing, now)
    };
    // Un moniteur désactivé est en pause, quoi qu'il dise.
    assert_eq!(status("snmp", false, Some("timeout"), fresh, None, true), WidgetStatus::Paused);
    // Équipement : dernière interrogation récente, trop ancienne, jamais.
    assert_eq!(status("snmp", true, None, fresh, None, false), WidgetStatus::Up);
    let old = Some("2026-11-23T09:00:00Z");
    assert_eq!(status("snmp", true, None, old, None, false), WidgetStatus::Down);
    let timeout = Some("Timed out after 5 s");
    assert_eq!(status("snmp", true, timeout, fresh, None, false), WidgetStatus::Down);
    assert_eq!(status("snmp", true, None, None, None, false), WidgetStatus::Paused);
    // Une alerte en cours le dégrade.
    assert_eq!(status("snmp", true, None, fresh, None, true), WidgetStatus::Degraded);
    // Sonde : la vérité est dans son résultat.
    assert_eq!(status("http", true, None, None, Some(true), false), WidgetStatus::Up);
    assert_eq!(status("http", true, None, fresh, Some(false), false), WidgetStatus::Down);
    assert_eq!(status("http", true, None, fresh, None, false), WidgetStatus::Paused);
    // Une erreur de configuration n'est pas une panne.
    assert_eq!(
        status("proxmox", true, Some("Authentication failed: bad token"), fresh, None, false),
        WidgetStatus::Degraded
    );
}

fn monitor(id: i64, name: &str) -> Monitor {
    Monitor {
        id,
        name: name.to_string(),
        kind: "dummy".to_string(),
        address: String::new(),
        category: Category::Servers,
        status: WidgetStatus::Up,
        site: usize::MAX,
        created_at: None,
    }
}

#[test]
fn les_moniteurs_se_rangent_sous_le_site_de_leur_relais() {
    // 10 et 20 sont des agents relais ; 1 passe par 10, 2 par 20, 3 par le
    // serveur, 10 est lui-même au site de Lyon.
    let relays: HashMap<i64, String> =
        HashMap::from([(10, "Lyon".to_string()), (20, "Annecy".to_string())]);
    assert_eq!(sites::site_name(1, Some(10), &relays), Some("Lyon"));
    assert_eq!(sites::site_name(10, None, &relays), Some("Lyon"));
    assert_eq!(sites::site_name(3, None, &relays), None);
    // Un relais inconnu (supprimé) laisse le moniteur au site local.
    assert_eq!(sites::site_name(4, Some(99), &relays), None);

    let entries = vec![
        (Some("Lyon".to_string()), monitor(1, "lyon-nas")),
        (Some("Annecy".to_string()), monitor(2, "annecy-router")),
        (None, monitor(3, "home-nas")),
        (Some("Lyon".to_string()), monitor(10, "lyon-relay")),
    ];
    let fleet = sites::group(entries, "Home");
    let names: Vec<(&str, &str)> =
        fleet.sites.iter().map(|s| (s.key.as_str(), s.name.as_str())).collect();
    assert_eq!(names, [(LOCAL_KEY, "Home"), ("relay:Annecy", "Annecy"), ("relay:Lyon", "Lyon")]);
    assert_eq!(fleet.site_of(1), "Lyon");
    assert_eq!(fleet.site_of(10), "Lyon");
    assert_eq!(fleet.site_of(2), "Annecy");
    assert_eq!(fleet.site_of(3), "Home");
    assert_eq!(fleet.monitor(1).map(|m| m.name.as_str()), Some("lyon-nas"));
}

#[test]
fn un_parc_sans_moniteur_local_n_a_pas_de_site_local_vide() {
    let fleet = sites::group(vec![(Some("Lyon".to_string()), monitor(1, "lyon-nas"))], "Home");
    assert_eq!(fleet.sites.len(), 1);
    assert_eq!(fleet.sites[0].key, "relay:Lyon");
    // Un parc vide garde un site, au nom de l'instance.
    let empty = sites::group(Vec::new(), "Home");
    assert_eq!(empty.sites.len(), 1);
    assert_eq!(empty.sites[0].key, LOCAL_KEY);
    assert_eq!(empty.sites[0].name, "Home");
    assert!(empty.monitors.is_empty());
}

#[test]
fn un_meme_site_ecrit_avec_des_casses_differentes_ne_donne_qu_une_cle_par_nom() {
    let entries = vec![
        (Some("Lyon".to_string()), monitor(1, "a")),
        (Some("lyon".to_string()), monitor(2, "b")),
        (Some("Lyon".to_string()), monitor(3, "c")),
    ];
    let fleet = sites::group(entries, "Home");
    let keys: Vec<&str> = fleet.sites.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["relay:Lyon", "relay:lyon"]);
    assert_eq!(fleet.site_of(1), "Lyon");
    assert_eq!(fleet.site_of(3), "Lyon");
    assert_eq!(fleet.site_of(2), "lyon");
}
