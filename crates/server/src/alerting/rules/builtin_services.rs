//! Règles livrées de CrowdSec, des mandataires inverses, des domaines, des
//! listes de blocage, des sites web, d'Active Directory, de Kubernetes, de
//! pfSense, d'Unraid, de Veeam, de WinRM, de Kopia et de Duplicati.

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // --- CrowdSec (`collectors/crowdsec`) ---
        Rule {
            description: "The CrowdSec Local API does not answer: no decision is taken or handed to the bouncers."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("crowdsec_lapi_down", "CrowdSec Local API down", RuleKind::Threshold, "dumbmonit_crowdsec_lapi_up")
        },
        // CrowdSec ne publie pas l'heure du dernier tirage : le compteur de
        // requêtes du bouncer qui ne bouge plus en tient lieu.
        //
        // Ces deux règles d'absence de progrès finissent par `and <compteur>` :
        // une variante que plus rien n'écrit (étiquettes renommées, tags
        // retouchés) reste dans la fenêtre sans plus bouger, et passerait pour
        // un bouncer arrêté ou une acquisition cassée. Seules les séries encore
        // alimentées sont jugées.
        Rule {
            description: "A CrowdSec bouncer has not called the Local API for 30 minutes: it enforces a frozen list."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("crowdsec_bouncer_stale", "CrowdSec bouncer stopped pulling", RuleKind::Threshold, "changes_prometheus(dumbmonit_crowdsec_bouncer_requests_total[30m]) and dumbmonit_crowdsec_bouncer_requests_total")
        },
        Rule {
            description: "CrowdSec has not read a single log line in six hours: its acquisition is broken and it sees no attack."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("crowdsec_no_logs_read", "CrowdSec reads no logs", RuleKind::Threshold, "increase_prometheus(dumbmonit_crowdsec_lines_read_total[6h]) and dumbmonit_crowdsec_lines_read_total")
        },
        // --- Proxys inverses (`collectors/{traefik,caddy,npm}`) ---
        Rule {
            description: "A Traefik router is disabled by a configuration error: its sites are not served."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("traefik_router_disabled", "Traefik router disabled", RuleKind::Threshold, "dumbmonit_traefik_router_status")
        },
        Rule {
            description: "A backend server of a Traefik service fails its health check."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("traefik_server_down", "Traefik backend server down", RuleKind::Threshold, "dumbmonit_traefik_server_up")
        },
        Rule {
            description: "No backend server of a Traefik service answers its health check: its sites get 503."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base("traefik_service_down", "Traefik service without a server", RuleKind::Threshold, "dumbmonit_traefik_service_servers_up")
        },
        // ACME obtient un certificat en quelques secondes : une heure sans,
        // c'est un résolveur qui échoue, et Traefik sert son certificat
        // auto-signé par défaut.
        Rule {
            description: "A Traefik router has waited an hour for a certificate from its resolver: Traefik serves its default self-signed certificate instead."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("traefik_certificate_missing", "Traefik certificate not obtained", RuleKind::Threshold, "dumbmonit_traefik_resolver_routers_uncovered")
        },
        Rule {
            description: "A Caddy upstream fails its active or passive health check."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("caddy_upstream_unhealthy", "Caddy upstream unhealthy", RuleKind::Threshold, "dumbmonit_caddy_upstream_healthy")
        },
        Rule {
            description: "Caddy refused its last configuration reload: it still runs the previous configuration."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("caddy_reload_failed", "Caddy configuration reload failed", RuleKind::Threshold, "dumbmonit_caddy_config_last_reload_ok")
        },
        Rule {
            description: "Nginx Proxy Manager could not load a host's configuration (nginx -t failed): the host is not served."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("npm_host_offline", "Nginx Proxy Manager host offline", RuleKind::Threshold, "dumbmonit_npm_host_online")
        },
        // Let's Encrypt et ACME renouvellent trente jours avant l'échéance : à
        // quatorze jours, le renouvellement a échoué au moins une semaine.
        Rule {
            description: "A certificate served by the reverse proxy expires in less than fourteen days: its renewal is failing."
                .to_string(),
            operator: Operator::Lt,
            threshold: 14.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "d".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("proxy_cert_expiring", "Proxy certificate not renewed", RuleKind::Threshold, "dumbmonit_traefik_cert_expiry_days or dumbmonit_npm_cert_expiry_days")
        },
        // Part de 5xx sur dix minutes, avec un plancher de cinquante requêtes :
        // une erreur sur deux requêtes nocturnes n'est pas un incident.
        Rule {
            description: "More than 5% of a reverse proxy's answers were 5xx errors over ten minutes, with at least fifty requests."
                .to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "proxy_5xx_rate",
                "Reverse proxy answering 5xx",
                RuleKind::Threshold,
                "(100 * increase_prometheus(dumbmonit_traefik_requests_5xx_total[10m]) \
                 / increase_prometheus(dumbmonit_traefik_requests_total[10m]) \
                 and increase_prometheus(dumbmonit_traefik_requests_total[10m]) >= 50) \
                 or (100 * increase_prometheus(dumbmonit_caddy_requests_5xx_total[10m]) \
                 / increase_prometheus(dumbmonit_caddy_requests_total[10m]) \
                 and increase_prometheus(dumbmonit_caddy_requests_total[10m]) >= 50)",
            )
        },
        // --- Noms de domaine (`collectors/domain`) ---
        // Deux règles disjointes (`>= 7` / `< 7`), comme pour les certificats.
        Rule {
            description: "The domain expires in less than thirty days.".to_string(),
            operator: Operator::Lt,
            threshold: 30.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "d".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("domain_expiring", "Domain expiring", RuleKind::Threshold, "dumbmonit_domain_expiry_days >= 7")
        },
        Rule {
            description: "The domain expires in less than seven days, or has expired.".to_string(),
            operator: Operator::Lt,
            threshold: 7.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Critical,
            unit: "d".to_string(),
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base("domain_expiring_soon", "Domain about to expire", RuleKind::Threshold, "dumbmonit_domain_expiry_days")
        },
        Rule {
            description: "The registry put the domain on hold (clientHold or serverHold): it is taken out of the DNS."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("domain_on_hold", "Domain on hold", RuleKind::Threshold, "dumbmonit_domain_on_hold")
        },
        Rule {
            description: "The domain is in its redemption period or pending deletion: it has expired and will be released."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base("domain_redemption", "Domain in redemption", RuleKind::Threshold, "dumbmonit_domain_redemption")
        },
        // Changements de délégation (`collectors/domain`) : les empreintes ne
        // bougent que si la liste change, et `changes_prometheus` ignore la
        // première valeur d'une série neuve. Un jour de fenêtre : l'alerte se
        // résout seule ensuite.
        Rule {
            description: "The name servers of the domain changed: expected after a migration, a sign of hijacking otherwise."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("domain_nameservers_changed", "Domain name servers changed", RuleKind::Threshold, "changes_prometheus(dumbmonit_domain_nameservers_fingerprint[1d])")
        },
        Rule {
            description: "The registrar of the domain changed: a transfer happened. Check that it was yours."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("domain_registrar_changed", "Domain registrar changed", RuleKind::Threshold, "changes_prometheus(dumbmonit_domain_registrar_fingerprint[1d])")
        },
        Rule {
            description: "DNSSEC was active on the domain in the last seven days and is not any more: validating resolvers may refuse the domain, or the signature was stripped."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("domain_dnssec_lost", "Domain DNSSEC lost", RuleKind::Threshold, "(dumbmonit_domain_dnssec == 0) and (max_over_time(dumbmonit_domain_dnssec[7d]) == 1)")
        },
        // --- Listes noires DNS (`collectors/blacklist`) ---
        // Un passage n'a lieu que toutes les six heures et son résultat est
        // réécrit à chaque sonde : la durée minimale ne sert qu'à laisser passer
        // une sonde isolée.
        Rule {
            description: "An address or the domain is listed on a major blocklist (Spamhaus, Barracuda or SpamCop): a large share of its mail is refused. The monitor page links to the delisting page."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("blacklist_listed_major", "Listed on a major blocklist", RuleKind::Threshold, "dumbmonit_blacklist_listed_major_lists")
        },
        Rule {
            description: "An address or the domain is listed on a smaller blocklist (Mailspike, UCEPROTECT, PSBL, SpamRATS, SURBL or URIBL): some receivers refuse its mail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("blacklist_listed_minor", "Listed on a blocklist", RuleKind::Threshold, "dumbmonit_blacklist_listed_lists - dumbmonit_blacklist_listed_major_lists")
        },
        Rule {
            description: "The reverse DNS of a mail server address is missing, or names a host that does not resolve back to it: Gmail, Outlook.com and Yahoo reject or junk such mail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("blacklist_reverse_dns", "Mail server reverse DNS mismatch", RuleKind::Threshold, "dumbmonit_blacklist_ptr_ok == bool 0")
        },
        Rule {
            description: "The mail domain has no SPF record (or several), or no DMARC record: Gmail and Yahoo require both from bulk senders."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("blacklist_mail_auth_missing", "SPF or DMARC missing", RuleKind::Threshold, "dumbmonit_blacklist_mail_auth_missing")
        },
        // Réponse DNS modifiée (`uptime/dns`, option `alert_on_change`) :
        // l'empreinte ne change que si l'ensemble des valeurs change, et
        // `changes_prometheus` ne compte pas la première valeur d'une série
        // neuve. L'alerte se résout d'elle-même une heure plus tard.
        Rule {
            description: "The answer of a DNS check changed: a record was added, removed or replaced."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("dns_answer_changed", "DNS answer changed", RuleKind::Threshold, "changes_prometheus(dumbmonit_probe_dns_answer_fingerprint[1h])")
        },
        // Contenu d'un site modifié (`webchange`) : la jauge vaut le nombre de
        // changements (page modifiée, apparue ou disparue) trouvés par la
        // dernière vérification. Une seule série, donc une seule alerte, par
        // surveillance : tant qu'elle reste active, le moteur annonce chaque
        // nouvelle vérification qui trouve des changements (voir
        // `cycle::Occurrence`), en citant les pages lues en base. Elle se
        // résout à la première vérification sans changement. Pas de rappel :
        // le changement est un fait, pas une panne en cours.
        Rule {
            description: "A check of a watched website found pages that changed, appeared or disappeared. Every such check is notified, naming the pages, until a check finds nothing new; the device page shows each change before and after."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Info,
            repeat_interval: None,
            ..base("webchange_detected", "Website changed", RuleKind::Threshold, "dumbmonit_webchange_last_check_changes")
        },
        // --- Sauvegarde des configurations réseau (`netconfig`) ---
        //
        // La jauge vaut 1 quand le dernier relevé réussi a produit une nouvelle
        // version (jamais pour la toute première). Elle retombe au relevé
        // suivant sans changement. Pas de rappel : un changement est un fait.
        Rule {
            description: "The configuration of a backed-up network device changed since the previous version. The device page shows the line diff, secrets masked."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Info,
            repeat_interval: None,
            ..base("netconfig_changed", "Network configuration changed", RuleKind::Threshold, "dumbmonit_netconfig_config_changed")
        },
        // Clé d'hôte SSH différente de celle mémorisée : la connexion est
        // refusée avant toute authentification, plus aucune sauvegarde n'a lieu
        // tant qu'un administrateur n'a pas accepté la nouvelle clé. Équipement
        // remplacé ou réinstallé… ou interception : à vérifier, d'où le niveau.
        Rule {
            description: "A backed-up device presented an SSH host key other than the remembered one. Backups are stopped and no credential was sent; check the device, then accept the new key on its page."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            // La jauge reste à 1 tant que la clé n'est pas acceptée : une minute
            // de retenue écarte un relevé isolé sans retarder l'alerte.
            for_duration: Duration::from_secs(60),
            severity: Severity::Critical,
            ..base("netconfig_host_key_changed", "SSH host key changed", RuleKind::Threshold, "dumbmonit_netconfig_host_key_changed")
        },
        // --- Santé et référencement d'un site (`website`) ---
        //
        // Un site injoignable (réseau, délai, certificat refusé, boucle de
        // redirections) relève de « Monitor unreachable » : la sonde échoue.
        // `website_up` couvre le site qui répond, mais par une erreur.
        Rule {
            description: "The website answers with an error (4xx or 5xx) after following its redirects.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            ..base("website_down", "Website down", RuleKind::Threshold, "dumbmonit_website_up")
        },
        Rule {
            description: "The website answers, but its page shows almost no text: a broken theme, a failed database or an empty template.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            ..base("website_blank", "Website page blank", RuleKind::Threshold, "dumbmonit_website_blank")
        },
        Rule {
            description: "The text the page must show (option \"Expected text\") is missing.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Warning,
            ..base("website_expected_text_missing", "Website expected text missing", RuleKind::Threshold, "dumbmonit_website_expect_text_missing")
        },
        // Les deux règles suivantes sortent le site des moteurs de recherche :
        // critiques, mais lentes à prouver un préjudice, d'où un rappel par jour.
        Rule {
            description: "The page asks search engines not to index it (noindex in a robots meta tag or an X-Robots-Tag header).".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("website_noindex", "Website not indexable", RuleKind::Threshold, "dumbmonit_website_noindex")
        },
        Rule {
            description: "The site's robots.txt forbids search engines to crawl anything.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base("website_robots_blocks_all", "Website blocked by robots.txt", RuleKind::Threshold, "dumbmonit_website_robots_blocks_all")
        },
        // Changements confirmés (vus deux sondes de suite) : la jauge vaut 1
        // pendant 24 heures. Pas de rappel : c'est un fait, pas une panne.
        Rule {
            description: "The canonical URL of the page changed, confirmed on two checks in a row. Search engines may move the page's ranking to the new address.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Warning,
            repeat_interval: None,
            ..base("website_canonical_changed", "Website canonical URL changed", RuleKind::Threshold, "dumbmonit_website_canonical_changed")
        },
        Rule {
            description: "After its redirects, the website now lands on another host or path, confirmed on two checks in a row (a move, a parked domain, a hijack). Going from http to https or to www. is not a change.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Warning,
            repeat_interval: None,
            ..base("website_final_url_changed", "Website address changed", RuleKind::Threshold, "dumbmonit_website_final_url_changed")
        },
        Rule {
            description: "The title or the meta description that search engines show changed, confirmed on two checks in a row.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Info,
            repeat_interval: None,
            ..base(
                "website_metadata_changed",
                "Website title or description changed",
                RuleKind::Threshold,
                "dumbmonit_website_title_changed + dumbmonit_website_description_changed",
            )
        },
        Rule {
            description: "The visible text of the page fell far below its usual size (option \"Near-empty page: text drop\"), confirmed on several checks in a row: a page that is no longer blank, but almost.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Warning,
            ..base("website_text_dropped", "Website page nearly empty", RuleKind::Threshold, "dumbmonit_website_text_dropped")
        },
        Rule {
            description: "The Google site verification tag (google-site-verification) changed or disappeared, confirmed on several checks in a row. Losing it can cut access to Search Console.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Warning,
            repeat_interval: None,
            ..base("website_google_verification_changed", "Website Google verification changed", RuleKind::Threshold, "dumbmonit_website_google_verification_changed")
        },
        Rule {
            description: "The robots directives, Open Graph tags, page language, h1 count or hreflang alternates of the page changed, confirmed on several checks in a row.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30),
            severity: Severity::Info,
            repeat_interval: None,
            ..base(
                "website_seo_markup_changed",
                "Website SEO markup changed",
                RuleKind::Threshold,
                "dumbmonit_website_robots_changed + dumbmonit_website_open_graph_changed + dumbmonit_website_lang_changed + dumbmonit_website_h1_changed + dumbmonit_website_hreflang_changed",
            )
        },
        Rule {
            description: "The website takes more than five seconds to answer, redirects included.".to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base("website_slow", "Website slow", RuleKind::Threshold, "dumbmonit_website_response_seconds")
        },
        // --- Active Directory (`collectors/activedirectory`) ---
        //
        // Le contrôleur interrogé lui-même relève de « Monitor unreachable » ;
        // `ad_dc_reachable` couvre les autres contrôleurs du domaine. Un nom
        // qui ne se résout pas depuis DumbMonit ne produit pas de série.
        Rule {
            description: "A domain controller of the domain no longer answers on its LDAP port.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "ad_dc_unreachable",
                "Domain controller unreachable",
                RuleKind::Threshold,
                "dumbmonit_ad_dc_reachable == bool 0",
            )
        },
        // Le contrôleur répond mais refuse le compte de service : mot de
        // passe expiré, compte désactivé ou verrouillé. Plus rien n'est lu.
        Rule {
            description: "The domain controller refuses the service account (expired password, \
                          disabled or locked out account): nothing is read any more."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "ad_bind_failed",
                "Active Directory bind failed",
                RuleKind::Threshold,
                "dumbmonit_ad_bind_ok == bool 0",
            )
        },
        // L'empreinte change dès qu'un membre entre ou sort, même si le
        // nombre reste le même ; `changes_prometheus` ignore la première valeur
        // d'une série neuve. L'alerte se résout d'elle-même une heure plus tard.
        Rule {
            description: "The members of a privileged group (Domain Admins, Enterprise Admins, \
                          Schema Admins, Administrators, operators) changed."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "ad_privileged_group_changed",
                "Privileged group membership changed",
                RuleKind::Threshold,
                "changes_prometheus(dumbmonit_ad_privileged_group_fingerprint[1h])",
            )
        },
        Rule {
            description: "The domain controller cannot replicate from one of its partners: \
                          changes made elsewhere do not reach it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "ad_replication_failing",
                "Active Directory replication failing",
                RuleKind::Threshold,
                "dumbmonit_ad_replication_consecutive_failures",
            )
        },
        Rule {
            description: "The krbtgt password is older than 180 days: a stolen krbtgt hash keeps \
                          forging valid tickets until it is changed twice."
                .to_string(),
            operator: Operator::Gt,
            threshold: 180.0 * 86_400.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Info,
            unit: "s".to_string(),
            repeat_interval: None,
            ..base(
                "ad_krbtgt_password_old",
                "krbtgt password older than 180 days",
                RuleKind::Threshold,
                "dumbmonit_ad_krbtgt_password_age_seconds",
            )
        },
        // --- Kubernetes (`collectors/kubernetes`) ---
        //
        // Toutes ces séries n'existent que pour une cible Kubernetes : aucune ne
        // peut se déclencher ailleurs.
        Rule {
            description: "A Kubernetes node is not Ready: its kubelet no longer reports, and its pods \
                          will be evicted."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_node_not_ready",
                "Kubernetes node not ready",
                RuleKind::Threshold,
                "dumbmonit_k8s_node_ready == bool 0",
            )
        },
        Rule {
            description: "A Kubernetes node reports memory, disk or PID pressure: the kubelet starts \
                          evicting pods and refuses new ones."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_node_pressure",
                "Kubernetes node under pressure",
                RuleKind::Threshold,
                "dumbmonit_k8s_node_pressure",
            )
        },
        // Entre deux tentatives, le conteneur redémarre et l'état
        // `CrashLoopBackOff` disparaît quelques secondes : le maximum sur dix
        // minutes lisse ce va-et-vient, sans quoi le `for` repartirait de zéro.
        //
        // Une boucle redémarre au moins toutes les cinq minutes (plafond du
        // back-off de Kubernetes) : on exige deux redémarrages en quinze minutes.
        // Le redémarrage d'un nœud n'en ajoute qu'un par conteneur, et le compteur
        // à vie d'un pod système ne fait plus passer ce redémarrage pour une boucle.
        Rule {
            description: "A pod keeps crashing and Kubernetes waits longer and longer before \
                          restarting it (CrashLoopBackOff)."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_pod_crashlooping",
                "Kubernetes pod crash looping",
                RuleKind::Threshold,
                K8S_CRASHLOOP_QUERY,
            )
        },
        Rule {
            description: "A pod has been Pending for fifteen minutes: no node can take it, or a \
                          volume or an image it needs does not come."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_pod_pending",
                "Kubernetes pod stuck pending",
                RuleKind::Threshold,
                "dumbmonit_k8s_pod_pending",
            )
        },
        // Un pod déjà signalé en boucle de redémarrage n'est pas compté deux fois.
        Rule {
            description: "A pod restarted more than five times in the last hour without being in \
                          CrashLoopBackOff: it crashes, then recovers, then crashes again."
                .to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_pod_restarting",
                "Kubernetes pod restarting",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_k8s_pod_restarts[1h]) \
                 unless on (target, namespace, pod) \
                 max_over_time(dumbmonit_k8s_pod_crashlooping[10m]) > 0",
            )
        },
        // Une mise à jour progressive retire un réplica le temps d'en démarrer
        // un autre : dix minutes de patience.
        Rule {
            description: "A Deployment, StatefulSet or DaemonSet has fewer available replicas than \
                          it asks for."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            escalate_after: Some(Duration::from_secs(3600)),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_workload_unavailable",
                "Kubernetes workload missing replicas",
                RuleKind::Threshold,
                "dumbmonit_k8s_workload_unavailable",
            )
        },
        Rule {
            description: "A PersistentVolumeClaim that a pod needs, or that a provisioning error \
                          concerns, is still not bound after fifteen minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "k8s_pvc_pending",
                "Kubernetes volume claim pending",
                RuleKind::Threshold,
                "dumbmonit_k8s_pvc_pending",
            )
        },
        // --- pfSense (`collectors/pfsense`) ---
        Rule {
            description: "A pfSense gateway is down: its monitor address no longer answers, and the traffic it carries stops or fails over.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pfsense_gateway_down",
                "pfSense gateway down",
                RuleKind::Threshold,
                "dumbmonit_pfsense_gateway_up",
            )
        },
        // Une passerelle tombée perd tout : elle est déjà signalée par la règle précédente.
        Rule {
            description: "A pfSense gateway loses more than 10 % of the probes to its monitor address: the line is degraded.".to_string(),
            operator: Operator::Gt,
            threshold: 10.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pfsense_gateway_loss",
                "pfSense gateway losing packets",
                RuleKind::Threshold,
                "dumbmonit_pfsense_gateway_loss_percent unless on (target, gateway) dumbmonit_pfsense_gateway_up == 0",
            )
        },
        Rule {
            description: "The round trip to a pfSense gateway's monitor address has stayed above 500 ms for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 500.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "ms".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pfsense_gateway_latency",
                "pfSense gateway slow",
                RuleKind::Threshold,
                "dumbmonit_pfsense_gateway_delay_milliseconds",
            )
        },
        Rule {
            description: "An enabled pfSense interface has no link: cable, switch port or network card.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pfsense_interface_down",
                "pfSense interface without link",
                RuleKind::Threshold,
                "dumbmonit_pfsense_interface_down",
            )
        },
        Rule {
            description: "A pfSense service that is enabled is not running: DNS resolver, VPN, NTP…".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "pfsense_service_stopped",
                "pfSense service stopped",
                RuleKind::Threshold,
                "dumbmonit_pfsense_service_stopped",
            )
        },
        Rule {
            description: "The pfSense disk is more than 90 % full: logs, reports and package updates stop being written.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "pfsense_disk_full",
                "pfSense disk nearly full",
                RuleKind::Threshold,
                "dumbmonit_pfsense_disk_used_percent",
            )
        },
        // --- Unraid (`collectors/unraid`) ---
        Rule {
            description: "The Unraid array is not started: shares, containers and VMs that live on it are unavailable.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "unraid_array_stopped",
                "Unraid array stopped",
                RuleKind::Threshold,
                "dumbmonit_unraid_array_started",
            )
        },
        Rule {
            description: "An Unraid disk is disabled, missing or invalid: the array runs degraded, and one more failure loses data.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "unraid_disk_problem",
                "Unraid disk disabled or missing",
                RuleKind::Threshold,
                "dumbmonit_unraid_disk_ok",
            )
        },
        Rule {
            description: "An Unraid disk has reported read errors since the array started: it may be disabled at the next one.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "unraid_disk_errors",
                "Unraid disk read errors",
                RuleKind::Threshold,
                "dumbmonit_unraid_disk_errors",
            )
        },
        // Les disques de cache (NVMe) chauffent plus, et légitimement : ils ne sont pas visés.
        Rule {
            description: "An Unraid array or parity disk has stayed above 55 °C for fifteen minutes.".to_string(),
            operator: Operator::Gt,
            threshold: 55.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "unraid_disk_hot",
                "Unraid disk too hot",
                RuleKind::Threshold,
                "dumbmonit_unraid_disk_temperature_celsius{role=~\"data|parity\"}",
            )
        },
        Rule {
            description: "The last Unraid parity check failed or found sync errors: parity no longer matches the data.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "unraid_parity_errors",
                "Unraid parity check found errors",
                RuleKind::Threshold,
                "dumbmonit_unraid_parity_check_ok",
            )
        },
        Rule {
            description: "The last Unraid parity check is more than 40 days old: a monthly check was missed.".to_string(),
            operator: Operator::Gt,
            threshold: 40.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "unraid_parity_check_old",
                "Unraid parity check overdue",
                RuleKind::Threshold,
                "dumbmonit_unraid_parity_check_age_seconds",
            )
        },
        Rule {
            description: "An Unraid cache pool is more than 90 % full: writes fall back to the slow array, or fail.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "unraid_cache_full",
                "Unraid cache nearly full",
                RuleKind::Threshold,
                "dumbmonit_unraid_disk_used_percent{role=\"cache\"}",
            )
        },
        Rule {
            description: "A Docker container set to start with the array is not running.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "unraid_container_stopped",
                "Unraid container stopped",
                RuleKind::Threshold,
                "dumbmonit_unraid_container_autostart_stopped",
            )
        },
        // --- Veeam Backup & Replication (`collectors/veeam`) ---
        Rule {
            description: "The last run of an enabled Veeam job failed: what it protects has no new restore point.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_job_failed",
                "Veeam job failed",
                RuleKind::Threshold,
                "dumbmonit_veeam_job_failed",
            )
        },
        Rule {
            description: "The last run of an enabled Veeam job ended with a warning: some objects may not be protected.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_job_warning",
                "Veeam job ended with a warning",
                RuleKind::Threshold,
                "dumbmonit_veeam_job_warning",
            )
        },
        // Aucune sauvegarde réussie depuis deux jours. La série n'existe que pour un
        // travail dont la dernière exécution a réussi (ou fini en avertissement) :
        // un travail en échec dit déjà `veeam_job_failed`. Le filtre sur la
        // prochaine exécution (moins de 36 h) écarte les travaux hebdomadaires ou
        // mensuels, dont deux jours sans exécution sont normaux.
        Rule {
            description: "No successful Veeam run for this job for more than two days, \
                          although it is scheduled at least daily."
                .to_string(),
            operator: Operator::Gt,
            threshold: 2.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_job_old",
                "Veeam backup too old",
                RuleKind::Threshold,
                "dumbmonit_veeam_job_last_success_age_seconds and on (target, job) dumbmonit_veeam_job_next_run_seconds < 129600",
            )
        },
        Rule {
            description: "A Veeam backup repository is more than 90 % full: the next jobs will fail for lack of space.".to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_repository_full",
                "Veeam repository nearly full",
                RuleKind::Threshold,
                "dumbmonit_veeam_repository_used_percent",
            )
        },
        Rule {
            description: "The Veeam license expires in less than 30 days: jobs stop at the end of the grace period.".to_string(),
            operator: Operator::Lt,
            threshold: 30.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_license_expiring",
                "Veeam license expiring",
                RuleKind::Threshold,
                "dumbmonit_veeam_license_expiry_seconds",
            )
        },
        Rule {
            description: "The Veeam support contract expires in less than 30 days: updates and Veeam assistance stop at expiry, jobs keep running.".to_string(),
            operator: Operator::Lt,
            threshold: 30.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "veeam_support_expiring",
                "Veeam support expiring",
                RuleKind::Threshold,
                "dumbmonit_veeam_support_expiry_seconds",
            )
        },
        // --- Windows sans agent (`collectors/winrm`) ---
        Rule {
            description: "One or more Windows services set to start automatically are stopped (reads WMI over WinRM, no agent).".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "winrm_services_auto_stopped",
                "Windows automatic services stopped",
                RuleKind::Threshold,
                "dumbmonit_windows_services_auto_stopped",
            )
        },
        Rule {
            description: "The last snapshot of a Kopia source finished with errors: some files were not saved.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "kopia_snapshot_errors",
                "Kopia snapshot with errors",
                RuleKind::Threshold,
                "dumbmonit_kopia_source_last_snapshot_errors",
            )
        },
        // Seules les sources planifiées : une source manuelle n'a pas à être récente.
        Rule {
            description: "A scheduled Kopia source has no snapshot for more than two days: its schedule is not running or its snapshots fail.".to_string(),
            operator: Operator::Gt,
            threshold: 2.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "kopia_snapshot_old",
                "Kopia snapshot too old",
                RuleKind::Threshold,
                "dumbmonit_kopia_source_last_snapshot_age_seconds and on (target, source) dumbmonit_kopia_source_scheduled > 0",
            )
        },
        Rule {
            description: "The Kopia server is not connected to its repository: no snapshot can run.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "kopia_repo_disconnected",
                "Kopia repository disconnected",
                RuleKind::Threshold,
                "dumbmonit_kopia_repo_connected",
            )
        },
        // --- Duplicati (`collectors/duplicati`) ---
        Rule {
            description: "The last run of a Duplicati backup failed: what it protects has no new version.".to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "duplicati_backup_failed",
                "Duplicati backup failed",
                RuleKind::Threshold,
                "dumbmonit_duplicati_backup_failed",
            )
        },
        Rule {
            description: "A scheduled Duplicati backup has not succeeded for more than two days.".to_string(),
            operator: Operator::Gt,
            threshold: 2.0 * 24.0 * 3600.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "duplicati_backup_old",
                "Duplicati backup too old",
                RuleKind::Threshold,
                "dumbmonit_duplicati_backup_last_success_age_seconds and on (target, backup_id) dumbmonit_duplicati_backup_scheduled > 0",
            )
        },
    ]
}
