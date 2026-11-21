//! Réglages des équipements d'infrastructure : Proxmox VE, PBS, PDM, PMG,
//! Redfish, OPNsense, TrueNAS, Synology, MDaemon et SecurityGateway.

use super::*;

/// Options lues par `collectors/proxmox/options.rs`.
pub(super) const PROXMOX_OPTIONS: &[OptionView] = &[
    number("port", "API port", "Used if the address does not give a port.", "8006", "8006"),
    insecure_tls(
        "Proxmox ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120.",
        "10",
        "10",
    ),
    grouped(
        number(
            "backup_lookback_days",
            "Backup lookback (days)",
            "Older backup tasks are not examined, from 1 to 3650.",
            "31",
            "31",
        ),
        "Backups & replication",
    ),
    grouped(
        boolean(
            "scan_backup_storage",
            "Inventory backup archives",
            "Scans the backup storages to date the last backup of each machine. Disable if the storage is slow to answer.",
            true,
        ),
        "Backups & replication",
    ),
    grouped(
        text(
            "nodes",
            "Monitored nodes",
            "Names of the nodes to monitor, separated by commas. Empty: every node in the cluster.",
            "pve1, pve2",
            "",
        ),
        "Inventory",
    ),
    grouped(
        boolean(
            "ha",
            "Watch high availability",
            "Reads the HA manager state: quorum, master, LRMs and the state of each HA resource.",
            true,
        ),
        "Performance & capacity",
    ),
    grouped(
        boolean(
            "backup_jobs",
            "Watch backup jobs",
            "Reads the scheduled backup jobs (next run, last result) and lists the guests no job covers.",
            true,
        ),
        "Backups & replication",
    ),
    grouped(
        boolean(
            "scan_snapshots",
            "Inventory snapshots",
            "Lists the snapshots of every VM and container to report their number and age. One API call per guest.",
            true,
        ),
        "Backups & replication",
    ),
    grouped(
        number(
            "max_snapshot_guests",
            "Snapshot inventory limit",
            "Maximum number of guests whose snapshots are listed per probe, from 1 to 10000. Beyond it, the remaining guests are counted as skipped.",
            "200",
            "200",
        ),
        "Backups & replication",
    ),
    grouped(
        boolean(
            "replication",
            "Watch replication jobs",
            "Reads the state of ZFS replication jobs on every node.",
            true,
        ),
        "Backups & replication",
    ),
    grouped(
        boolean(
            "ceph",
            "Watch Ceph",
            "Reads the Ceph cluster health, OSDs and usage. Silently skipped when Ceph is not set up.",
            true,
        ),
        "Cluster services",
    ),
    grouped(
        boolean(
            "updates",
            "Count pending updates",
            "Lists the packages waiting for an update on each node. Needs Sys.Modify on \"/nodes\" (see the docs); silently skipped otherwise.",
            true,
        ),
        "Security & certificates",
    ),
    grouped(
        boolean(
            "certificates",
            "Watch node certificates",
            "Reports the days left before each node certificate expires.",
            true,
        ),
        "Security & certificates",
    ),
    grouped(
        boolean(
            "guest_agent",
            "Ask the QEMU guest agent",
            "For each running VM: balloon memory and, when the guest agent is enabled, the disk usage seen from inside (needs VM.GuestAgent.Audit, or VM.Monitor before Proxmox VE 9). Silently skipped when the agent is absent.",
            true,
        ),
        "Cluster services",
    ),
    grouped(
        boolean(
            "disks",
            "Watch physical disks",
            "SMART health, wearout and temperature of every disk of each node.",
            true,
        ),
        "Performance & capacity",
    ),
    grouped(
        boolean(
            "zfs",
            "Watch ZFS pools",
            "Health, capacity and fragmentation of the ZFS pools of each node.",
            true,
        ),
        "Performance & capacity",
    ),
    grouped(
        boolean(
            "packages",
            "Detect package changes",
            "Compares the installed Proxmox packages with the previous probe and reports an upgrade for one hour.",
            true,
        ),
        "Inventory",
    ),
    grouped(
        boolean(
            "subscription",
            "Watch subscription and repositories",
            "Subscription status and APT repositories of each node (enterprise without subscription, unreadable sources).",
            true,
        ),
        "Security & certificates",
    ),
    grouped(
        boolean(
            "cluster_resources",
            "Read the cluster inventory",
            "One call lists every node, guest, storage and pool of the cluster. Keep it on: it is the only source that still sees the guests of a node that stopped answering, and it saves two calls per node.",
            true,
        ),
        "Inventory",
    ),
    grouped(
        boolean(
            "services",
            "Watch node services",
            "State of the Proxmox daemons of each node (pvestatd, pveproxy, pve-cluster, corosync…) and the version installed on it. A stopped pvestatd leaves the whole cluster showing frozen numbers.",
            true,
        ),
        "Cluster services",
    ),
    grouped(
        boolean(
            "network",
            "Watch node networking",
            "Bridges, bonds and VLANs of each node with their link state, plus the traffic counters of each guest network card.",
            true,
        ),
        "Performance & capacity",
    ),
    grouped(
        boolean(
            "lvm",
            "Watch LVM and thin pools",
            "Volume groups, LVM thin pools (data and metadata fill) and PVE-managed directory mounts. A full thin pool puts every guest on it read-only.",
            true,
        ),
        "Performance & capacity",
    ),
    grouped(
        boolean(
            "ceph_detail",
            "Watch Ceph in detail",
            "Per-OSD state, usage and latency, per-pool usage, CephFS, OSD flags and muted health checks. Needs \"Watch Ceph\" to be on; silently skipped when Ceph is not set up.",
            true,
        ),
        "Cluster services",
    ),
    grouped(
        boolean(
            "backup_volumes",
            "Check what backup jobs include",
            "For each scheduled job, which guests it covers and which of their disks it actually writes. Catches a job that succeeds every night while skipping a data disk.",
            true,
        ),
        "Backups & replication",
    ),
    grouped(
        boolean(
            "guest_os",
            "Read guest OS and addresses",
            "Operating system and IP addresses seen from inside each running guest (QEMU guest agent for VMs, the container namespace for containers). Refreshed once an hour, not at every probe.",
            true,
        ),
        "Inventory",
    ),
    grouped(
        boolean(
            "metrics_export",
            "Ingest the full RRD metric stream",
            "Reads /cluster/metrics/export, the stream Proxmox's own metric servers consume: everything pvestatd measures, including per-node pressure stall (PSI), and every point since the last probe rather than just the current one. Off by default: the series it adds (…_rrd_…) partly repeat the ones already collected. Needs Sys.Audit on \"/\".",
            false,
        ),
        "Performance & capacity",
    ),
];

/// Options lues par `collectors/pbs/options.rs`.
pub(super) const PBS_OPTIONS: &[OptionView] = &[
    number("port", "API port", "Used if the address does not give a port.", "8007", "8007"),
    insecure_tls(
        "Proxmox Backup Server ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. Listing the snapshots of a large datastore can take several seconds.",
        "15",
        "15",
    ),
    number(
        "task_lookback_hours",
        "Task window (hours)",
        "Older tasks are not counted among the failures, from 1 to 8760.",
        "24",
        "24",
    ),
    text(
        "datastores",
        "Monitored datastores",
        "Names of the datastores to monitor, separated by commas. Empty: every datastore.",
        "main, archive",
        "",
    ),
    number(
        "max_groups",
        "Backup group limit",
        "Maximum number of backed-up machines producing series; beyond it, the oldest are ignored. From 1 to 100000.",
        "500",
        "500",
    ),
    boolean(
        "jobs",
        "Watch sync, verify and prune jobs",
        "Reads the job lists to report each job's last result and next run. Needs Datastore.Audit on the datastores, plus Remote.Audit for sync jobs.",
        true,
    ),
    boolean(
        "updates",
        "Count pending updates",
        "Lists the packages waiting for an update on the backup server. Needs Sys.Audit on \"/\"; silently skipped otherwise.",
        true,
    ),
    boolean(
        "disks",
        "Watch disks and ZFS pools",
        "Reads the physical disks (SMART verdict, SSD wear) and the ZFS pools of the backup server (/nodes/localhost/disks). Needs Sys.Audit on \"/system\"; silently skipped otherwise.",
        true,
    ),
    boolean(
        "services",
        "Watch the server's services",
        "Reads the systemd units of the backup server and reports whether the ones it cannot do without are running. Needs Sys.Audit on \"/system\"; silently skipped otherwise.",
        true,
    ),
    boolean(
        "datastore_details",
        "Read datastore detail",
        "Two more calls per datastore: how many machines and snapshots it holds, what is reading or writing it right now, and whether it is held for maintenance. Cheap, and it explains a garbage collection that will not finish.",
        true,
    ),
    boolean(
        "traffic_control",
        "Watch traffic limits",
        "Reads the rate limits configured on the backup server and what they are carrying right now. Needs Sys.Audit; silently skipped otherwise.",
        true,
    ),
    boolean(
        "certificates",
        "Watch certificate expiry",
        "Reads the certificate the web interface serves and warns before it expires. Off by default: Proxmox Backup Server guards this one call behind Sys.Modify, a write privilege a monitoring token should not be given. Turn it on only if you granted it.",
        false,
    ),
    boolean(
        "tape",
        "Watch tape backups",
        "Reads the tape tier: backup jobs and their last run, drives, changers, media pools and the tapes themselves. Off by default, since most installations have no tape hardware. Needs Tape.Audit on \"/tape\".",
        false,
    ),
    DEVICE_STALE_DAYS,
];

/// Options lues par `collectors/pdm/options.rs`.
pub(super) const PDM_OPTIONS: &[OptionView] = &[
    number("port", "API port", "Used if the address does not give a port.", "8443", "8443"),
    insecure_tls(
        "Proxmox Datacenter Manager ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. The console relays calls to every federated instance, so one slow site holds the whole answer.",
        "20",
        "20",
    ),
    text(
        "node",
        "Console node name",
        "Name of the node that runs the console. A Datacenter Manager has a single node and calls it \"localhost\".",
        "localhost",
        "localhost",
    ),
    number(
        "task_lookback_hours",
        "Task window (hours)",
        "Older tasks are not counted among the failures, from 1 to 8760.",
        "24",
        "24",
    ),
    number(
        "max_age_seconds",
        "Accepted cache age (seconds)",
        "How stale the console's own inventory may be before it queries the federated instances again, from 0 to 3600. Zero asks for fresh data on every probe, which puts every cluster back on the line.",
        "60",
        "60",
    ),
    text(
        "remotes",
        "Monitored instances",
        "Names of the federated instances to monitor, separated by commas. Empty: every instance.",
        "site-a, site-b",
        "",
    ),
    number(
        "max_remotes",
        "Instance version limit",
        "Maximum number of instances asked for their version on each probe, from 1 to 1000.",
        "100",
        "100",
    ),
    boolean(
        "versions",
        "Read each instance version",
        "One call per federated instance, to report its version and to flag the ones left behind their peers. An instance that does not answer counts as unreachable.",
        true,
    ),
    boolean(
        "tasks",
        "Watch tasks across instances",
        "Reads the task list of every federated instance to report the failures. Needs Resource.Audit; silently skipped otherwise.",
        true,
    ),
    boolean(
        "node_status",
        "Watch the console host",
        "Reads the CPU, memory, root filesystem, uptime, certificates and subscription of the machine running the console. Needs Sys.Audit on \"/system\"; silently skipped otherwise.",
        true,
    ),
    boolean(
        "updates",
        "Count pending updates",
        "Lists the packages waiting for an update on the console itself. Needs Sys.Audit; silently skipped otherwise.",
        true,
    ),
    boolean(
        "remote_updates",
        "Count updates on each instance",
        "Reads the console's update summary for the federated instances. Off by default: the console guards that path with Resource.Modify, a write privilege a monitoring account has no reason to hold.",
        false,
    ),
];

/// Options lues par `collectors/pmg/options.rs`.
pub(super) const PMG_OPTIONS: &[OptionView] = &[
    number("port", "API port", "Used if the address does not give a port.", "8006", "8006"),
    insecure_tls(
        "Proxmox Mail Gateway ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. Reading a queue runs a Postfix command on the gateway and can take a few seconds.",
        "15",
        "15",
    ),
    text(
        "node",
        "Monitored node",
        "Name of the cluster node to monitor. Empty: every node the gateway lists.",
        "mail1",
        "",
    ),
    number(
        "recent_hours",
        "Traffic window (hours)",
        "How far back the traffic curve goes, from 1 to 24. The daily totals always cover the current day.",
        "12",
        "12",
    ),
    boolean(
        "queues",
        "Watch the mail queues",
        "Reads the incoming, active, deferred and hold queues and the age of the oldest message in each. A growing deferred queue is the first sign that mail is stuck.",
        true,
    ),
    boolean(
        "quarantine",
        "Count the quarantines",
        "Reads how many messages the spam and virus quarantines hold, and how much disk they use. Counts only: no subject, sender or message content is ever read.",
        true,
    ),
    boolean(
        "attachment_quarantine",
        "Also count the attachment quarantine",
        "That quarantine has no count call: it has to be listed to be counted. Off by default; only the number of entries is kept.",
        false,
    ),
    boolean(
        "signatures",
        "Watch the signature databases",
        "Reads the age of the ClamAV virus databases and of the SpamAssassin rule channels. Out-of-date signatures fail silently: the gateway keeps filtering, badly.",
        true,
    ),
    boolean(
        "services",
        "Watch the services",
        "Reads the state of postfix, pmg-smtp-filter, pmgpolicy and the other units of each node.",
        true,
    ),
    boolean(
        "certificates",
        "Watch the certificates",
        "Reads the certificates the web interface serves and warns before they expire.",
        true,
    ),
    boolean(
        "updates",
        "Count pending updates",
        "Lists the packages waiting for an update on each node. Needs the Audit role; silently skipped otherwise.",
        true,
    ),
    boolean(
        "subscription",
        "Read the subscription",
        "Reads the subscription status of each node. An installation without a key reports \"notfound\", which is not an error.",
        true,
    ),
];

/// Options lues par `collectors/redfish/options.rs`.
pub(super) const REDFISH_OPTIONS: &[OptionView] = &[
    number("port", "HTTPS port", "Used if the address does not give a port.", "443", "443"),
    insecure_tls(
        "Management controllers ship with a self-signed certificate: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each Redfish call, from 1 to 120. A management controller is a small processor: a few seconds per answer is normal.",
        "8",
        "8",
    ),
    select(
        "auth",
        "Authentication",
        "basic sends the user name and password with every request and leaves nothing open on the controller. session logs in once and reuses the session token; use it only if the controller refuses basic authentication, since controllers have few session slots.",
        "basic",
        &["basic", "session"],
    ),
    boolean(
        "storage",
        "Watch the drives",
        "Reads the storage controllers and every drive behind them: health, predicted failure, remaining SSD life. One request per drive.",
        true,
    ),
    boolean(
        "logs",
        "Count log entries",
        "Counts the entries of the system event log and of the controller's own log, by severity. The messages themselves are never read.",
        true,
    ),
];

/// Options lues par `collectors/opnsense/options.rs`.
pub(super) const OPNSENSE_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "HTTPS fits a firewall out of the box. HTTP only if the web interface is served in clear text.",
        "https",
        &["https", "http"],
    ),
    number("port", "Web interface port", "Used if the address does not give a port.", "443", "443"),
    insecure_tls(
        "OPNsense ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. Most answer instantly; the firmware status is the slow one.",
        "15",
        "15",
    ),
    boolean(
        "gateways",
        "Watch the gateways",
        "Reads what dpinger says about each gateway: up or down, round-trip delay and packet loss. On a multi-WAN firewall this is the one thing that fails without anyone noticing.",
        true,
    ),
    boolean(
        "interfaces",
        "Watch the interfaces",
        "Reads each interface's link state, addresses and byte, packet, error and drop counters. The addresses are where the current public address shows up.",
        true,
    ),
    boolean(
        "firewall",
        "Watch the state table",
        "Reads how many connections pf is tracking and the configured limit. A firewall that fills its state table refuses connections with no other symptom.",
        true,
    ),
    boolean(
        "dhcp",
        "Count the DHCP leases",
        "Counts the leases the firewall is handing out, whichever server it runs. Counts only: no address, host name or hardware address is ever read.",
        true,
    ),
    boolean(
        "vpn",
        "Watch the VPN tunnels",
        "Reads WireGuard, OpenVPN and IPsec: which tunnels exist and which ones are up. A plugin that is not installed is skipped in silence.",
        true,
    ),
    boolean(
        "unbound",
        "Watch the resolver",
        "Reads whether Unbound is answering. A firewall that routes but no longer resolves looks healthy to everyone except the machines behind it.",
        true,
    ),
    boolean(
        "services",
        "Watch the services",
        "Reads the list of services the firewall manages and which of them are running.",
        true,
    ),
    boolean(
        "carp",
        "Watch CARP",
        "Reads the virtual addresses of a high-availability pair and whether this firewall holds them. Also reports persistent maintenance mode, which is the state everyone forgets to switch off.",
        true,
    ),
    boolean(
        "firmware",
        "Watch for updates",
        "Reads the result of the firewall's last update check and whether a reboot is pending. The check itself is never triggered: that would send the firewall to the mirror on every measurement.",
        true,
    ),
    boolean(
        "temperature",
        "Read the temperature sensors",
        "Reads the CPU and board sensors the firewall exposes. A machine without sensors reports none, which is not an error.",
        true,
    ),
    boolean(
        "certificates",
        "Watch the certificates",
        "Reads the expiry date of the certificates the firewall keeps (web interface, captive portal, VPN); those OPNsense marks as unused are ignored. Needs the privilege System: Trust: Certificates; without it the option is skipped, which is not an error.",
        true,
    ),
    boolean(
        "live_map",
        "Live firewall map",
        "Shows on the device page where the connections your rules block or pass come from, by country. Reads the firewall log every few seconds, only while someone is looking, and keeps fifteen minutes in memory; no address is stored or shown, and countries are found in a local database. Needs the privilege Diagnostics: Logs: Firewall: Live View.",
        false,
    ),
];

/// Options lues par `collectors/truenas/options.rs`.
pub(super) const TRUENAS_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "Keep HTTPS: TrueNAS revokes an API key it receives over plain HTTP from another machine. HTTP only behind a reverse proxy on the NAS itself.",
        "https",
        &["https", "http"],
    ),
    number("port", "Web interface port", "Used if the address does not give a port.", "443", "443"),
    insecure_tls(
        "TrueNAS ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. The service list probes every daemon and can take a while on a busy NAS.",
        "20",
        "20",
    ),
    boolean(
        "datasets",
        "Watch the datasets",
        "Reads each dataset's usage against its quota, its snapshot count and when its snapshots last changed. Counts only, from ZFS's own cache: no snapshot is listed.",
        true,
    ),
    boolean(
        "disks",
        "Watch the disks",
        "Reads the disk inventory and the temperatures TrueNAS already has in cache. A sleeping disk is never woken up for a measurement.",
        true,
    ),
    boolean(
        "smart",
        "Read the SMART test results",
        "Reads the last self-test of each disk. No test is ever started. TrueNAS 25.10 no longer offers this call; it is then skipped in silence.",
        true,
    ),
    boolean(
        "alerts",
        "Read TrueNAS's own alerts",
        "Reads the alert list TrueNAS keeps itself — pool state, SMART, capacity, certificates, failed replications. Dismissed alerts are left out.",
        true,
    ),
    boolean(
        "tasks",
        "Watch replication and snapshot tasks",
        "Reads the state of every replication and periodic snapshot task, its last run and the last snapshot it handled.",
        true,
    ),
    boolean(
        "services",
        "Watch the services",
        "Reads the services set to start with the NAS and whether they are running. A service you switched off is not watched.",
        true,
    ),
];

/// Options lues par `collectors/synology/options.rs`.
pub(super) const SYNOLOGY_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "HTTPS fits a NAS fresh out of the box. HTTP only if DSM listens in clear text only.",
        "https",
        &["https", "http"],
    ),
    number(
        "port",
        "DSM port",
        "Used if the address does not give a port. Empty: 5001 over HTTPS, 5000 over HTTP.",
        "5001",
        "",
    ),
    insecure_tls(
        "A NAS ships with a self-signed certificate by default: enable this if the connection is refused for that reason.",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per request (seconds)",
        "Time allowed for each API call, from 1 to 120. The storage inventory can wake up sleeping disks.",
        "15",
        "15",
    ),
    boolean(
        "abb",
        "Watch Active Backup for Business",
        "Reads the Active Backup for Business tasks (PCs, servers, virtual machines, file servers): last result, age of the last successful backup, schedule. Needs the package installed and an account allowed to use it; a NAS without the package is simply skipped.",
        true,
    ),
    boolean(
        "drive",
        "Watch Synology Drive",
        "Reads whether the Synology Drive service responds, its team folders and its active connection count. A NAS without the package is simply skipped.",
        true,
    ),
    boolean(
        "photos",
        "Watch Synology Photos",
        "Shows whether the Synology Photos package is installed, with no extra request to the NAS. Synology does not expose a list of mobile backup devices through any API.",
        true,
    ),
];

/// Options lues par `collectors/mdaemon/email_server.rs`.
pub(super) const MDAEMON_OPTIONS: &[OptionView] = &[
    agent_link(
        "agent_id",
        "Agent installed on this server",
        "The DumbMonit agent that runs on the Windows server of MDaemon. It reads what cannot be read from outside: the mail queues, the sessions, the Windows services and the disk space. DumbMonit pre-selects the agent whose host name matches the address.",
    ),
    text(
        "services",
        "Services",
        "Comma-separated list of what to check: smtp, msa, smtps, pop3, pop3s, imap, imaps, webmail, remote_admin, remote_admin_https, xmpp. Write name:port for a port other than the default, and any other name with its port for a service of your own. none checks no port.",
        "smtp,imap,webmail",
        "smtp,imap,webmail",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per check (seconds)",
        "Time allowed for each connection, greeting and API call, from 1 to 60.",
        "10",
        "10",
    ),
    number(
        "api_port",
        "XML API port",
        "The Remote Administration port that serves /MdMgmtWS/. Used only with an account.",
        "444",
        "444",
    ),
    boolean(
        "api_tls",
        "XML API over HTTPS",
        "Untick only for Remote Administration over plain HTTP (port 1000 by default): the password then crosses the network in clear.",
        true,
    ),
    insecure_tls(
        "Remote Administration often uses a self-signed certificate: enable this if the connection is refused for that reason.",
    ),
];

/// Options lues par `collectors/mdaemon/security_gateway.rs`.
pub(super) const SECURITY_GATEWAY_OPTIONS: &[OptionView] = &[
    agent_link(
        "agent_id",
        "Agent installed on this server",
        "The DumbMonit agent that runs on the Windows server of SecurityGateway. It reads what cannot be read from outside: the Windows services, the mail queue folders and the disk space. DumbMonit pre-selects the agent whose host name matches the address.",
    ),
    text(
        "services",
        "Services",
        "Comma-separated list of what to check: smtp, smtps, web, web_https. Write name:port for a port other than the default, and any other name with its port for a service of your own. none checks no port.",
        "smtp,web",
        "smtp,web",
    ),
    number(
        "request_timeout_seconds",
        "Timeout per check (seconds)",
        "Time allowed for each connection, greeting and API call, from 1 to 60.",
        "10",
        "10",
    ),
    number(
        "api_port",
        "REST API port",
        "The web interface port that serves /api/v1. Used only with an API key.",
        "4443",
        "4443",
    ),
    boolean(
        "api_tls",
        "REST API over HTTPS",
        "Untick only for the web interface over plain HTTP (port 4000 by default): the key then crosses the network in clear.",
        true,
    ),
    insecure_tls(
        "The web interface often uses a self-signed certificate on 4443: enable this if the connection is refused for that reason.",
    ),
    boolean(
        "counters",
        "Read the performance counters",
        "Reads the read-only performance counters the REST API publishes (queues, quarantine, sessions) and keeps every numeric value as is.",
        true,
    ),
];
