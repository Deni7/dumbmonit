use std::time::Duration;

use super::*;

fn target(interval_secs: u64, enabled: bool) -> Target {
    Target {
        id: 1,
        name: "NAS".into(),
        address: "10.0.0.5".into(),
        kind: "dummy".into(),
        profile_id: None,
        parent_id: None,
        interval: Duration::from_secs(interval_secs),
        enabled,
        tags: BTreeMap::new(),
        credential: dumbmonit_proto::Credential::None,
        group_name: String::new(),
        position: 0,
    }
}

#[test]
fn the_classic_state_follows_the_interface_rules() {
    let now = Utc::now();
    let fresh = rfc3339(now - TimeDelta::seconds(30));
    let stale = rfc3339(now - TimeDelta::seconds(600));

    assert_eq!(classic_state(&target(60, false), None, None, now), DeviceState::Disabled);
    assert_eq!(
        classic_state(&target(60, true), Some(&fresh), Some("timeout"), now),
        DeviceState::Unreachable
    );
    assert_eq!(classic_state(&target(60, true), None, None, now), DeviceState::Waiting);
    assert_eq!(classic_state(&target(60, true), Some(&fresh), None, now), DeviceState::Reporting);
    assert_eq!(classic_state(&target(60, true), Some(&stale), None, now), DeviceState::Unreachable);
    // La tolérance ne descend jamais sous 90 s, même à 10 s de période.
    let recent = rfc3339(now - TimeDelta::seconds(80));
    assert_eq!(classic_state(&target(10, true), Some(&recent), None, now), DeviceState::Reporting);
}

#[test]
fn thinning_keeps_at_most_sixty_points() {
    let values: Vec<(f64, String)> = (0..1000).map(|i| (i as f64, i.to_string())).collect();
    let thinned = thin(values);
    assert!(thinned.len() <= MAX_POINTS, "{}", thinned.len());
    assert_eq!(thinned[0].0, 0.0);
    let short: Vec<(f64, String)> = (0..10).map(|i| (i as f64, i.to_string())).collect();
    assert_eq!(thin(short).len(), 10);
}

#[test]
fn option_values_become_text() {
    assert_eq!(option_text(&json!(" public ")).as_deref(), Some("public"));
    assert_eq!(option_text(&json!(161)).as_deref(), Some("161"));
    assert_eq!(option_text(&json!(true)).as_deref(), Some("true"));
    assert_eq!(option_text(&json!({ "a": 1 })), None);
}

#[test]
fn arguments_are_read_tolerantly() {
    let args = Args(json!({ "id": "3", "enabled": "yes", "hours": "2.5", "name": "  " }));
    assert_eq!(args.i64("id"), Some(3));
    assert_eq!(args.bool("enabled"), Some(true));
    assert_eq!(args.f64("hours"), Some(2.5));
    assert_eq!(args.str("name"), None);
    assert_eq!(args.device_ref().as_deref(), Some("3"));
}
