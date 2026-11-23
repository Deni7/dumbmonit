//! Tests unitaires du module.

use std::collections::BTreeMap;

use super::*;

struct TestDb {
    pool: SqlitePool,
    cipher: Cipher,
    _dir: tempfile::TempDir,
}

async fn setup() -> TestDb {
    let dir = tempfile::tempdir().expect("répertoire temporaire");
    let pool = db::open(&dir.path().join("test.db")).await.expect("base");
    let cipher =
        db::init_cipher(&pool, "secret-de-test-suffisamment-long").await.expect("chiffrement");
    TestDb { pool, cipher, _dir: dir }
}

fn identity(hostname: &str, machine_id: Option<&str>) -> AgentIdentity {
    AgentIdentity {
        hostname: hostname.to_string(),
        os: "linux".into(),
        os_version: Some("Debian GNU/Linux 12".into()),
        kernel_version: Some("6.1.0".into()),
        arch: Some("x86_64".into()),
        agent_version: "0.1.0".into(),
        commands_enabled: Some(true),
        relay: false,
        site: None,
        relay_control: false,
        relay_local: false,
        machine_id: machine_id.map(str::to_string),
        tags: BTreeMap::new(),
        binding_supported: true,
        self_update: None,
        relay_gateway: None,
        relay_gateway_check: None,
        relay_gateway_state: None,
        server_link: None,
        remediation: None,
    }
}

/// Un agent d'avant la liaison : il ignore le secret que le serveur
/// renverrait, donc le serveur ne doit pas lui en attribuer.
fn legacy(hostname: &str, machine_id: Option<&str>) -> AgentIdentity {
    AgentIdentity { binding_supported: false, ..identity(hostname, machine_id) }
}

async fn token(db: &TestDb, _name: &str) -> i64 {
    create_token(&db.pool, TokenPolicy { max_uses: None, expires_in_days: None })
        .await
        .expect("création")
        .0
        .id
}

/// Enrôle une machine et renvoie son enregistrement, en échouant clairement.
async fn enrol(db: &TestDb, identity: &AgentIdentity, token_id: i64) -> Registration {
    register(&db.pool, &db.cipher, identity, token_id, None).await.expect("enregistrement")
}

#[tokio::test]
async fn a_created_token_can_be_verified_then_revoked() {
    let db = setup().await;
    let (record, clear) = create_token(&db.pool, TokenPolicy::default()).await.expect("création");

    let found =
        find_active_token(&db.pool, &token::fingerprint(&clear)).await.expect("vérification");
    assert_eq!(found, Some(record.id));

    assert!(revoke_token(&db.pool, record.id).await.expect("révocation"));
    // Un jeton révoqué ne doit plus jamais ouvrir l'ingestion.
    assert_eq!(
        find_active_token(&db.pool, &token::fingerprint(&clear)).await.expect("vérification"),
        None
    );
    // Deuxième révocation : sans effet, mais sans erreur.
    assert!(!revoke_token(&db.pool, record.id).await.expect("révocation"));
}

#[tokio::test]
async fn the_clear_token_is_never_stored() {
    let db = setup().await;
    let (_, clear) = create_token(&db.pool, TokenPolicy::default()).await.expect("création");

    let row = sqlx::query("SELECT token_hash, prefix FROM agent_tokens")
        .fetch_one(&db.pool)
        .await
        .expect("lecture");
    let stored: String = row.try_get("token_hash").unwrap();
    let prefix: String = row.try_get("prefix").unwrap();

    assert_ne!(stored, clear);
    assert_eq!(stored, token::fingerprint(&clear));
    // Le préfixe affichable ne doit pas suffire à reconstituer le jeton.
    assert!(clear.starts_with(&prefix) && prefix.len() < clear.len());
}

#[tokio::test]
async fn an_unknown_fingerprint_opens_nothing() {
    let db = setup().await;
    token(&db, "parc").await;
    assert_eq!(
        find_active_token(&db.pool, &token::fingerprint("dmon_inconnu")).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn an_unknown_machine_registers_itself() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    let first = register(&db.pool, &db.cipher, &identity("nas", Some("id-nas")), token_id, None)
        .await
        .expect("enregistrement");
    assert!(first.created, "la première présentation crée la cible");

    let target = db::targets::get(&db.pool, &db.cipher, first.target_id)
        .await
        .expect("lecture")
        .expect("cible");
    assert_eq!(target.name, "nas");
    assert_eq!(target.kind, "agent");
    assert!(target.enabled);
}

#[tokio::test]
async fn a_known_machine_does_not_register_twice() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    let identity = identity("nas", Some("id-nas"));

    let first = enrol(&db, &identity, token_id).await;
    let secret = first.issued_secret.clone().expect("secret de liaison");
    let second = register(&db.pool, &db.cipher, &identity, token_id, Some(&secret))
        .await
        .expect("second lot");

    assert_eq!(first.target_id, second.target_id);
    assert!(!second.created);
    assert!(second.issued_secret.is_none(), "le secret n'est remis qu'une fois");
    assert_eq!(db::targets::list(&db.pool, &db.cipher).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_relay_gateway_opens_by_default_closes_on_demand_and_keeps_its_check() {
    let db = setup().await;
    let token_id = token(&db, "relais").await;
    let mut relay = identity("relais", Some("id-relais"));
    let first = enrol(&db, &relay, token_id).await;
    let secret = first.issued_secret.clone().expect("secret");
    let target = first.target_id;
    let gateway = |setting: Option<RelaySetting>| setting.expect("machine").gateway_port;

    // Pas un relais : pas de passerelle.
    assert_eq!(gateway(relay_setting(&db.pool, target).await.unwrap()), None);
    // Relais : passerelle ouverte d'office, sans attendre qu'on la demande.
    assert!(set_relay_wanted(&db.pool, target, true, Some("Lyon")).await.unwrap());
    let open = Some(dumbmonit_proto::DEFAULT_RELAY_GATEWAY_PORT);
    assert_eq!(gateway(relay_setting(&db.pool, target).await.unwrap()), open);
    let info = host(&db.pool, target).await.unwrap().expect("machine");
    assert_eq!(info.relay_gateway_wanted, open);

    // Le relais rapporte l'essai de sa passerelle : rangé, borné.
    relay.relay = true;
    relay.relay_gateway = Some(8443);
    relay.relay_gateway_check = Some(GatewayCheck {
        address: "192.168.10.5:8443".into(),
        ok: false,
        error: Some("x".repeat(5000)),
        checked_at_ms: 1_700_000_000_000,
    });
    register(&db.pool, &db.cipher, &relay, token_id, Some(&secret)).await.expect("lot");
    let check = host(&db.pool, target).await.unwrap().unwrap().relay_gateway_check.unwrap();
    assert_eq!(check.address, "192.168.10.5:8443");
    assert!(!check.ok);
    let error = check.error.unwrap();
    assert_eq!(error.chars().count(), dumbmonit_proto::MAX_GATEWAY_CHECK_ERROR);

    // Fermée depuis l'interface : elle le reste, avec l'adresse gardée.
    assert!(set_relay_gateway(&db.pool, target, None, Some("192.168.10.5")).await.unwrap());
    let setting = relay_setting(&db.pool, target).await.unwrap().unwrap();
    assert_eq!(setting.gateway_port, None);
    assert_eq!(setting.gateway_host.as_deref(), Some("192.168.10.5"));
    assert!(set_relay_gateway(&db.pool, target, Some(9443), None).await.unwrap());
    assert_eq!(gateway(relay_setting(&db.pool, target).await.unwrap()), Some(9443));
}

#[tokio::test]
async fn a_relay_says_why_its_gateway_does_not_listen_and_the_cause_is_cleared() {
    let db = setup().await;
    let token_id = token(&db, "relais").await;
    let mut relay = identity("relais", Some("id-relais"));
    let first = enrol(&db, &relay, token_id).await;
    let secret = first.issued_secret.clone().expect("secret");
    let target = first.target_id;
    let state = async || host(&db.pool, target).await.unwrap().unwrap().relay_gateway_state;
    assert_eq!(state().await, None);

    // Le port est pris sur le relais : la cause est rangée, bornée.
    relay.relay = true;
    relay.relay_gateway_state = Some(GatewayState {
        wanted: "0.0.0.0:8443".into(),
        listen: None,
        error: Some("y".repeat(5000)),
        local: false,
        encrypted: true,
    });
    register(&db.pool, &db.cipher, &relay, token_id, Some(&secret)).await.expect("lot");
    let failed = state().await.expect("état");
    assert_eq!(failed.wanted, "0.0.0.0:8443");
    assert_eq!(failed.listen, None);
    assert_eq!(failed.error.unwrap().chars().count(), dumbmonit_proto::MAX_GATEWAY_CHECK_ERROR);

    // Un agent qui n'est plus relais ne garde rien.
    relay.relay = false;
    register(&db.pool, &db.cipher, &relay, token_id, Some(&secret)).await.expect("lot");
    assert_eq!(state().await, None);
}

#[tokio::test]
async fn the_server_an_agent_reports_using_is_kept_bounded_then_cleared() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    let mut agent = identity("nas", Some("id-nas"));
    let first = enrol(&db, &agent, token_id).await;
    let secret = first.issued_secret.clone().expect("secret");
    let target = first.target_id;
    assert!(host(&db.pool, target).await.unwrap().unwrap().server_link.is_none());

    // Sur le secours : rangé, borné (un rang hors liste est ramené).
    agent.server_link = Some(ServerLink {
        active: 7,
        servers: 2,
        url: "https://b.example:8443".into(),
        since_ms: Some(1_700_000_000_000),
        primary_error: Some("x".repeat(5000)),
    });
    register(&db.pool, &db.cipher, &agent, token_id, Some(&secret)).await.expect("lot");
    let link = host(&db.pool, target).await.unwrap().unwrap().server_link.expect("lien");
    assert_eq!((link.active, link.servers), (1, 2));
    assert_eq!(link.url, "https://b.example:8443");
    assert_eq!(link.since_ms, Some(1_700_000_000_000));
    assert_eq!(link.primary_error.unwrap().chars().count(), MAX_SERVER_LINK_TEXT);

    // Plus de secours dans sa configuration : plus rien d'affiché.
    agent.server_link = None;
    register(&db.pool, &db.cipher, &agent, token_id, Some(&secret)).await.expect("lot");
    assert!(host(&db.pool, target).await.unwrap().unwrap().server_link.is_none());
}

#[test]
fn a_gateway_never_set_is_open_for_a_relay_only() {
    assert_eq!(gateway_wanted_of(None, true), Some(8443));
    assert_eq!(gateway_wanted_of(None, false), None);
    assert_eq!(gateway_wanted_of(Some(0), true), None, "fermée depuis l'interface");
    assert_eq!(gateway_wanted_of(Some(9443), false), Some(9443));
    assert_eq!(gateway_wanted_of(Some(70_000), true), None);
}

#[tokio::test]
async fn renaming_a_machine_does_not_create_a_second_target() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    let before = enrol(&db, &identity("nas", Some("id-stable")), token_id).await;
    let secret = before.issued_secret.clone().expect("secret");
    // Même identifiant machine, nom d'hôte différent : c'est la même machine.
    let after = register(
        &db.pool,
        &db.cipher,
        &identity("nas-cave", Some("id-stable")),
        token_id,
        Some(&secret),
    )
    .await
    .unwrap();

    assert_eq!(before.target_id, after.target_id);
    assert_eq!(db::targets::list(&db.pool, &db.cipher).await.unwrap().len(), 1);
}

#[tokio::test]
async fn without_a_machine_id_the_hostname_identifies_the_machine() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    let first = enrol(&db, &identity("Pi-Salon", None), token_id).await;
    let secret = first.issued_secret.clone().expect("secret");
    // La casse ne doit pas suffire à dédoubler une machine.
    let second =
        register(&db.pool, &db.cipher, &identity("pi-salon", None), token_id, Some(&secret))
            .await
            .unwrap();

    assert_eq!(first.target_id, second.target_id);
}

#[tokio::test]
async fn the_freshness_of_a_machine_is_recorded_at_each_batch() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    let registration = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;

    // Aucun lot reçu pour l'instant : la machine est enregistrée sans être vue.
    assert_eq!(last_seen_ms(&db.pool, registration.target_id).await.unwrap(), None);

    record_batch(&db.pool, registration.target_id, 1_700_000_000_000, 42).await.unwrap();
    assert_eq!(
        last_seen_ms(&db.pool, registration.target_id).await.unwrap(),
        Some(1_700_000_000_000)
    );
}

#[tokio::test]
async fn the_agent_capabilities_follow_its_last_batch() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    // Un agent d'avant le canal de commandes ne dit rien : il n'est pas
    // réputé capable pour autant.
    let mut identity = identity("nas", Some("id-nas"));
    identity.commands_enabled = None;
    let registration = enrol(&db, &identity, token_id).await;
    let secret = registration.issued_secret.clone().expect("secret");
    let info = host(&db.pool, registration.target_id).await.unwrap().expect("machine");
    assert_eq!(info.commands_enabled, None);
    assert!(!info.commands_supported());
    assert_eq!(info.agent_version, "0.1.0");

    // Après mise à jour de l'agent, le lot suivant suffit.
    identity.commands_enabled = Some(true);
    identity.agent_version = "0.2.0".into();
    register(&db.pool, &db.cipher, &identity, token_id, Some(&secret)).await.unwrap();
    let info = host(&db.pool, registration.target_id).await.unwrap().expect("machine");
    assert!(info.commands_supported());
    assert_eq!(info.agent_version, "0.2.0");

    // `commands: false` dans la configuration de l'agent.
    identity.commands_enabled = Some(false);
    register(&db.pool, &db.cipher, &identity, token_id, Some(&secret)).await.unwrap();
    let info = host(&db.pool, registration.target_id).await.unwrap().expect("machine");
    assert_eq!(info.commands_enabled, Some(false));
    assert!(!info.commands_supported());

    // Une cible sans agent n'a pas de machine.
    assert!(host(&db.pool, 4_242).await.unwrap().is_none());
}

#[tokio::test]
async fn a_target_that_is_not_an_agent_has_no_freshness() {
    let db = setup().await;
    assert_eq!(last_seen_ms(&db.pool, 4_242).await.unwrap(), None);
}

#[tokio::test]
async fn a_manually_created_agent_target_is_adopted() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    // L'utilisateur a préparé la cible dans l'interface avant d'installer
    // l'agent : la machine doit s'y rattacher, pas échouer sur l'unicité.
    let manual = db::targets::create(
        &db.pool,
        &db.cipher,
        &db::targets::TargetInput {
            name: "Serveur de sauvegarde".into(),
            address: "id-nas".into(),
            kind: "agent".into(),
            profile_id: None,
            parent_id: None,
            via_agent: None,
            interval: std::time::Duration::from_secs(60),
            enabled: true,
            tags: BTreeMap::new(),
            credential: Some(Credential::None),
            group_name: String::new(),
        },
    )
    .await
    .unwrap();

    let registration = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;

    assert_eq!(registration.target_id, manual);
    assert!(!registration.created);
    // Le nom choisi par l'utilisateur est préservé : c'est le sien, pas celui
    // que la machine se donne.
    let target = db::targets::get(&db.pool, &db.cipher, manual).await.unwrap().unwrap();
    assert_eq!(target.name, "Serveur de sauvegarde");
}

// ------------------------------------------------------- liaison machine

#[tokio::test]
async fn a_first_contact_binds_the_machine_and_hands_back_its_secret() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    let registration = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;
    let secret = registration.issued_secret.clone().expect("secret remis une fois");
    assert!(registration.bound);
    assert!(secret.starts_with(dumbmonit_proto::AGENT_SECRET_PREFIX));

    // Seule l'empreinte est en base : un vol de la base ne rejoue rien.
    let stored: Option<String> = sqlx::query("SELECT secret_hash FROM agent_hosts")
        .fetch_one(&db.pool)
        .await
        .unwrap()
        .try_get("secret_hash")
        .unwrap();
    assert_eq!(stored, Some(token::fingerprint(&secret)));
}

#[tokio::test]
async fn a_second_machine_cannot_claim_an_existing_registration() {
    // Le cœur de la correction : une machine du parc, porteuse du même jeton
    // de flotte, se présente avec la clé d'identité d'une autre.
    let db = setup().await;
    let token_id = token(&db, "flotte").await;
    let victim = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;

    // Sans secret : refusé.
    let error = register(&db.pool, &db.cipher, &identity("pirate", Some("id-nas")), token_id, None)
        .await
        .expect_err("l'usurpation doit être refusée");
    assert!(matches!(error, RegisterError::BindingMismatch), "{error:?}");

    // Avec un secret inventé : refusé aussi.
    let error = register(
        &db.pool,
        &db.cipher,
        &identity("pirate", Some("id-nas")),
        token_id,
        Some("dmab_0000000000000000"),
    )
    .await
    .expect_err("l'usurpation doit être refusée");
    assert!(matches!(error, RegisterError::BindingMismatch), "{error:?}");

    // Et rien n'a bougé : ni le nom d'hôte de la victime, ni son nombre de cibles.
    let info = host(&db.pool, victim.target_id).await.unwrap().expect("machine");
    assert_eq!(info.hostname, "nas");
    assert_eq!(db::targets::list(&db.pool, &db.cipher).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_machine_cannot_read_or_report_the_commands_of_another() {
    let db = setup().await;
    let token_id = token(&db, "flotte").await;
    let victim = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;
    let secret = victim.issued_secret.clone().expect("secret");

    // L'agent légitime, avec son secret.
    assert_eq!(
        authorise_key(&db.pool, "id-nas", Some(&secret)).await.unwrap(),
        KeyAuth::Allowed { target_id: victim.target_id }
    );
    // Le voisin compromis, avec le jeton de flotte mais pas le secret.
    assert_eq!(authorise_key(&db.pool, "id-nas", None).await.unwrap(), KeyAuth::Denied);
    assert_eq!(
        authorise_key(&db.pool, "id-nas", Some("dmab_pas-le-bon")).await.unwrap(),
        KeyAuth::Denied
    );
    // Une clé qu'aucune machine ne porte.
    assert_eq!(authorise_key(&db.pool, "id-inconnu", None).await.unwrap(), KeyAuth::Unknown);
}

/// Une machine enrôlée avant la liaison, telle que la base la garde : une
/// ligne sans secret, rattachée au jeton qui l'a fait entrer.
async fn legacy_host(db: &TestDb, hostname: &str, key: &str, token_id: i64) -> TargetId {
    let identity = legacy(hostname, Some(key));
    let target_id = db::targets::create(
        &db.pool,
        &db.cipher,
        &db::targets::TargetInput {
            name: hostname.into(),
            address: identity.key(),
            kind: "agent".into(),
            profile_id: None,
            parent_id: None,
            via_agent: None,
            interval: std::time::Duration::from_secs(60),
            enabled: true,
            tags: BTreeMap::new(),
            credential: Some(Credential::None),
            group_name: String::new(),
        },
    )
    .await
    .unwrap();
    insert_host(&db.pool, target_id, &identity.key(), &identity, token_id, None).await.unwrap();
    target_id
}

#[tokio::test]
async fn an_agent_too_old_to_be_bound_is_not_enrolled() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;

    let error = register(&db.pool, &db.cipher, &legacy("vieux", Some("id-vieux")), token_id, None)
        .await
        .expect_err("un agent qui ne sait pas se lier n'entre plus");
    assert!(matches!(error, RegisterError::AgentTooOld), "{error:?}");
    // Rien n'a été créé, et le jeton n'a rien consommé.
    assert!(db::targets::list(&db.pool, &db.cipher).await.unwrap().is_empty());
    assert!(find_host(&db.pool, "id-vieux").await.unwrap().is_none());
}

#[tokio::test]
async fn a_legacy_unbound_machine_is_refused_everywhere_until_re_enrolled() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    let target_id = legacy_host(&db, "vieux-nas", "id-vieux", token_id).await;

    // Ni le binaire d'origine, ni un binaire à jour, même avec son propre jeton.
    for agent in [legacy("vieux-nas", Some("id-vieux")), identity("vieux-nas", Some("id-vieux"))] {
        let error = register(&db.pool, &db.cipher, &agent, token_id, None)
            .await
            .expect_err("une machine jamais liée est refusée");
        assert!(matches!(error, RegisterError::NotBound), "{error:?}");
    }
    // Les commandes et le relais non plus.
    assert_eq!(authorise_key(&db.pool, "id-vieux", None).await.unwrap(), KeyAuth::NotBound);

    // L'interface a de quoi dire quoi faire.
    let info = host(&db.pool, target_id).await.unwrap().expect("machine");
    assert_eq!(info.binding_state(), "unbound");
    assert!(!info.bound);
    assert!(info.bound_at.is_none());

    // Fenêtre ouverte : un binaire trop ancien ne s'y lie toujours pas…
    allow_rebind(&db.pool, target_id).await.unwrap().expect("fenêtre");
    assert!(matches!(
        register(&db.pool, &db.cipher, &legacy("vieux-nas", Some("id-vieux")), token_id, None)
            .await
            .expect_err("trop ancien"),
        RegisterError::AgentTooOld
    ));
    // … un binaire à jour, si : il reçoit son secret, et la fenêtre se referme.
    let bound = enrol(&db, &identity("vieux-nas", Some("id-vieux")), token_id).await;
    assert_eq!(bound.target_id, target_id);
    assert!(bound.bound);
    let secret = bound.issued_secret.expect("secret remis à la liaison");
    assert_eq!(
        authorise_key(&db.pool, "id-vieux", Some(&secret)).await.unwrap(),
        KeyAuth::Allowed { target_id }
    );
    let info = host(&db.pool, target_id).await.unwrap().expect("machine");
    assert_eq!(info.binding_state(), "bound");
    assert!(info.rebind_until.is_none());
}

#[tokio::test]
async fn a_legacy_machine_re_enrolled_with_another_token_needs_a_token_that_can_enrol() {
    let db = setup().await;
    let own = token(&db, "parc").await;
    let target_id = legacy_host(&db, "nas", "id-nas", own).await;
    let (spent, _) = create_token(&db.pool, TokenPolicy::default()).await.expect("création");
    enrol(&db, &identity("autre", Some("id-autre")), spent.id).await;

    allow_rebind(&db.pool, target_id).await.unwrap().expect("fenêtre");
    assert!(matches!(
        register(&db.pool, &db.cipher, &identity("nas", Some("id-nas")), spent.id, None)
            .await
            .expect_err("jeton épuisé"),
        RegisterError::EnrolmentDenied(EnrolmentDenied::Exhausted)
    ));
    let fresh = token(&db, "neuf").await;
    assert!(enrol(&db, &identity("nas", Some("id-nas")), fresh).await.issued_secret.is_some());
}

#[tokio::test]
async fn a_reinstalled_machine_rebinds_only_inside_the_window_someone_opened() {
    let db = setup().await;
    let token_id = token(&db, "parc").await;
    let registration = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;
    let old_secret = registration.issued_secret.clone().expect("secret");

    // Le disque a été refait : l'agent revient sans secret, et se fait jeter.
    assert!(matches!(
        register(&db.pool, &db.cipher, &identity("nas", Some("id-nas")), token_id, None)
            .await
            .expect_err("liaison exigée"),
        RegisterError::BindingMismatch
    ));

    // Quelqu'un ouvre la fenêtre depuis l'interface.
    let until = allow_rebind(&db.pool, registration.target_id).await.unwrap();
    assert!(until.is_some());
    let rebound = enrol(&db, &identity("nas", Some("id-nas")), token_id).await;
    let new_secret = rebound.issued_secret.clone().expect("nouveau secret");
    assert_ne!(new_secret, old_secret, "la reliaison change le secret");

    // La fenêtre s'est refermée derrière elle, et l'ancien secret ne vaut plus.
    assert!(matches!(
        register(
            &db.pool,
            &db.cipher,
            &identity("nas", Some("id-nas")),
            token_id,
            Some(&old_secret)
        )
        .await
        .expect_err("l'ancien secret est périmé"),
        RegisterError::BindingMismatch
    ));
    assert!(host(&db.pool, registration.target_id).await.unwrap().unwrap().rebind_until.is_none());

    // Une cible qui n'a pas d'agent n'ouvre aucune fenêtre.
    assert_eq!(allow_rebind(&db.pool, 4_242).await.unwrap(), None);
}

// ------------------------------------------------------- portée du jeton

#[tokio::test]
async fn a_single_use_token_enrols_one_machine_and_not_a_second() {
    let db = setup().await;
    let (record, _) = create_token(&db.pool, TokenPolicy::default()).await.expect("création");
    assert_eq!(record.max_uses, Some(1), "l'usage unique est le défaut");

    let first = enrol(&db, &identity("portable", Some("id-1")), record.id).await;
    let secret = first.issued_secret.clone().expect("secret");

    let error = register(&db.pool, &db.cipher, &identity("autre", Some("id-2")), record.id, None)
        .await
        .expect_err("une seconde machine doit être refusée");
    assert!(
        matches!(error, RegisterError::EnrolmentDenied(EnrolmentDenied::Exhausted)),
        "{error:?}"
    );

    // La machine déjà entrée continue pourtant de pousser : un jeton épuisé
    // n'éteint pas ce qu'il a fait entrer.
    assert!(
        register(
            &db.pool,
            &db.cipher,
            &identity("portable", Some("id-1")),
            record.id,
            Some(&secret)
        )
        .await
        .is_ok()
    );
}

#[test]
fn token_names_are_opaque_and_uniform() {
    let first = generate_token_name();
    assert_eq!(first.len(), 14);
    assert!(is_token_name(&first), "{first}");
    assert_ne!(first, generate_token_name());
    assert!(!is_token_name("Agent enrolment · 2026-11-02"));
    assert!(!is_token_name("tok_ABCDEFGHIJ"));
    assert!(!is_token_name("tok_abc"));
    assert!(!is_token_name("tok_abcdefghi1"));
}

#[tokio::test]
async fn created_tokens_get_distinct_opaque_names() {
    let db = setup().await;
    let first = token(&db, "ignored").await;
    let second = token(&db, "ignored").await;
    let names: Vec<String> = list_tokens(&db.pool)
        .await
        .unwrap()
        .into_iter()
        .filter(|t| t.id == first || t.id == second)
        .map(|t| t.name)
        .collect();
    assert_eq!(names.len(), 2);
    assert!(names.iter().all(|n| is_token_name(n)));
    assert_ne!(names[0], names[1]);
}

#[tokio::test]
async fn a_fleet_token_enrols_as_many_machines_as_it_is_allowed() {
    let db = setup().await;
    let (record, _) =
        create_token(&db.pool, TokenPolicy { max_uses: Some(2), expires_in_days: None })
            .await
            .expect("création");

    enrol(&db, &identity("a", Some("id-a")), record.id).await;
    enrol(&db, &identity("b", Some("id-b")), record.id).await;
    assert!(matches!(
        register(&db.pool, &db.cipher, &identity("c", Some("id-c")), record.id, None)
            .await
            .expect_err("la troisième dépasse le compte"),
        RegisterError::EnrolmentDenied(EnrolmentDenied::Exhausted)
    ));

    let listed = list_tokens(&db.pool).await.unwrap();
    let capped = listed.iter().find(|t| t.id == record.id).expect("jeton");
    assert_eq!(capped.uses, 2);
    assert_eq!(capped.max_uses, Some(2));

    // Sans limite, le parc entier passe.
    let open = token(&db, "sans limite").await;
    for n in 0..5 {
        enrol(&db, &identity(&format!("m{n}"), Some(&format!("id-m{n}"))), open).await;
    }
    assert!(enrolment_allowed(&db.pool, open).await.unwrap().is_ok());
}

#[tokio::test]
async fn a_token_past_its_deadline_stops_enrolling_but_not_reporting() {
    let db = setup().await;
    let (record, _) =
        create_token(&db.pool, TokenPolicy { max_uses: None, expires_in_days: Some(1) })
            .await
            .expect("création");
    let registration = enrol(&db, &identity("nas", Some("id-nas")), record.id).await;
    let secret = registration.issued_secret.clone().expect("secret");

    // Le terme passe.
    sqlx::query("UPDATE agent_tokens SET expires_at = '2000-01-01T00:00:00Z' WHERE id = ?")
        .bind(record.id)
        .execute(&db.pool)
        .await
        .unwrap();

    assert_eq!(
        enrolment_allowed(&db.pool, record.id).await.unwrap(),
        Err(EnrolmentDenied::Expired)
    );
    assert!(matches!(
        register(&db.pool, &db.cipher, &identity("autre", Some("id-2")), record.id, None)
            .await
            .expect_err("plus d'enrôlement après le terme"),
        RegisterError::EnrolmentDenied(EnrolmentDenied::Expired)
    ));
    // La machine déjà enrôlée n'est pas coupée pour autant.
    assert!(
        register(&db.pool, &db.cipher, &identity("nas", Some("id-nas")), record.id, Some(&secret))
            .await
            .is_ok()
    );
    // Le jeton reste « actif » au sens de l'ingestion : seule la révocation ferme tout.
    assert!(find_active_token(&db.pool, &token::fingerprint("x")).await.unwrap().is_none());
}

#[tokio::test]
async fn the_enrolment_count_starts_from_the_machines_already_there() {
    // Le scénario de la migration : des machines sont déjà enrôlées quand le
    // compteur apparaît. Lui poser une limite plus tard doit partir de là.
    let db = setup().await;
    let open = token(&db, "parc").await;
    enrol(&db, &identity("a", Some("id-a")), open).await;
    enrol(&db, &identity("b", Some("id-b")), open).await;

    let listed = list_tokens(&db.pool).await.unwrap();
    assert_eq!(listed.iter().find(|t| t.id == open).unwrap().uses, 2);
}

#[test]
fn an_ad_domain_is_normalised_or_refused() {
    assert_eq!(clean_ad_domain(" Corp.Example.COM. ").as_deref(), Some("corp.example.com"));
    assert_eq!(clean_ad_domain("ad-01.lan").as_deref(), Some("ad-01.lan"));
    for bad in ["", "  ", "corp..lan", "-corp.lan", "corp-.lan", "corp lan", "corp';exit;'.lan"] {
        assert_eq!(clean_ad_domain(bad), None, "{bad:?} doit être refusé");
    }
    assert_eq!(clean_ad_domain(&"a".repeat(64)), None, "étiquette de plus de 63 caractères");
}

#[test]
fn only_a_full_name_inside_the_domain_belongs_to_it() {
    let domain = "corp.example.com";
    assert!(hostname_in_domain("SRV01.corp.example.com", domain));
    assert!(hostname_in_domain("srv01.corp.example.com.", domain));
    assert!(hostname_in_domain("srv02.paris.corp.example.com", domain));
    assert!(!hostname_in_domain("srv01", domain), "un nom court ne dit pas son domaine");
    assert!(!hostname_in_domain("corp.example.com", domain));
    assert!(!hostname_in_domain(".corp.example.com", domain));
    assert!(!hostname_in_domain("srv01.evilcorp.example.com", domain));
    assert!(!hostname_in_domain("srv01.corp.example.com.evil", domain));
}

#[tokio::test]
async fn an_ad_fleet_token_enrols_only_its_domain_and_files_the_machines() {
    let db = setup().await;
    let id = token(&db, "AD corp").await;
    set_token_ad_domain(&db.pool, id, "corp.example.com").await.unwrap();
    assert_eq!(
        list_tokens(&db.pool).await.unwrap()[0].ad_domain.as_deref(),
        Some("corp.example.com")
    );

    // Hors du domaine, ou sous un nom court : refusé, et rien n'est compté.
    for outsider in ["srv01", "srv01.other.lan"] {
        assert!(matches!(
            register(&db.pool, &db.cipher, &identity(outsider, None), id, None)
                .await
                .expect_err("hors du domaine"),
            RegisterError::EnrolmentDenied(EnrolmentDenied::OutsideDomain)
        ));
    }
    assert_eq!(list_tokens(&db.pool).await.unwrap()[0].uses, 0);

    // Dans le domaine : inscrit, rangé dans le dossier du domaine, étiqueté.
    let mut member = identity("SRV01.corp.example.com", None);
    member.tags.insert("ad_ou".into(), "OU=Servers,DC=corp,DC=example,DC=com".into());
    let registration = enrol(&db, &member, id).await;
    assert!(registration.created);
    assert!(registration.issued_secret.is_some(), "chaque machine a son propre secret");
    let target = db::targets::get(&db.pool, &db.cipher, registration.target_id)
        .await
        .unwrap()
        .expect("cible");
    assert_eq!(target.group_name, "Active Directory corp.example.com");
    assert_eq!(target.tags.get("ad").map(String::as_str), Some("corp.example.com"));
    assert_eq!(
        target.tags.get("ad_ou").map(String::as_str),
        Some("OU=Servers,DC=corp,DC=example,DC=com")
    );
    assert_eq!(list_tokens(&db.pool).await.unwrap()[0].uses, 1);
}

#[tokio::test]
async fn closing_a_fleet_token_stops_enrolments_but_not_reports() {
    let db = setup().await;
    let id = token(&db, "flotte").await;
    let registration = enrol(&db, &identity("nas", Some("id-nas")), id).await;
    let secret = registration.issued_secret.expect("secret");

    assert!(close_enrolment(&db.pool, id).await.unwrap());
    assert_eq!(enrolment_allowed(&db.pool, id).await.unwrap(), Err(EnrolmentDenied::Expired));
    assert!(matches!(
        register(&db.pool, &db.cipher, &identity("autre", Some("id-2")), id, None)
            .await
            .expect_err("plus d'inscription"),
        RegisterError::EnrolmentDenied(EnrolmentDenied::Expired)
    ));
    // La machine inscrite continue, et le jeton reste valable pour elle.
    assert!(
        register(&db.pool, &db.cipher, &identity("nas", Some("id-nas")), id, Some(&secret))
            .await
            .is_ok()
    );
    let listed = list_tokens(&db.pool).await.unwrap();
    assert!(listed[0].revoked_at.is_none());

    // Un jeton révoqué ne se « clôt » plus : il est déjà fermé à tout.
    assert!(revoke_token(&db.pool, id).await.unwrap());
    assert!(!close_enrolment(&db.pool, id).await.unwrap());
}
