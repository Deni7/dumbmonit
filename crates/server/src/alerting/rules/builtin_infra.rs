//! Règles livrées des serveurs de messagerie (MDaemon, SecurityGateway),
//! d'OPNsense, de TrueNAS et des serveurs d'observabilité (VictoriaMetrics,
//! Loki, Graylog).

use super::*;

/// Les règles livrées de cette tranche, dans l'ordre du catalogue.
///
/// Le tableau est mis en page à la main, comme lorsqu'il tenait dans un seul
/// fichier : rustfmt n'y touche pas.
#[rustfmt::skip]
pub(super) fn rules() -> Vec<Rule> {
    vec![
        // --- MDaemon Email Server et SecurityGateway (`collectors/mdaemon`) ---
        //
        // Ce qui compte d'abord pour un serveur de messagerie : que ses ports
        // acceptent le courrier. `service_up` vaut 0 quand le port ne répond pas
        // ou que la bannière refuse le service (`421`, `-ERR`, `* BYE`) ; seuls
        // les services listés sur l'équipement ont une série. Le `for` absorbe
        // un redémarrage du service Windows.
        Rule {
            description: "A mail service of the MDaemon server does not answer, or refuses \
                          service."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mdaemon_service_down",
                "MDaemon mail service down",
                RuleKind::Threshold,
                "dumbmonit_mdaemon_service_up",
            )
        },
        // L'API XML ne répond plus alors que la collecte, elle, réussit : les
        // ports de messagerie répondent, c'est l'administration à distance qui
        // est arrêtée. La série n'existe que si un compte est configuré.
        Rule {
            description: "The MDaemon XML API has stopped answering while the mail services \
                          still do: Remote Administration is probably stopped."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "mdaemon_api_down",
                "MDaemon XML API not answering",
                RuleKind::Threshold,
                "dumbmonit_mdaemon_api_up",
            )
        },
        // Files d'attente de MDaemon : elles ne sont publiées que comme compteurs
        // de performance Windows, que l'agent lit sur le serveur lui-même
        // (`crates/agent/src/collect/perf_counters.rs`). Les séries portent donc
        // la cible de l'agent, pas celle de l'équipement `mdaemon`, et
        // n'existent que sur une machine où tourne MDaemon.
        //
        // Croissance et non niveau, comme pour PMG : un serveur chargé garde en
        // permanence quelques messages en transit. Les files entrante, locale et
        // distante sont celles que MDaemon vide d'elle-même ; une file qui
        // grossit de cent messages en deux heures ne se vide plus.
        Rule {
            description: "An MDaemon mail queue has grown by more than a hundred messages in two \
                          hours: MDaemon has stopped delivering from it. Read from the Windows \
                          performance counters by the agent installed on the mail server."
                .to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mdaemon_queue_growing",
                "MDaemon mail queue growing",
                RuleKind::Threshold,
                "delta(dumbmonit_mdaemon_queue_messages{queue=~\"inbound|local|remote\"}[2h])",
            )
        },
        // La file « Bad » reçoit ce que MDaemon n'a pas su traiter ni renvoyer :
        // rien n'en sort sans un administrateur. Un seul message suffit à le
        // prévenir ; l'attente absorbe un message que quelqu'un traite déjà.
        Rule {
            description: "Messages are waiting in the MDaemon Bad queue: MDaemon could neither \
                          deliver nor bounce them, and nothing leaves that queue unless an \
                          administrator releases or deletes it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "mdaemon_bad_queue_not_empty",
                "MDaemon Bad queue not empty",
                RuleKind::Threshold,
                "dumbmonit_mdaemon_queue_messages{queue=\"bad\"}",
            )
        },
        // La file de nouvelle tentative garde ce que les serveurs distants ont
        // refusé temporairement. Quelques messages y passent chaque jour ; plus
        // de cinquante pendant une heure, c'est un voisin qui refuse tout, ou
        // une réputation d'expéditeur à vérifier.
        Rule {
            description: "More than fifty messages have been waiting in the MDaemon Retry queue \
                          for an hour: remote servers keep refusing mail temporarily."
                .to_string(),
            operator: Operator::Gt,
            threshold: 50.0,
            for_duration: Duration::from_secs(60 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mdaemon_retry_queue_high",
                "MDaemon Retry queue high",
                RuleKind::Threshold,
                "dumbmonit_mdaemon_queue_messages{queue=\"retry\"}",
            )
        },
        // Services Windows et dossiers de files, lus par l'agent installé sur le
        // serveur de messagerie (`crates/agent/src/collect/mail_server.rs`) : les
        // séries portent la cible de l'agent. Un service à démarrage manuel ou
        // désactivé n'alerte pas : seul `start="auto"` dit qu'il doit tourner.
        Rule {
            description: "A Windows service of MDaemon or SecurityGateway that starts \
                          automatically is not running. Read by the agent installed on the \
                          mail server."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mail_server_service_stopped",
                "Mail server Windows service stopped",
                RuleKind::Threshold,
                "dumbmonit_agent_mail_service_up{start=\"auto\"}",
            )
        },
        // L'âge du plus ancien message d'une file qui doit se vider : au-delà de
        // deux heures, rien n'en sort (file gelée, remise bloquée). Le nombre de
        // messages ne le dirait pas : une file peut garder le même effectif
        // avec toujours les mêmes messages.
        Rule {
            description: "The oldest message of an MDaemon inbound, local or remote queue has \
                          been waiting for more than two hours: the queue is frozen or \
                          delivery is blocked. Counted from the queue folders by the agent on \
                          the mail server."
                .to_string(),
            operator: Operator::Gt,
            threshold: 7200.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mail_queue_stuck",
                "Mail queue stuck",
                RuleKind::Threshold,
                "dumbmonit_agent_mail_queue_oldest_seconds{queue=~\"inbound|local|remote\"}",
            )
        },
        // MDaemon lève lui-même l'indicateur « gelée » d'une file (compteur de
        // performance), ce qui ne dépend pas de l'âge des messages.
        Rule {
            description: "MDaemon reports one of its queues as frozen: it no longer processes \
                          it. Read from the Windows performance counters by the agent on the \
                          mail server."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "mdaemon_queue_frozen",
                "MDaemon queue frozen",
                RuleKind::Threshold,
                "dumbmonit_mdaemon_queue_frozen",
            )
        },
        Rule {
            description: "A service of the SecurityGateway mail gateway does not answer, or \
                          refuses service."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "securitygateway_service_down",
                "SecurityGateway service down",
                RuleKind::Threshold,
                "dumbmonit_securitygateway_service_up",
            )
        },
        Rule {
            description: "The SecurityGateway REST API has stopped answering while the mail \
                          services still do."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "securitygateway_api_down",
                "SecurityGateway API not answering",
                RuleKind::Threshold,
                "dumbmonit_securitygateway_api_up",
            )
        },
        // La file de remise est le premier symptôme d'un serveur aval qui refuse
        // le courrier. Les noms des compteurs viennent tels quels de l'API : la
        // règle vise ceux qui parlent de la file de remise, sous les deux formes
        // que prend ce nom dans l'aide (« Delivery Queue », « Queued for
        // Delivery »). Croissance et non niveau, comme pour PMG.
        Rule {
            description: "The SecurityGateway delivery queue has grown by more than a hundred \
                          messages in two hours: the mail server behind it is probably \
                          refusing mail."
                .to_string(),
            operator: Operator::Gt,
            threshold: 100.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "securitygateway_queue_growing",
                "SecurityGateway delivery queue growing",
                RuleKind::Threshold,
                "delta(dumbmonit_securitygateway_counter{counter=~\".*(delivery_queue|queued_for_delivery).*\"}[2h])",
            )
        },
        // --- fin du bloc MDaemon ---
        // --- OPNsense : passerelles, pare-feu, tunnels (`collectors/opnsense`) ---
        //
        // Un pare-feu tombe rarement d'un bloc : il perd un lien, sature sa table
        // d'états ou cesse de résoudre, et tout le monde continue à le croire en
        // bonne santé parce qu'il répond toujours au ping. Les règles qui suivent
        // visent ces pannes-là.
        //
        // La bascule multi-WAN est la première : le lien de secours a pris le
        // relais il y a trois semaines, la facture mobile explose et personne ne
        // le sait. `gateway_up` vaut 0 sur « down » et « force_down » seulement :
        // « loss » et « delay » sont des avertissements de dpinger, pas des pannes.
        Rule {
            description: "A gateway is down: dpinger no longer gets an answer from it. On a \
                          multi-WAN firewall the traffic has already moved to another link, \
                          quietly."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_gateway_down",
                "Internet gateway down",
                RuleKind::Threshold,
                "dumbmonit_opnsense_gateway_up",
            )
        },
        // Perte de paquets : une passerelle qui répond encore mais perd un
        // cinquième de ce qu'on lui envoie rend les appels inaudibles et les
        // sessions instables, sans jamais être déclarée tombée.
        Rule {
            description: "A gateway is losing more than a fifth of the packets sent to it: the \
                          link answers, badly."
                .to_string(),
            operator: Operator::Gt,
            threshold: 20.0,
            clear_threshold: Some(10.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_gateway_loss",
                "Gateway losing packets",
                RuleKind::Threshold,
                "dumbmonit_opnsense_gateway_loss_percent",
            )
        },
        // Latence : trois cents millisecondes tiennent compte d'un lien mobile de
        // secours, qui n'a aucune raison de déclencher une alerte pour sa nature.
        Rule {
            description: "A gateway's round-trip time has stayed above 300 ms for a quarter of \
                          an hour."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.3,
            clear_threshold: Some(0.2),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Info,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "opnsense_gateway_latency",
                "Gateway slow",
                RuleKind::Threshold,
                "dumbmonit_opnsense_gateway_delay_seconds",
            )
        },
        // Table d'états : elle se remplit longtemps avant de déborder, et quand
        // elle déborde le pare-feu refuse des connexions neuves sans qu'aucune
        // autre série ne bouge.
        Rule {
            description: "The firewall's state table is more than 80% full. Past the limit it \
                          drops new connections without any other symptom."
                .to_string(),
            operator: Operator::Gt,
            threshold: 80.0,
            clear_threshold: Some(70.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_state_table_filling",
                "Firewall state table filling up",
                RuleKind::Threshold,
                "dumbmonit_opnsense_pf_states_used_percent",
            )
        },
        // Tampons réseau de FreeBSD : les épuiser arrête le routage alors que le
        // processeur et la mémoire restent au repos. C'est la panne qui ne
        // ressemble à rien, et elle a un seuil.
        Rule {
            description: "The firewall has used more than 90% of its network buffers. FreeBSD \
                          stops forwarding when they run out, with the CPU still idle."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_mbuf_exhausted",
                "Firewall network buffers exhausted",
                RuleKind::Threshold,
                "dumbmonit_opnsense_mbuf_used_percent",
            )
        },
        // Tunnel VPN : la série n'existe que pour un tunnel configuré et activé.
        // Un greffon absent ou une connexion désactivée n'en produisent aucune,
        // et ne peuvent donc pas déclencher.
        Rule {
            description: "A configured VPN tunnel has had no session for ten minutes.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_vpn_tunnel_down",
                "VPN tunnel down",
                RuleKind::Threshold,
                "dumbmonit_opnsense_vpn_tunnel_up",
            )
        },
        // Le résolveur : un pare-feu qui route mais ne résout plus paraît en
        // bonne santé à tout le monde sauf aux machines derrière lui. La série
        // n'existe que si Unbound est installé ; un pare-feu qui résout avec
        // autre chose n'a qu'à désactiver cette règle.
        Rule {
            description: "Unbound has stopped answering. The firewall still routes; the machines \
                          behind it no longer resolve."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_resolver_down",
                "Firewall resolver stopped",
                RuleKind::Threshold,
                "dumbmonit_opnsense_unbound_running",
            )
        },
        // Services du cœur : `dpinger` est ce qui surveille les passerelles et
        // `configd` ce qui exécute tout le reste. Les deux tournent sur toute
        // installation ; les autres services sont volontairement hors de la
        // règle, parce qu'un service arrêté exprès n'est pas une panne.
        Rule {
            description: "A core service of the firewall is stopped: dpinger, which watches the \
                          gateways, or configd, which runs everything else."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_core_service_down",
                "Firewall core service stopped",
                RuleKind::Threshold,
                "dumbmonit_opnsense_service_running{service=~\"dpinger|configd\"}",
            )
        },
        // Température : un boîtier de pare-feu vit dans un placard, souvent sans
        // ventilateur. Quatre-vingt-cinq degrés est la limite au-delà de laquelle
        // les processeurs embarqués commencent à se brider.
        Rule {
            description: "A sensor of the firewall is above 85 °C: the fanless box in the \
                          cupboard is about to start throttling."
                .to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            clear_threshold: Some(78.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "opnsense_temperature_high",
                "Firewall too hot",
                RuleKind::Threshold,
                "dumbmonit_opnsense_temperature_celsius",
            )
        },
        // CARP en mode maintenance : la bascule a été demandée à la main pour une
        // mise à jour, et jamais annulée. Le pare-feu reste secondaire pour
        // toujours, ce qui ne se voit nulle part ailleurs.
        Rule {
            description: "CARP has been left in persistent maintenance mode: this firewall has \
                          handed its virtual addresses to its partner and will not take them \
                          back on its own."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "opnsense_carp_maintenance",
                "Firewall left in CARP maintenance mode",
                RuleKind::Threshold,
                "dumbmonit_opnsense_carp_maintenance_mode",
            )
        },
        // Mise à jour posée mais pas démarrée : le noyau ou un micrologiciel
        // attend un redémarrage, et le pare-feu tourne encore sur l'ancien.
        Rule {
            description: "An update is installed on the firewall but needs a reboot to take \
                          effect."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "opnsense_reboot_pending",
                "Firewall reboot pending",
                RuleKind::Threshold,
                "dumbmonit_opnsense_firmware_reboot_required",
            )
        },
        // Mises à jour en attente : un pare-feu est la machine la plus exposée du
        // réseau, et c'est la seule dont les mises à jour comptent vraiment.
        Rule {
            description: "The firewall has packages waiting to be updated. It is the most \
                          exposed machine on the network."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "opnsense_updates_pending",
                "Firewall updates pending",
                RuleKind::Threshold,
                "dumbmonit_opnsense_firmware_updates_pending",
            )
        },
        // --- fin du bloc OPNsense ---
        // --- TrueNAS : pools, disques, protection des données (`collectors/truenas`) ---
        //
        // La panne pour laquelle ce bloc existe : un vdev redondant qui a perdu un
        // disque sert toujours les données. Le partage reste monté, les
        // sauvegardes passent, et personne ne le voit avant le deuxième disque.
        // Les règles qui suivent visent ces pannes-là, avant la panne franche.
        //
        // `pool_healthy` est l'avis de ZFS lui-même : il tombe à 0 sur un pool
        // DEGRADED comme sur un pool FAULTED. Cinq minutes absorbent un disque
        // qu'on remplace à chaud.
        Rule {
            description: "A pool is no longer healthy. A degraded pool still serves its data, which \
                          is exactly why nobody notices until the next disk fails."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "truenas_pool_degraded",
                "NAS pool degraded",
                RuleKind::Threshold,
                "dumbmonit_truenas_pool_healthy",
            )
        },
        // Erreurs de lecture, d'écriture ou de somme de contrôle : ZFS les a
        // corrigées tant qu'il avait de la redondance, mais un disque qui en
        // accumule est en train de partir. Elles restent jusqu'au `zpool clear`.
        Rule {
            description: "Disks in this pool have reported read, write or checksum errors. ZFS \
                          repaired what it could; a disk that keeps doing this is on its way out."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "truenas_pool_device_errors",
                "NAS disk errors",
                RuleKind::Threshold,
                "dumbmonit_truenas_pool_device_errors",
            )
        },
        // Un pool ZFS ralentit nettement passé 80 % et se fragmente ; à 90 % il
        // devient pénible, à 100 % il refuse d'écrire.
        Rule {
            description: "A pool is more than 85% full. ZFS slows down well before it is full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 85.0,
            clear_threshold: Some(80.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            escalate_after: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "truenas_pool_almost_full",
                "NAS pool almost full",
                RuleKind::Threshold,
                "dumbmonit_truenas_pool_used_percent",
            )
        },
        // Une vérification qui trouve des erreurs a trouvé des données abîmées ;
        // elle les a réparées si la redondance le permettait, pas sinon.
        Rule {
            description: "The last scrub of this pool found errors: data on disk was damaged."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "truenas_scrub_errors",
                "NAS scrub found errors",
                RuleKind::Threshold,
                "dumbmonit_truenas_pool_last_scrub_errors",
            )
        },
        // Sans vérification, une corruption silencieuse n'est découverte qu'à la
        // lecture — souvent le jour de la restauration. TrueNAS en planifie une
        // tous les 35 jours par défaut : 45 jours laissent passer un retard.
        Rule {
            description: "This pool has not completed a scrub in more than 45 days: silent \
                          corruption would only be found when the data is read."
                .to_string(),
            operator: Operator::Gt,
            threshold: 45.0 * 86_400.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Info,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(7 * 24 * 3600)),
            ..base(
                "truenas_scrub_overdue",
                "NAS scrub overdue",
                RuleKind::Threshold,
                "dumbmonit_truenas_pool_last_scrub_age_seconds",
            )
        },
        // Le journal SMART garde les échecs : la série vaut 1 tant qu'un test
        // du journal a échoué, et n'existe pas pour un disque jamais testé.
        Rule {
            description: "A SMART self-test of this disk failed. Replace it before it takes the \
                          pool's redundancy with it."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "truenas_disk_smart_failed",
                "NAS disk failed its SMART test",
                RuleKind::Threshold,
                "dumbmonit_truenas_disk_smart_failed",
            )
        },
        Rule {
            description: "A disk of this NAS is above 55 °C.".to_string(),
            operator: Operator::Gt,
            threshold: 55.0,
            clear_threshold: Some(50.0),
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            unit: "°C".to_string(),
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "truenas_disk_hot",
                "NAS disk too hot",
                RuleKind::Threshold,
                "dumbmonit_truenas_disk_temperature_celsius",
            )
        },
        // Un quota plein refuse les écritures de ce jeu de données seul, alors
        // que le pool a de la place : l'application qui écrit dedans tombe.
        Rule {
            description: "A dataset has used more than 90% of its quota. At 100% its writes fail \
                          while the pool still has room."
                .to_string(),
            operator: Operator::Gt,
            threshold: 90.0,
            clear_threshold: Some(85.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "truenas_dataset_quota_full",
                "NAS dataset near its quota",
                RuleKind::Threshold,
                "dumbmonit_truenas_dataset_quota_used_percent",
            )
        },
        // Une réplication en échec laisse la copie distante vieillir sans rien
        // dire. La série n'existe que pour une tâche activée.
        Rule {
            description: "A replication task failed: the copy on the other side is getting older."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "truenas_replication_failed",
                "NAS replication failed",
                RuleKind::Threshold,
                "dumbmonit_truenas_replication_error",
            )
        },
        Rule {
            description: "A periodic snapshot task failed: no new snapshot is being taken."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "truenas_snapshot_task_failed",
                "NAS snapshot task failed",
                RuleKind::Threshold,
                "dumbmonit_truenas_snapshot_task_error",
            )
        },
        // Une tâche d'instantanés qui ne tourne plus ne tombe pas en erreur :
        // elle se tait. Huit jours laissent passer une tâche hebdomadaire.
        Rule {
            description: "A periodic snapshot task has not run for more than eight days."
                .to_string(),
            operator: Operator::Gt,
            threshold: 8.0 * 86_400.0,
            for_duration: Duration::from_secs(3600),
            severity: Severity::Warning,
            unit: "s".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "truenas_snapshots_stale",
                "NAS snapshots stale",
                RuleKind::Threshold,
                "dumbmonit_truenas_snapshot_task_last_run_age_seconds",
            )
        },
        // TrueNAS calcule lui-même l'état des pools, SMART, la capacité, les
        // certificats, l'onduleur : relayer ses alertes graves est le filet le
        // plus large. La série de chaque niveau existe toujours, zéro compris,
        // ce qui fait retomber la règle quand l'alerte disparaît.
        Rule {
            description: "TrueNAS has raised an alert of level ERROR or above. Its own sentence is \
                          on the device page."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(12 * 3600)),
            ..base(
                "truenas_alert_raised",
                "TrueNAS alert raised",
                RuleKind::Threshold,
                "sum by (target, host) (dumbmonit_truenas_alerts{level=~\"ERROR|CRITICAL|ALERT|EMERGENCY\"})",
            )
        },
        // Seuls les services qui démarrent avec le NAS sont mesurés : un service
        // qu'on a éteint exprès n'a pas de série.
        Rule {
            description: "A service set to start with the NAS is stopped.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "truenas_service_down",
                "NAS service stopped",
                RuleKind::Threshold,
                "dumbmonit_truenas_service_running",
            )
        },
        // --- fin du bloc TrueNAS ---
        // --- Serveurs de journaux et de métriques (`collectors/observability`) ---
        //
        // Surveiller la surveillance. Ces serveurs tombent rarement tout à fait :
        // ils répondent, l'interface s'ouvre, et ils refusent en silence une partie
        // de ce qu'on leur envoie. Les règles visent ces refus-là. Les compteurs
        // passent par `increase_prometheus`, jamais `increase`, pour qu'un serveur
        // ajouté avec un historique de rejets ne sonne pas dès la première mesure.
        //
        // VictoriaMetrics et VictoriaLogs passent en lecture seule sous
        // `-storage.minFreeDiskSpaceBytes` d'espace libre : toute écriture est
        // refusée. C'est une panne franche de l'ingestion.
        Rule {
            description: "The storage switched to read-only because free disk space fell below \
                          -storage.minFreeDiskSpaceBytes: every new sample or log line is refused."
                .to_string(),
            operator: Operator::Ge,
            threshold: 1.0,
            for_duration: Duration::from_secs(2 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "victoria_read_only",
                "Metrics or log storage read-only",
                RuleKind::Threshold,
                "dumbmonit_victoriametrics_read_only or dumbmonit_victorialogs_read_only",
            )
        },
        // La marge avant la lecture seule, en pourcentage du disque : moins de
        // 10 % laisse le temps d'agrandir le volume ou de raccourcir la rétention.
        Rule {
            description: "Less than 10% of the disk is left before the storage switches to \
                          read-only and refuses new data."
                .to_string(),
            operator: Operator::Lt,
            threshold: 10.0,
            clear_threshold: Some(12.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "victoria_disk_headroom_low",
                "Metrics or log storage almost read-only",
                RuleKind::Threshold,
                "dumbmonit_victoriametrics_disk_headroom_percent \
                 or dumbmonit_victorialogs_disk_headroom_percent",
            )
        },
        Rule {
            description: "The server does not answer OK on /health.".to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(3 * 60),
            severity: Severity::Critical,
            ..base(
                "victoria_unhealthy",
                "Metrics or log server unhealthy",
                RuleKind::Threshold,
                "dumbmonit_victoriametrics_healthy or dumbmonit_victorialogs_healthy",
            )
        },
        // Lignes refusées : horodatage hors rétention ou trop loin dans le futur,
        // étiquettes trop longues, limite de séries. Chacune est une donnée perdue.
        Rule {
            description: "The server refused samples or log lines in the last hour: timestamps \
                          outside retention, labels too long, or a series limit reached."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "victoria_rows_rejected",
                "Metrics or log data refused",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_victoriametrics_rows_rejected_total[1h]) \
                 or increase_prometheus(dumbmonit_victorialogs_rows_rejected_total[1h])",
            )
        },
        // Le seuil que la documentation de VictoriaMetrics donne lui-même : au-delà
        // de 5 % d'insertions lentes, le cache des séries ne tient plus en mémoire.
        // Une demi-heure écarte la vague de séries neuves d'un déploiement.
        //
        // Après un redémarrage, le cache est froid et la fenêtre de quinze
        // minutes mêle des points d'avant et d'après : la règle restait en
        // `pending`, ni déclenchée ni résolue, jusqu'à ce que la fenêtre se
        // vide. Elle ne juge donc qu'un serveur démarré depuis plus que la
        // fenêtre, dont les compteurs ne contiennent plus de remise à zéro.
        Rule {
            description: "More than 5% of new samples take the slow insert path: VictoriaMetrics \
                          lacks memory for its active series."
                .to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            clear_threshold: Some(3.0),
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "victoriametrics_slow_inserts",
                "VictoriaMetrics slow inserts",
                RuleKind::Threshold,
                VM_SLOW_INSERTS_QUERY,
            )
        },
        // Loki : `/ready` dit s'il accepte d'écrire et de lire. Cinq minutes
        // couvrent le démarrage d'un ingester qui rejoue son WAL.
        Rule {
            description: "Loki does not answer ready on /ready: it cannot accept or serve logs."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            ..base("loki_not_ready", "Loki not ready", RuleKind::Threshold, "dumbmonit_loki_ready")
        },
        Rule {
            description: "Loki refused log lines in the last hour: too old, rate or stream limit \
                          reached, or line too long. The sender drops them."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "loki_lines_discarded",
                "Loki refusing log lines",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_loki_discarded_lines_total[1h])",
            )
        },
        // Un bloc qui ne part pas vers le stockage reste en mémoire : il est
        // perdu au prochain redémarrage.
        Rule {
            description: "Loki failed to write chunks to storage: they are held in memory and \
                          lost if Loki restarts."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "loki_flush_failing",
                "Loki cannot flush to storage",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_loki_flush_failures_total[30m])",
            )
        },
        Rule {
            description: "Loki could not write to its write-ahead log because the disk is full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "loki_wal_disk_full",
                "Loki write-ahead log disk full",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_loki_wal_disk_full_failures_total[30m])",
            )
        },
        Rule {
            description: "Loki answered more than five requests with a server error in fifteen \
                          minutes."
                .to_string(),
            operator: Operator::Gt,
            threshold: 5.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(6 * 3600)),
            ..base(
                "loki_request_errors",
                "Loki request errors",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_loki_request_errors_total[15m])",
            )
        },
        // Graylog : la panne typique est OpenSearch qui ne suit plus. Graylog
        // continue d'accepter, range dans son journal, et les recherches ne
        // montrent plus rien de récent.
        Rule {
            description: "Graylog has paused message processing on this node: messages pile up \
                          in the journal."
                .to_string(),
            operator: Operator::Lt,
            threshold: 1.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            ..base(
                "graylog_not_processing",
                "Graylog not processing",
                RuleKind::Threshold,
                "dumbmonit_graylog_processing",
            )
        },
        // 2 rouge, 3 injoignable depuis Graylog.
        Rule {
            description: "Graylog's search cluster is red or unreachable: new messages cannot be \
                          indexed."
                .to_string(),
            operator: Operator::Ge,
            threshold: 2.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Critical,
            repeat_interval: Some(Duration::from_secs(3600)),
            ..base(
                "graylog_search_cluster_down",
                "Graylog search cluster down",
                RuleKind::Threshold,
                "dumbmonit_graylog_indexer_status",
            )
        },
        Rule {
            description: "Graylog's search cluster has been yellow for half an hour: replica \
                          shards are not assigned, one more node lost would lose data."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(30 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "graylog_search_cluster_yellow",
                "Graylog search cluster yellow",
                RuleKind::Threshold,
                "dumbmonit_graylog_indexer_status == 1",
            )
        },
        // Graylog prévient lui-même vers 95 % ; à moitié plein, il reste de quoi
        // réparer la sortie avant de perdre des messages.
        Rule {
            description: "Graylog's journal is more than half full: the output does not keep up \
                          and messages will be dropped once it is full."
                .to_string(),
            operator: Operator::Gt,
            threshold: 50.0,
            clear_threshold: Some(40.0),
            for_duration: Duration::from_secs(10 * 60),
            severity: Severity::Warning,
            unit: "%".to_string(),
            escalate_after: Some(Duration::from_secs(3600)),
            ..base(
                "graylog_journal_filling",
                "Graylog journal filling",
                RuleKind::Threshold,
                "dumbmonit_graylog_journal_used_percent",
            )
        },
        Rule {
            description: "A Graylog input failed to start: whatever sends to it is not received."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(5 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "graylog_input_failed",
                "Graylog input failed",
                RuleKind::Threshold,
                "dumbmonit_graylog_inputs_failed",
            )
        },
        Rule {
            description: "Graylog failed to write messages to its search cluster in the last \
                          hour."
                .to_string(),
            operator: Operator::Gt,
            threshold: 0.0,
            for_duration: Duration::from_secs(15 * 60),
            severity: Severity::Warning,
            repeat_interval: Some(Duration::from_secs(24 * 3600)),
            ..base(
                "graylog_indexing_failures",
                "Graylog indexing failures",
                RuleKind::Threshold,
                "increase_prometheus(dumbmonit_graylog_output_failures_total[1h]) \
                 + increase_prometheus(dumbmonit_graylog_processing_failures_total[1h])",
            )
        },
        // --- fin du bloc serveurs de journaux et de métriques ---
    ]
}
