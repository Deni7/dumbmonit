use super::*;
use crate::alerting::machine::EffectivePhase;

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + seconds, 0).expect("valid timestamp")
}

fn item(fp: &str, reason: NotifyReason, severity: Severity) -> GroupItem {
    GroupItem {
        fingerprint: fp.to_string(),
        rule_uid: fp.to_string(),
        rule_name: fp.to_string(),
        severity,
        reason,
        value: Some(95.0),
        score: None,
        unit: "%".to_string(),
        operator: ">".to_string(),
        threshold: 90.0,
        series_key: "s".to_string(),
        since: Some(at(0)),
        phase: EffectivePhase::Firing,
        note: None,
        detail: None,
    }
}

fn sent(fp: &str, reason: NotifyReason, seconds: i64) -> Sent {
    Sent { fingerprint: fp.to_string(), reason, at: at(seconds) }
}

fn fingerprints(items: &[GroupItem]) -> Vec<&str> {
    items.iter().map(|item| item.fingerprint.as_str()).collect()
}

fn on() -> Prefs {
    Prefs { email: true, ..Prefs::default() }
}

#[test]
fn nothing_is_sent_until_asked_for() {
    let prefs = Prefs::default();
    assert!(!prefs.active());
    assert_eq!(prefs.min_severity, Severity::Warning);
    assert!(on().active());
    assert!(Prefs { webpush: true, ..Prefs::default() }.active());
}

#[test]
fn the_minimum_severity_filters_problems_but_not_their_resolution() {
    let prefs = Prefs { min_severity: Severity::Critical, ..on() };
    let items = [
        item("disk", NotifyReason::Firing, Severity::Warning),
        item("down", NotifyReason::Firing, Severity::Critical),
    ];
    let chosen = select_items(&prefs, &items, &[], &HashSet::new(), at(10));
    assert_eq!(fingerprints(&chosen), ["down"]);
}

#[test]
fn a_resolution_only_follows_what_was_announced_to_this_account() {
    let prefs = on();
    let items = [
        item("down", NotifyReason::Resolved, Severity::Critical),
        item("never-seen", NotifyReason::Resolved, Severity::Critical),
    ];
    let log = [sent("down", NotifyReason::Firing, 0)];
    let chosen = select_items(&prefs, &items, &log, &HashSet::new(), at(60));
    assert_eq!(fingerprints(&chosen), ["down"]);

    // Déjà annoncée résolue : pas deux fois.
    let log = [sent("down", NotifyReason::Firing, 0), sent("down", NotifyReason::Resolved, 30)];
    assert!(select_items(&prefs, &items, &log, &HashSet::new(), at(60)).is_empty());

    // Résolutions coupées.
    let quiet = Prefs { notify_resolved: false, ..on() };
    let log = [sent("down", NotifyReason::Firing, 0)];
    assert!(select_items(&quiet, &items, &log, &HashSet::new(), at(60)).is_empty());
}

#[test]
fn the_minimum_interval_holds_back_a_repeat() {
    let prefs = Prefs { min_interval_secs: 600, ..on() };
    let items = [item("cpu", NotifyReason::Reminder, Severity::Warning)];
    let log = [sent("cpu", NotifyReason::Firing, 0)];
    assert!(select_items(&prefs, &items, &log, &HashSet::new(), at(300)).is_empty());
    assert_eq!(select_items(&prefs, &items, &log, &HashSet::new(), at(900)).len(), 1);
    // Sans délai minimal, le rappel décidé par la règle part.
    assert_eq!(select_items(&on(), &items, &log, &HashSet::new(), at(300)).len(), 1);
}

#[test]
fn flapping_holds_and_escalation_relays_never_reach_a_person() {
    let items = [
        item("flap", NotifyReason::Firing, Severity::Critical),
        item("relay", NotifyReason::Unacked, Severity::Critical),
        item("ok", NotifyReason::Firing, Severity::Critical),
    ];
    let held = HashSet::from(["flap".to_string()]);
    let chosen = select_items(&on(), &items, &[], &held, at(0));
    assert_eq!(fingerprints(&chosen), ["ok"]);
}

#[test]
fn quiet_hours_let_only_critical_alerts_through() {
    // 1 700 000 000 tombe un mardi (jour 1), à 22 h 13 UTC.
    let quiet = Schedule::Weekly {
        days: vec![1],
        start_minute: 22 * 60,
        end_minute: 23 * 60,
        utc_offset_minutes: 0,
        timezone: Some("UTC".into()),
    };
    let prefs = Prefs { quiet_hours: Some(quiet), min_severity: Severity::Info, ..on() };
    assert!(prefs.in_quiet_hours(at(0)));
    let items = [
        item("warn", NotifyReason::Firing, Severity::Warning),
        item("crit", NotifyReason::Firing, Severity::Critical),
    ];
    let chosen = select_items(&prefs, &items, &[], &HashSet::new(), at(0));
    assert_eq!(fingerprints(&chosen), ["crit"]);
    // Après la fenêtre, tout repasse.
    let later = select_items(&prefs, &items, &[], &HashSet::new(), at(2 * 3600));
    assert_eq!(later.len(), 2);
}

#[test]
fn the_scope_picks_folders_tags_or_team_folders() {
    let tags = BTreeMap::from([("room".to_string(), "Cave".to_string())]);
    let none = BTreeMap::new();
    let nas = TargetFacts { id: Some(1), folder: Some("Paris/Rack A"), tags: &tags };
    let web = TargetFacts { id: Some(2), folder: Some("Lyon"), tags: &none };
    let orphan = TargetFacts { id: None, folder: None, tags: &none };

    let all = on();
    assert!(in_scope(&all, &[], &nas) && in_scope(&all, &[], &orphan));

    let folders = Prefs { scope: Scope::Pick, folders: vec!["Paris".into()], ..on() };
    assert!(in_scope(&folders, &[], &nas));
    assert!(!in_scope(&folders, &[], &web));
    assert!(!in_scope(&folders, &[], &orphan), "no device, no folder");

    let tagged = Prefs { scope: Scope::Pick, tags: vec!["room=cave".into()], ..on() };
    assert!(in_scope(&tagged, &[], &nas), "the value compares without case");
    assert!(!in_scope(&tagged, &[], &web));
    let key_only = Prefs { scope: Scope::Pick, tags: vec!["room".into()], ..on() };
    assert!(in_scope(&key_only, &[], &nas));

    let teams = Prefs { scope: Scope::Teams, ..on() };
    assert!(in_scope(&teams, &["Lyon".to_string()], &web));
    assert!(!in_scope(&teams, &["Lyon".to_string()], &nas));
    assert!(!in_scope(&teams, &[], &web), "no team, nothing");
}

#[test]
fn a_saved_choice_is_checked_and_cleaned() {
    let raw = Prefs {
        scope: Scope::Pick,
        folders: vec![" Paris//Rack ".into(), "Paris/Rack".into(), String::new()],
        tags: vec![" room = cave ".into()],
        ..on()
    };
    let clean = raw.validated().expect("valid");
    assert_eq!(clean.folders, ["Paris/Rack"]);
    assert_eq!(clean.tags, ["room=cave"]);

    assert!(Prefs { scope: Scope::Pick, ..on() }.validated().is_err(), "nothing picked");
    assert!(Prefs { min_interval_secs: 90_000, ..on() }.validated().is_err());
    assert!(Prefs { tags: vec!["=x".into()], ..on() }.validated().is_err());
    let too_many: Vec<String> = (0..60).map(|n| format!("f{n}")).collect();
    assert!(Prefs { folders: too_many, ..on() }.validated().is_err());

    let once = Schedule::Once { starts_at: at(0), ends_at: at(60) };
    assert!(Prefs { quiet_hours: Some(once), ..on() }.validated().is_err());
    let unknown_zone = Schedule::Weekly {
        days: vec![0],
        start_minute: 60,
        end_minute: 120,
        utc_offset_minutes: 0,
        timezone: Some("Mars/Olympus".into()),
    };
    assert!(Prefs { quiet_hours: Some(unknown_zone), ..on() }.validated().is_err());
    let paris = Schedule::Weekly {
        days: vec![0, 6],
        start_minute: 22 * 60,
        end_minute: 7 * 60,
        utc_offset_minutes: 60,
        timezone: Some("Europe/Paris".into()),
    };
    assert!(Prefs { quiet_hours: Some(paris), ..on() }.validated().is_ok());
}

#[test]
fn older_or_partial_documents_read_with_defaults() {
    let prefs: Prefs = serde_json::from_str(r#"{"email":true}"#).unwrap();
    assert_eq!(prefs, on());
    assert_eq!(parse("not json"), Prefs::default());
}
