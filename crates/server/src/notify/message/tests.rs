//! Tests unitaires du module.

use super::*;
use crate::alerting::model::Severity;

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + seconds, 0).expect("valid timestamp")
}

fn item(name: &str, reason: NotifyReason, severity: Severity) -> GroupItem {
    GroupItem {
        fingerprint: name.to_string(),
        rule_uid: name.to_string(),
        rule_name: name.to_string(),
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

#[test]
fn la_ligne_cite_ce_que_l_alerte_a_constate() {
    let mut web = item("Website changed", NotifyReason::Recurrence, Severity::Info);
    web.series_key = r#"dumbmonit_webchange_last_check_changes{host="site",target="3"}"#.into();
    web.detail = Some("/pricing, /contact and 2 more".to_string());
    web.value = Some(4.0);
    web.threshold = 0.0;
    web.unit = String::new();
    web.since = Some(at(3600));
    let message = render(&group(vec![web]));
    assert_eq!(
        message.text,
        "Info · Website changed — /pricing, /contact and 2 more — 4 (threshold > 0), for 0 s"
    );
}

fn group(items: Vec<GroupItem>) -> AlertGroup {
    AlertGroup {
        target_id: Some(1),
        target_name: "nas".to_string(),
        target_kind: "snmp".to_string(),
        target_tags: std::collections::BTreeMap::new(),
        severity: Severity::Warning,
        items,
        channels: Vec::new(),
        client_id: None,
        at: at(3600),
    }
}

#[test]
fn une_alerte_unique_donne_un_titre_explicite() {
    let message =
        render(&group(vec![item("Disk full", NotifyReason::Firing, Severity::Warning)]));
    assert_eq!(message.title, "nas — Disk full");
    assert!(message.text.contains("95 %"));
    assert!(message.text.contains("threshold > 90 %"));
    assert!(message.text.contains("for 1 h"));
}

#[test]
fn la_ligne_porte_le_mot_de_severite_a_cote_du_pictogramme() {
    let text =
        |severity| render(&group(vec![item("Disk full", NotifyReason::Firing, severity)])).text;
    assert!(text(Severity::Critical).starts_with("Critical · Disk full"));
    assert!(text(Severity::Warning).starts_with("Warning · Disk full"));
    assert!(text(Severity::Info).starts_with("Info · Disk full"));
    let resolved =
        render(&group(vec![item("Disk full", NotifyReason::Resolved, Severity::Warning)]));
    assert!(resolved.text.starts_with("Resolved · Disk full"), "{}", resolved.text);
}

#[test]
fn la_ligne_nomme_la_serie_derriere_l_alerte() {
    let mut vm = item("VM or container stopped", NotifyReason::Firing, Severity::Warning);
    vm.series_key = r#"dumbmonit_pve_guest_running{host="pve",name="win11-desktop",node="pve1",target="11",type="qemu",vmid="101"}"#.to_string();
    vm.value = Some(1.0);
    vm.threshold = 0.0;
    vm.unit = String::new();
    let message = render(&group(vec![vm]));
    assert_eq!(
        message.text,
        "Warning · VM or container stopped — win11-desktop (101), for 1 h"
    );

    let mut node = item("Proxmox node offline", NotifyReason::Firing, Severity::Critical);
    node.series_key =
        r#"dumbmonit_pve_node_online{host="pve",node="pve2",target="11"}"#.to_string();
    node.value = Some(1.0);
    node.threshold = 0.0;
    node.unit = String::new();
    assert!(render(&group(vec![node])).text.contains("Proxmox node offline — pve2,"));

    // Deux « Service down » sur le même équipement se distinguent par l'URL.
    let mut service = item("Service down", NotifyReason::Firing, Severity::Warning);
    service.series_key =
        r#"dumbmonit_probe_success{host="bad",probe="http",target="25",url="http://host/x6"}"#
            .to_string();
    assert!(render(&group(vec![service])).text.contains("Service down — http://host/x6 —"));

    // Une série sans autre étiquette que celles de l'équipement n'ajoute rien.
    let mut host = item("Monitor unreachable", NotifyReason::Firing, Severity::Critical);
    host.series_key = r#"dumbmonit_up{host="nas",tag_site="cellar",target="1"}"#.to_string();
    assert!(
        render(&group(vec![host])).text.starts_with("Critical · Monitor unreachable — 95 %")
    );
}

#[test]
fn l_identite_d_une_serie_prefere_les_etiquettes_parlantes() {
    assert_eq!(
        series_identity(r#"m{datastore="main",group="vm/101",host="pbs",target="12"}"#)
            .as_deref(),
        Some("vm/101 · main")
    );
    assert_eq!(
        series_identity(
            r#"m{host="nas",result="fail",target="13",task="Office VMs",task_id="6"}"#
        )
        .as_deref(),
        Some("Office VMs")
    );
    assert_eq!(
        series_identity(r#"m{host="nas",mountpoint="/data",target="1"}"#).as_deref(),
        Some("/data")
    );
    // Sans étiquette connue, les valeurs restantes servent, mais jamais plus de deux.
    assert_eq!(
        series_identity(r#"m{host="sw",index="5",target="4",zone="a"}"#).as_deref(),
        Some("5 · a")
    );
    // Un OSD Ceph se nomme par son identifiant puis son hôte, pas par la classe.
    assert_eq!(
        series_identity(
            r#"dumbmonit_proxmox_ceph_osd_up{ceph_host="pve1",device_class="ssd",osd="osd.3",target="2"}"#
        )
        .as_deref(),
        Some("osd.3 · pve1")
    );
    assert_eq!(series_identity(r#"m{esxi="esx1",target="2"}"#).as_deref(), Some("esx1"));
    assert_eq!(
        series_identity(r#"m{proxy_host="app.example.org",target="2"}"#).as_deref(),
        Some("app.example.org")
    );
    assert_eq!(series_identity("dumbmonit_up"), None);
    assert_eq!(series_identity(r#"m{host="nas",target="1"}"#), None);
    // Une virgule dans une valeur ne coupe pas la clé.
    assert_eq!(
        series_identity(r#"m{host="sw",ifalias="Workshop, rack 2",ifname="g5",target="4"}"#)
            .as_deref(),
        Some("g5 · Workshop, rack 2")
    );
}

#[test]
fn une_regle_tout_ou_rien_ne_repete_pas_sa_valeur() {
    let boolean = |value: f64, operator: &str, threshold: f64| {
        let mut it = item("UPS on battery", NotifyReason::Firing, Severity::Warning);
        it.value = Some(value);
        it.operator = operator.to_string();
        it.threshold = threshold;
        it.unit = String::new();
        render(&group(vec![it])).text
    };
    assert_eq!(boolean(1.0, ">", 0.0), "Warning · UPS on battery, for 1 h");
    assert_eq!(boolean(1.0, ">=", 1.0), "Warning · UPS on battery, for 1 h");
    assert_eq!(boolean(0.0, "<", 1.0), "Warning · UPS on battery, for 1 h");
    // Un état codé sur plusieurs valeurs reste chiffré : 5 n'est pas un booléen.
    assert!(boolean(5.0, ">", 0.0).contains("— 5 (threshold > 0)"));
    assert!(boolean(3.0, ">=", 3.0).contains("— 3 (threshold >= 3)"));
    // Un pourcentage à 1 % sous un seuil de 0 % n'est pas non plus un booléen.
    let mut pct = item("Disk full", NotifyReason::Firing, Severity::Warning);
    pct.value = Some(1.0);
    pct.threshold = 0.0;
    assert!(render(&group(vec![pct])).text.contains("— 1 % (threshold > 0 %)"));
}

#[test]
fn plusieurs_alertes_donnent_un_titre_compte() {
    let message = render(&group(vec![
        item("Disk full", NotifyReason::Firing, Severity::Warning),
        item("CPU", NotifyReason::Firing, Severity::Critical),
    ]));
    assert_eq!(message.title, "nas — 2 alerts");
    assert_eq!(message.text.lines().count(), 2);
}

#[test]
fn un_groupe_entierement_resolu_le_dit_dans_son_titre() {
    let message =
        render(&group(vec![item("Disk full", NotifyReason::Resolved, Severity::Warning)]));
    assert_eq!(message.title, "Resolved — nas");
    assert!(message.resolved);
    assert_eq!(message.color(), 0x2E_A0_43);
    assert!(!message.text.contains("threshold"), "pointless on a resolution");
}

#[test]
fn une_escalade_est_annoncee_dans_la_ligne() {
    let message =
        render(&group(vec![item("CPU", NotifyReason::Escalation, Severity::Critical)]));
    assert!(message.text.contains("escalation"));
}

#[test]
fn un_relais_d_escalade_se_lit_dans_la_ligne() {
    let message = render(&group(vec![item("CPU", NotifyReason::Unacked, Severity::Critical)]));
    assert!(message.text.contains("still unacknowledged"), "{}", message.text);
}

#[test]
fn le_score_d_anomalie_apparait_quand_il_existe() {
    let mut anomalie = item("Unusual traffic", NotifyReason::Firing, Severity::Info);
    anomalie.score = Some(7.28);
    let message = render(&group(vec![anomalie]));
    assert!(message.text.contains("7.3 σ"), "{}", message.text);
}

#[test]
fn les_valeurs_sont_lisibles() {
    assert_eq!(format_value(95.0, "%"), "95 %");
    assert_eq!(format_value(95.46, "%"), "95.5 %");
    assert_eq!(format_value(1234.56, ""), "1235");
    assert_eq!(format_value(f64::NAN, "%"), "n/a");
}

#[test]
fn les_durees_sont_lisibles() {
    assert_eq!(format_duration(TimeDelta::seconds(45)), "45 s");
    assert_eq!(format_duration(TimeDelta::seconds(90)), "1 min");
    assert_eq!(format_duration(TimeDelta::hours(5)), "5 h");
    assert_eq!(format_duration(TimeDelta::days(3)), "3 d");
    assert_eq!(format_duration(TimeDelta::seconds(-10)), "0 s", "clock went backwards");
}

#[test]
fn la_priorite_suit_la_severite() {
    let mut message = render(&group(vec![item("x", NotifyReason::Firing, Severity::Critical)]));
    message.severity = Severity::Critical;
    assert_eq!(message.priority(), 5);
    message.severity = Severity::Info;
    assert_eq!(message.priority(), 2);
    message.resolved = true;
    assert_eq!(message.priority(), 2);
}

#[test]
fn la_table_de_substitution_couvre_exactement_les_variables_annoncees() {
    let message =
        render(&group(vec![item("Disk full", NotifyReason::Firing, Severity::Warning)]));
    let variables = message.variables();
    for (nom, description) in TEMPLATE_VARIABLES {
        assert!(variables.contains_key(nom), "\"{nom}\" is documented but missing");
        assert!(!description.is_empty(), "\"{nom}\" has no description");
    }
    for nom in variables.keys() {
        let documentee = TEMPLATE_VARIABLES.iter().any(|(known, _)| known == nom);
        assert!(documentee, "\"{nom}\" exists but is not documented");
    }
}

#[test]
fn les_variables_decrivent_l_alerte_en_tete_de_groupe() {
    let message = render(&group(vec![
        item("Disk full", NotifyReason::Firing, Severity::Warning),
        item("CPU", NotifyReason::Firing, Severity::Critical),
    ]));
    let vars = message.variables();
    assert_eq!(vars["rule"], "Disk full");
    assert_eq!(vars["target"], "nas");
    assert_eq!(vars["count"], "2");
    assert_eq!(vars["status"], "firing");
    assert_eq!(vars["status_fr"], "en cours");
    assert_eq!(vars["severity"], "warning");
    assert_eq!(vars["severity_fr"], "avertissement");
    assert_eq!(vars["value"], "95 %");
    assert_eq!(vars["value_raw"], "95", "usable as is as a JSON number");
    assert_eq!(vars["threshold_raw"], "90");
    assert_eq!(vars["unit"], "%");
    assert_eq!(vars["operator"], ">");
    assert_eq!(vars["color_hex"], "#D98A00");
    assert_eq!(vars["emoji"], "WARN");
    assert_eq!(vars["source"], "dumbmonit");
    // Complétés par le canal, qui seul connaît son URL publique et son secret.
    assert_eq!(vars["link"], "");
    assert_eq!(vars["token"], "");
}

#[test]
fn une_alerte_sans_valeur_laisse_les_variables_chiffrees_vides() {
    // Vide plutôt que « null » ou « NaN » : un gabarit JSON reste valide, et un
    // SMS ne part pas avec « valeur : NaN ».
    let mut sans_valeur = item("Host unreachable", NotifyReason::Firing, Severity::Critical);
    sans_valeur.value = None;
    let vars = render(&group(vec![sans_valeur])).variables();
    assert_eq!(vars["value"], "");
    assert_eq!(vars["value_raw"], "");
}

#[test]
fn une_resolution_le_dit_dans_ses_variables() {
    let vars =
        render(&group(vec![item("Disk full", NotifyReason::Resolved, Severity::Warning)]))
            .variables();
    assert_eq!(vars["status"], "resolved");
    assert_eq!(vars["status_fr"], "résolu");
    assert_eq!(vars["emoji"], "OK");
}

#[test]
fn le_message_de_test_ne_pretend_pas_etre_une_alerte() {
    let message = test_message("Home Discord");
    assert!(message.title.contains("Home Discord"));
    assert!(message.text.contains("No alert"));
    assert_eq!(message.severity, Severity::Info);
}

#[test]
fn les_liens_d_action_visent_les_alertes_actives_seulement() {
    let mut firing = item("Disk full", NotifyReason::Firing, Severity::Warning);
    firing.fingerprint = "disk@1".into();
    let mut reminder = item("CPU", NotifyReason::Reminder, Severity::Warning);
    reminder.fingerprint = "cpu@1".into();
    let mut resolved = item("RAM", NotifyReason::Resolved, Severity::Warning);
    resolved.fingerprint = "ram@1".into();
    let d =
        digest(vec![group(vec![firing, reminder.clone(), resolved]), group(vec![reminder])]);
    let fingerprints: Vec<String> =
        actionable_alerts(&d).into_iter().map(|alert| alert.fingerprint).collect();
    assert_eq!(fingerprints, vec!["disk@1", "cpu@1"]);
}

#[test]
fn un_canal_sans_boutons_recoit_les_liens_en_fin_de_texte() {
    let mut message = sample_message(false);
    let plain = message.with_action_links();
    assert_eq!(plain, message, "nothing to add without actions");
    let key = crate::notify::action_link::ActionKey::from_bytes([1; 32]);
    message.actions =
        crate::notify::action_link::build(&key, "https://m.lan", &["a@1".into()], at(0));
    let linked = message.with_action_links();
    assert!(linked.text.contains("\nAcknowledge: https://m.lan/alert-action#"));
    assert!(linked.text.contains("\nSnooze 1 h: https://m.lan/alert-action#"));
    assert!(linked.markdown.contains("[Acknowledge](https://m.lan/alert-action#"));
    assert!(!message.text.contains("alert-action"), "the original is untouched");
    let vars = message.variables();
    assert!(vars["ack_url"].starts_with("https://m.lan/alert-action#"));
    assert!(vars["snooze_url"].starts_with("https://m.lan/alert-action#"));
    assert!(vars["dismiss_url"].starts_with("https://m.lan/alert-action#"));
    assert!(message.without_actions().actions.is_empty());
}

fn digest(groups: Vec<AlertGroup>) -> Digest {
    Digest { groups, resolved_meanwhile: Vec::new(), hold: None, at: at(300) }
}

fn named_group(target: i64, name: &str, items: Vec<GroupItem>) -> AlertGroup {
    let mut group = group(items);
    group.target_id = Some(target);
    group.target_name = name.to_string();
    group
}

#[test]
fn un_resume_d_un_seul_equipement_garde_le_format_habituel() {
    let d = digest(vec![named_group(
        1,
        "nas",
        vec![item("Disk full", NotifyReason::Firing, Severity::Warning)],
    )]);
    let message = render_digest(&d, Some("https://monit.lan/"));
    assert_eq!(message.title, "nas — Disk full");
    assert_eq!(message.link.as_deref(), Some("https://monit.lan/targets/1"));
    assert!(message.text.ends_with("https://monit.lan/targets/1"), "{}", message.text);
}

#[test]
fn un_resume_multi_equipements_compte_et_sectionne() {
    let d = digest(vec![
        named_group(
            1,
            "nas",
            vec![
                item("Disk full", NotifyReason::Firing, Severity::Warning),
                item("Backup old", NotifyReason::Resolved, Severity::Warning),
            ],
        ),
        named_group(2, "router", vec![item("CPU", NotifyReason::Firing, Severity::Critical)]),
    ]);
    let message = render_digest(&d, Some("https://monit.lan"));
    assert_eq!(message.title, "2 alerts, 1 resolved on 2 devices");
    assert!(message.text.contains("nas:\n"), "{}", message.text);
    assert!(message.text.contains("router:\n"), "{}", message.text);
    assert!(message.text.contains("https://monit.lan/targets/2"));
    assert_eq!(message.count, 3);
    assert!(!message.resolved);
    assert_eq!(message.severity, Severity::Critical);
    assert_eq!(message.target_name, "2 devices");
}

#[test]
fn un_resume_trop_long_est_tronque_avec_un_compte() {
    let items: Vec<GroupItem> = (0..DIGEST_MAX_LINES + 3)
        .map(|i| item(&format!("rule {i:02}"), NotifyReason::Firing, Severity::Warning))
        .collect();
    let d = digest(vec![named_group(1, "nas", items), named_group(2, "b", vec![])]);
    let message = render_digest(&d, None);
    assert!(message.text.contains("…and 3 more alerts"), "{}", message.text);
}

#[test]
fn la_fin_des_heures_calmes_mentionne_ce_qui_s_est_resolu_entre_temps() {
    let mut d = digest(vec![named_group(
        1,
        "nas",
        vec![item("Disk full", NotifyReason::Firing, Severity::Warning)],
    )]);
    d.hold = Some(Hold::Quiet);
    d.resolved_meanwhile =
        vec![("router".to_string(), item("CPU", NotifyReason::Resolved, Severity::Warning))];
    let message = render_digest(&d, None);
    assert!(
        message.title.starts_with("Quiet hours over — 1 alert, 1 resolved on nas"),
        "{}",
        message.title
    );
    assert!(
        message.text.contains("Resolved during quiet hours: CPU (router)"),
        "{}",
        message.text
    );
}

#[test]
fn une_ligne_de_battement_porte_sa_note() {
    let mut flapping = item("Service", NotifyReason::Flapping, Severity::Warning);
    flapping.note = Some("flapping: 4 changes in 30 min".to_string());
    let message = render(&group(vec![flapping]));
    assert!(message.text.starts_with("Flapping · Service"), "{}", message.text);
    assert!(message.text.contains("— flapping: 4 changes in 30 min"));
}
