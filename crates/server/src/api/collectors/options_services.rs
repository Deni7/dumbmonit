//! Identifiants et réglages des bases de données, files de messages,
//! CrowdSec, Kubernetes, Active Directory et serveurs web ou mandataires.

use super::*;

/// Utilisateur ACL de Redis 6 et suivants, ou de Valkey (`collectors/redis`).
pub(super) const REDIS_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "ACL user (recommended)",
    help: "A Redis user allowed to run INFO and PING only. Sent as AUTH user password.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

/// Mot de passe seul (`requirepass`), porté par un jeton : `AUTH motdepasse`.
pub(super) const REDIS_PASSWORD: CredentialView = CredentialView {
    kind: "api_token",
    label: "Password only",
    help: "For Redis 5 or older, or a server with only requirepass set. Sent as AUTH password.",
    fields: &[cred_secret(
        "token",
        "Password",
        "The requirepass value. Stored encrypted, never shown again.",
        "",
        true,
    )],
};

pub(super) const MONGODB_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "An account with the clusterMonitor role only, logged in with SCRAM-SHA-256.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

pub(super) const RABBITMQ_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "A user tagged monitoring, sent as HTTP basic authentication.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

/// Métriques seules : `/metrics` de CrowdSec n'a pas d'authentification.
pub(super) const CROWDSEC_METRICS_ONLY: CredentialView = CredentialView {
    kind: "none",
    label: "Metrics only",
    help: "Reads /metrics, and /health of the Local API. Nothing to create.",
    fields: &[],
};

/// Clé de bouncer, envoyée en `X-Api-Key` à la LAPI (`collectors/crowdsec`).
pub(super) const CROWDSEC_BOUNCER_KEY: CredentialView = CredentialView {
    kind: "api_token",
    label: "Bouncer API key",
    help: "Also proves the Local API answers decision queries. A bouncer key can only read decisions.",
    fields: &[cred_secret(
        "token",
        "Bouncer API key",
        "Shown once by cscli bouncers add. Stored encrypted, never shown again.",
        "",
        true,
    )],
};

/// Compte de machine de la LAPI (`collectors/crowdsec`) : lit les alertes
/// récentes, réduites à leur pays pour la carte des menaces.
pub(super) const CROWDSEC_MACHINE_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Machine login",
    help: "Also reads the alerts of the last 24 hours, counted by country for the threat map. Addresses are never kept.",
    fields: &[
        cred_text("username", "Machine name", "The name given to cscli machines add.", "dumbmonit"),
        cred_secret(
            "password",
            "Password",
            "The password given to cscli machines add. Stored encrypted, never shown again.",
            "",
            true,
        ),
    ],
};

/// Délai commun aux intégrations Redis et MongoDB (`collectors/socket.rs`).
pub(super) const SOCKET_TIMEOUT: OptionView = number(
    "request_timeout_seconds",
    "Timeout (seconds)",
    "Time allowed for the whole check: connection, login and commands, from 1 to 120.",
    "10",
    "10",
);

pub(super) const fn socket_port(default: &'static str) -> OptionView {
    number("port", "Port", "Used if the address does not give a port.", default, default)
}

pub(super) const SOCKET_TLS: OptionView =
    boolean("tls", "TLS", "Tick when the server only accepts TLS connections on that port.", false);

pub(super) const SOCKET_INSECURE_TLS: OptionView = insecure_tls(
    "For a certificate from a private authority or a self-signed one: enable this if the connection is refused for that reason.",
);

/// Options lues par `collectors/redis/mod.rs` et `collectors/socket.rs`.
pub(super) const REDIS_OPTIONS: &[OptionView] =
    &[socket_port("6379"), SOCKET_TLS, SOCKET_INSECURE_TLS, SOCKET_TIMEOUT];

/// Options lues par `collectors/mongodb/mod.rs` et `collectors/socket.rs`.
pub(super) const MONGODB_OPTIONS: &[OptionView] = &[
    socket_port("27017"),
    text(
        "auth_source",
        "Authentication database",
        "The database the account was created in.",
        "admin",
        "admin",
    ),
    SOCKET_TLS,
    SOCKET_INSECURE_TLS,
    SOCKET_TIMEOUT,
];

/// Options lues par `collectors/rabbitmq/mod.rs` et `observability/options.rs`.
pub(super) const RABBITMQ_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The management API listens over plain HTTP unless you configured TLS for it (usually on port 15671).",
        "http",
        &["http", "https"],
    ),
    observability_port("15672"),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
    number(
        "max_queues",
        "Queues watched one by one",
        "The fullest queues are charted one by one, up to this number (0 to 500). Totals always cover every queue.",
        "100",
        "100",
    ),
];

/// Connexion SQL Server (authentification SQL, `collectors/mssql`).
pub(super) const MSSQL_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "SQL Server login",
    help: "A SQL Server authentication login, not a Windows account. Sent in the TDS login packet over TLS.",
    fields: &[
        cred_text("username", "Login", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

/// Compte de la base, envoyé en basic à ORDS (`collectors/oracle`).
pub(super) const ORACLE_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "The database account enabled for REST in ORDS, sent as HTTP basic authentication.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

pub(super) const ELASTICSEARCH_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "A user holding only the cluster privilege monitor, sent as HTTP basic authentication.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

pub(super) const ELASTICSEARCH_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "Bearer token",
    help: "A service account token, sent as a bearer token. Elasticsearch API keys use another scheme and are not supported.",
    fields: &[cred_secret("token", "Token", "Stored encrypted, never shown again.", "", true)],
};

pub(super) const INFLUXDB_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "API token",
    help: "Sent as a bearer token. InfluxDB 3 needs it on /health; InfluxDB 2 does not.",
    fields: &[cred_secret("token", "Token", "Stored encrypted, never shown again.", "", true)],
};

pub(super) const KAFKA_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "SASL/PLAIN",
    help: "For a SASL_SSL listener only: the password is accepted over TLS and never sent in clear text.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

/// Options lues par `collectors/mssql/mod.rs` et `collectors/uptime/tags.rs`.
pub(super) const MSSQL_OPTIONS: &[OptionView] = &[
    socket_port("1433"),
    text(
        "database",
        "Database",
        "The database the session opens. master exists on every instance.",
        "master",
        "master",
    ),
    boolean(
        "tls",
        "Encrypt the connection",
        "Untick only for an old server that cannot encrypt: the session is then sent in clear text, the login excepted.",
        true,
    ),
    insecure_tls(
        "A default installation presents a self-signed certificate: enable this if the connection is refused for that reason.",
    ),
    SOCKET_TIMEOUT,
];

/// Options lues par `collectors/oracle/mod.rs` et `observability/options.rs`.
pub(super) const ORACLE_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "ORDS listens over plain HTTP unless you configured TLS for it (usually on port 8443).",
        "http",
        &["http", "https"],
    ),
    observability_port("8080"),
    text(
        "ords_path",
        "ORDS path prefix",
        "The context path ORDS is served under, /ords by default.",
        "/ords",
        "/ords",
    ),
    text(
        "schema",
        "Schema alias",
        "The REST alias of the schema the account is enabled for. Empty: the account name in lower case.",
        "dumbmonit",
        "",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// Options lues par `collectors/elasticsearch/mod.rs` et `observability/options.rs`.
pub(super) const ELASTICSEARCH_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "Elasticsearch 8 and later, and the OpenSearch security plugin, answer over HTTPS by default.",
        "http",
        &["http", "https"],
    ),
    observability_port("9200"),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// Options lues par `collectors/influxdb/mod.rs` et `observability/options.rs`.
pub(super) const INFLUXDB_OPTIONS: &[OptionView] =
    &[OBSERVABILITY_SCHEME, observability_port("8086"), OBSERVABILITY_TLS, OBSERVABILITY_TIMEOUT];

/// Options lues par `collectors/kafka/mod.rs` et `collectors/socket.rs`.
pub(super) const KAFKA_OPTIONS: &[OptionView] = &[
    socket_port("9092"),
    SOCKET_TLS,
    SOCKET_INSECURE_TLS,
    SOCKET_TIMEOUT,
    number(
        "max_groups",
        "Consumer groups measured",
        "The lag is measured for this many groups, in name order (0 to 500, 0 to skip the lag). The cluster health is always read.",
        "50",
        "50",
    ),
];

/// Options lues par `collectors/crowdsec/mod.rs` et `observability/options.rs`.
pub(super) const CROWDSEC_OPTIONS: &[OptionView] = &[
    OBSERVABILITY_SCHEME,
    number("port", "Metrics port", "The prometheus listen_port of CrowdSec.", "6060", "6060"),
    boolean(
        "lapi",
        "Check the Local API",
        "Checks /health of the Local API on this host. Untick for a host that only runs an agent.",
        true,
    ),
    number(
        "lapi_port",
        "Local API port",
        "The port of listen_uri in CrowdSec's api.server section.",
        "8080",
        "8080",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// Le jeton du compte de service `dumbmonit` (`collectors/kubernetes`).
pub(super) const KUBERNETES_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "Service account token",
    help: "The token of the dumbmonit service account, read from its dumbmonit-token Secret.",
    fields: &[cred_secret(
        "token",
        "Service account token",
        "Printed by the first kubectl get secret command of step 2. Stored encrypted, never shown again.",
        "eyJhbGciOi…",
        true,
    )],
};

/// Options lues par `collectors/kubernetes/mod.rs`.
pub(super) const KUBERNETES_OPTIONS: &[OptionView] = &[
    number(
        "port",
        "Port",
        "Used if the address does not give a port. k3s, kubeadm and most distributions serve the API on 6443.",
        "6443",
        "6443",
    ),
    text(
        "ca_cert",
        "Cluster CA certificate",
        "The base64 value of ca.crt printed by the second kubectl get secret command (a PEM certificate works too). Only this CA is then trusted.",
        "LS0tLS1CRUdJTiBDRVJUSUZJQ0FURS0tLS0t…",
        "",
    ),
    insecure_tls(
        "Skips every certificate check. Prefer the cluster CA above; use this only on a network you trust.",
    ),
    OBSERVABILITY_TIMEOUT,
    text(
        "exclude_namespaces",
        "Ignored namespaces",
        "Comma-separated namespaces whose pods, workloads, volume claims and events are not watched, for example a CI namespace full of short-lived pods.",
        "ci, sandbox",
        "",
    ),
];

/// Le compte de service en lecture seule (`collectors/activedirectory`).
pub(super) const ACTIVE_DIRECTORY_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Service account",
    help: "A plain domain user created for DumbMonit, member of no group.",
    fields: &[
        cred_text(
            "username",
            "User name",
            "The user principal name (svc-dumbmonit@corp.example.com), DOMAIN\\svc-dumbmonit, or the full DN.",
            "svc-dumbmonit@corp.example.com",
        ),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

/// Options lues par `collectors/activedirectory/options.rs`.
pub(super) const ACTIVE_DIRECTORY_OPTIONS: &[OptionView] = &[
    select(
        "security",
        "Connection security",
        "ldaps (port 636) encrypts from the first byte; starttls upgrades port 389 before logging in; plain sends the password in clear text and needs Allow plain LDAP.",
        "ldaps",
        &["ldaps", "starttls", "plain"],
    ),
    number(
        "port",
        "Port",
        "Used if the address does not give a port: 636 for LDAPS, 389 otherwise.",
        "636",
        "",
    ),
    text(
        "ca_cert",
        "CA certificate",
        "The PEM certificate (Base-64 encoded X.509) of the authority that issued the domain controllers' certificates. Only that authority is then trusted.",
        "-----BEGIN CERTIFICATE-----…",
        "",
    ),
    insecure_tls(
        "Skips every certificate check: the connection stays encrypted, but anyone on the path could pose as the domain controller. Prefer the CA certificate above.",
    ),
    boolean(
        "allow_plaintext",
        "Allow plain LDAP",
        "Required for the plain setting: the service account's password then crosses the network readable by anyone on the path.",
        false,
    ),
    text(
        "dns_servers",
        "DNS servers",
        "Optional. IP addresses of the DNS servers that know the domain (comma-separated). Every name lookup of this monitor then goes through them instead of the system resolver; empty keeps the system DNS.",
        "10.0.0.10, 10.0.0.11",
        "",
    ),
    boolean(
        "check_all_dcs",
        "Check every domain controller",
        "Opens a connection to the LDAP port of each domain controller of the domain, to alert when one stops answering. Names that do not resolve from DumbMonit are reported, not alerted on.",
        true,
    ),
    number(
        "stale_days_users",
        "Inactive user after (days)",
        "An enabled user with no logon for this long is counted as inactive. The last-logon date can lag by up to 14 days.",
        "90",
        "90",
    ),
    number(
        "stale_days_computers",
        "Inactive computer after (days)",
        "An enabled computer with no logon for this long is counted as inactive.",
        "90",
        "90",
    ),
    number(
        "inventory_minutes",
        "Full inventory every (minutes)",
        "Every user and computer is read in the background at this pace (1 to 1440); health is read at every check.",
        "15",
        "15",
    ),
    number(
        "request_timeout_seconds",
        "Timeout (seconds)",
        "Time allowed for each LDAP operation, from 1 to 60.",
        "8",
        "8",
    ),
];

/// Traefik sans mot de passe : le point d'entrée de l'API est filtré.
pub(super) const TRAEFIK_NO_AUTH: CredentialView = CredentialView {
    kind: "none",
    label: "No password",
    help: "The API port is filtered so that only the DumbMonit host reaches it.",
    fields: &[],
};

/// L'utilisateur d'un middleware basicAuth placé devant l'API (`collectors/traefik`).
pub(super) const TRAEFIK_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "basicAuth user",
    help: "The user of the basicAuth middleware in front of the API and the metrics.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Options lues par `collectors/traefik/mod.rs` et `observability/options.rs`.
pub(super) const TRAEFIK_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The API entry point listens over plain HTTP unless you gave it TLS.",
        "http",
        &["http", "https"],
    ),
    number(
        "port",
        "API port",
        "The port of the entry point that serves /api: 8080 for the traefik entry point.",
        "8080",
        "8080",
    ),
    boolean(
        "metrics",
        "Read the metrics",
        "Also reads /metrics on the same port, for the request rate and the share of 5xx answers. Nothing breaks if it is not served there.",
        true,
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// L'API d'administration de Caddy publiée telle quelle, filtrée.
pub(super) const CADDY_NO_AUTH: CredentialView = CredentialView {
    kind: "none",
    label: "No password",
    help: "The admin API itself, reachable only by the DumbMonit host.",
    fields: &[],
};

/// La vue en lecture seule de la notice, protégée par `basic_auth` (`collectors/caddy`).
pub(super) const CADDY_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Read-only view",
    help: "The user of the basic_auth block of the read-only view (recommended).",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Options lues par `collectors/caddy/mod.rs` et `observability/options.rs`.
pub(super) const CADDY_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The admin API and the read-only view listen over plain HTTP unless you gave them TLS.",
        "http",
        &["http", "https"],
    ),
    number(
        "port",
        "Port",
        "Used if the address does not give a port: 2019 for the admin API itself.",
        "2019",
        "2019",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// Un exporter Prometheus sans mot de passe, ou derrière un proxy qui en pose un.
pub(super) const PROMETHEUS_NO_AUTH: CredentialView = CredentialView {
    kind: "none",
    label: "No password",
    help: "Most exporters answer without authentication on a filtered network.",
    fields: &[],
};

pub(super) const PROMETHEUS_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Basic auth",
    help: "For an exporter behind basic authentication (web.config.file, or a reverse proxy).",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "", "", true),
    ],
};

pub(super) const PROMETHEUS_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "Bearer token",
    help: "Sent as \"Authorization: Bearer …\" on every read.",
    fields: &[cred_secret("token", "Token", "Stored encrypted, never shown again.", "", true)],
};

/// Options lues par `collectors/prometheus/mod.rs` et `observability/options.rs`.
pub(super) const PROMETHEUS_OPTIONS: &[OptionView] = &[
    select(
        "preset",
        "Exporter",
        "Fills in the usual port. Pick custom for any other exporter and give its port.",
        "custom",
        &[
            "custom",
            "node_exporter",
            "windows_exporter",
            "cadvisor",
            "blackbox_exporter",
            "snmp_exporter",
            "smartctl_exporter",
            "mysqld_exporter",
            "postgres_exporter",
            "redis_exporter",
            "nginx_exporter",
            "prometheus",
        ],
    ),
    select(
        "scheme",
        "Protocol",
        "HTTPS if the exporter is behind TLS.",
        "http",
        &["http", "https"],
    ),
    number(
        "port",
        "Port",
        "Used if the address does not give a port. Empty: the port of the exporter chosen above (9100 for custom).",
        "9100",
        "",
    ),
    text(
        "path",
        "Path",
        "Page to read, with its query if any: /probe?target=example.org&module=http_2xx for the blackbox exporter.",
        "/metrics",
        "/metrics",
    ),
    text(
        "keep",
        "Keep only (regular expression)",
        "Only the metrics whose name matches are stored, for example ^(node_cpu|node_memory). Empty: all.",
        "^node_(cpu|memory|filesystem)",
        "",
    ),
    text(
        "drop",
        "Ignore (regular expression)",
        "The metrics whose name matches are not stored, for example ^go_|_bucket$.",
        "^go_",
        "",
    ),
    number(
        "max_series",
        "Series limit",
        "At most this many series are stored per read, from 1 to 50000. The surplus is counted in dumbmonit_scrape_series_dropped.",
        "5000",
        "5000",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// `stub_status` et `mod_status` are not guarded by a password on a correctly
/// filtered network : the usual setup is no credential at all.
pub(super) const WEB_SERVER_NO_AUTH: CredentialView = CredentialView {
    kind: "none",
    label: "No password",
    help: "The status page is filtered so that only the DumbMonit host reaches it.",
    fields: &[],
};

/// Un compte protégeant la page de statut par `basic_auth` (Nginx ou Apache).
pub(super) const WEB_SERVER_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Basic auth",
    help: "If the status page sits behind a basic auth block or directive.",
    fields: &[
        cred_text("username", "User name", "", "dumbmonit"),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Options lues par `collectors/nginx/mod.rs` et `observability/options.rs`.
pub(super) const NGINX_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The status page listens over plain HTTP unless Nginx gave it TLS.",
        "http",
        &["http", "https"],
    ),
    number("port", "Port", "Used if the address does not give a port.", "80", "80"),
    text(
        "status_path",
        "Status path",
        "Location where \"stub_status;\" was declared.",
        "/basic_status",
        "/basic_status",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
    boolean(
        "plus_api",
        "Read the NGINX Plus API",
        "In addition to stub_status, read upstream health and server zone counters from NGINX Plus's API. No effect on the open-source build, which does not serve it.",
        false,
    ),
    number(
        "plus_api_version",
        "NGINX Plus API version",
        "Path segment of the API (\"/api/<version>/...\").",
        "9",
        "9",
    ),
];

/// Options lues par `collectors/apache/mod.rs` et `observability/options.rs`.
pub(super) const APACHE_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The status page listens over plain HTTP unless Apache gave it TLS.",
        "http",
        &["http", "https"],
    ),
    number("port", "Port", "Used if the address does not give a port.", "80", "80"),
    text(
        "status_path",
        "Status path",
        "Location where \"SetHandler server-status\" was declared, without the \"?auto\" query string: it is added automatically.",
        "/server-status",
        "/server-status",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];

/// Un utilisateur de Nginx Proxy Manager, qui se connecte par son adresse
/// électronique (`collectors/npm`).
pub(super) const NPM_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "User email and password",
    help: "A user of its own, allowed to view the hosts and nothing else.",
    fields: &[
        cred_text("username", "Email", "", "dumbmonit@example.com"),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Options lues par `collectors/npm/mod.rs` et `observability/options.rs`.
pub(super) const NPM_OPTIONS: &[OptionView] = &[
    select(
        "scheme",
        "Protocol",
        "The web interface listens over plain HTTP on port 81 unless you put it behind a proxy with TLS.",
        "http",
        &["http", "https"],
    ),
    number("port", "Web interface port", "Used if the address does not give a port.", "81", "81"),
    boolean(
        "certificates",
        "Check the certificates",
        "Reads the certificate each host presents with a TLS handshake on the HTTPS port below.",
        true,
    ),
    number(
        "tls_port",
        "HTTPS port",
        "The port on which Nginx Proxy Manager serves the hosts over HTTPS.",
        "443",
        "443",
    ),
    OBSERVABILITY_TLS,
    OBSERVABILITY_TIMEOUT,
];
