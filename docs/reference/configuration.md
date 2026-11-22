# Configuration

Everything goes through environment variables; none is required. There is no
configuration file to mount.

## Server

| Variable | Default | Role |
|---|---|---|
| `DUMBMONIT_BIND` | `0.0.0.0:8080` | Listen address of the API and the web UI, inside the container. The port only speaks HTTPS: a plain HTTP request gets a redirect to `https://`. There is no switch to turn HTTPS off. |
| `PUID` / `PGID` | `65532` | User and group the server runs as when the container starts as root (`user: "0:0"`, as in the shipped `docker-compose.yml`): the data directory is handed to them at startup, then root is dropped. `0` is refused. `DUMBMONIT_PUID` / `DUMBMONIT_PGID` take precedence. Ignored otherwise. See [Permissions](../install/docker.md#permissions). |
| `DUMBMONIT_DATA_DIR` | `/data` | Persistent directory: `dumbmonit.db` (SQLite), `secret.key` and `vm/` (embedded VictoriaMetrics). |
| `DUMBMONIT_VM_URL` | *(unset)* | Base URL of an external VictoriaMetrics (`http://host:8428`). When set, the embedded one is not started; see [below](#embedded-victoriametrics) and [External VictoriaMetrics](external-victoriametrics.md). |
| `DUMBMONIT_SECRET` | *(generated)* | Instance secret. If empty, read from `<data dir>/secret.key`, generated on first start. Encrypts monitor credentials, channel secrets and agent tokens with AES-256-GCM. |
| `DUMBMONIT_MAX_CONCURRENT_PROBES` | `64` | Maximum simultaneous probes, all collectors combined. |
| `DUMBMONIT_WEBSITE_MAX_CONCURRENT` | `10` | Maximum simultaneous [website health](../devices/website.md#pacing-hundreds-of-sites) checks, 1 to 256, within the limit above. |
| `DUMBMONIT_WEBSITE_MAX_PER_HOST` | `1` | Maximum simultaneous website checks towards one domain (`www.example.com` and `shop.example.com` count as one), 1 to 16. |
| `DUMBMONIT_WEBSITE_HOST_SPACING_SECS` | `2` | Minimum seconds between two website checks towards one domain, 0 to 3600. A site's `host_spacing_seconds` option overrides it after that site's check. |
| `DUMBMONIT_WEBSITE_CHANGE_CHECK_MINUTES` | `360` | Default period, in minutes, of the website metadata comparison (title, description, canonical, final address, `robots.txt`), 0 to 10080; 0 compares at every check. Up/down is still checked at every polling interval. A site's `change_check_minutes` option overrides it. |
| `DUMBMONIT_PROBE_TIMEOUT_SECS` | `10` | Hard timeout of one probe. Service monitors have their own, shorter `timeout_seconds` option. |
| `DUMBMONIT_FLUSH_INTERVAL_SECS` | `5` | Period of the batched writes towards VictoriaMetrics. |
| `DUMBMONIT_FLUSH_BATCH` | `5000` | Number of buffered samples that triggers a write before the period elapses. |
| `DUMBMONIT_WORKERS` | `min(4, CPUs)` | Threads of the async runtime. The server spends its time waiting on the network: four cover a homelab, raise it only for hundreds of monitors. |
| `DUMBMONIT_DB_POOL` | `4` | Maximum open SQLite connections. Each one costs a thread and a 2 MB page cache; idle connections close after five minutes. |
| `DUMBMONIT_LOG` | `info` | Log filter, `tracing` syntax: `debug`, `warn`, `dumbmonit=debug,sqlx=warn`… |
| `DUMBMONIT_AGENT_DIR` | `/agents` | Directory of the agent binaries served under `/download/`. Empty or missing: the install command fails with `404`, the server still runs. |
| `DUMBMONIT_RESET_PASSWORD` | *(off)* | `1`, `true`, `yes` or `on`: clear the password and every session at startup. The UI then shows `/setup` again. Remove it afterwards. |
| `DUMBMONIT_AGENT_LISTEN` | `0.0.0.0:8443` | Listen address of the encrypted agent channel (TLS 1.3, client certificates). `off` closes it. The only port agents on other networks need; see [Agent channel security](../install/agent-security.md). |
| `DUMBMONIT_AGENT_PUBLIC_PORT` | listen port | Port written in the agent install commands, when a port forward or a TCP proxy publishes another number. |
| `DUMBMONIT_AGENT_PUBLIC_URL` | *(unset)* | Dedicated address of the agent channel, `https://host[:port]` (for example `https://agents.example.com:443` behind an SNI passthrough). Wins over the browser's host and `DUMBMONIT_AGENT_PUBLIC_PORT` in install commands and in the channel announcement. Checked at startup. See [Dedicated agent address](#dedicated-agent-address). |
| `DUMBMONIT_SINGLE_PORT` | *(off)* | One more listening port (`443`, or an address such as `0.0.0.0:443`) serving the web interface, the API **and** the agent channel, sorted per connection by the ALPN protocol agents announce. 8080 and 8443 stay open. Needs the agent channel and a port of its own, checked at startup. See [Single port](#single-port). |
| `DUMBMONIT_RELAY_MAX_TUNNELS` | `1024` | Connections of site agents a relay may carry at once, from 1 to 4096: its separate tunnels and the streams of its multiplexed connection together. Beyond it, a new one is refused and retried by the agent. |
| `DUMBMONIT_AGENT_LEGACY_DAYS` | *(unset)* | Days during which the old plain agent channel stays accepted after an update of an instance that already had agents. Unset: until an administrator turns on *Require the encrypted channel*. |
| `DUMBMONIT_TLS_CERT` | *(unset)* | PEM certificate chain to serve; see [HTTPS and encryption](../install/https.md). Unset: a self-signed certificate is created at first start, its key stored encrypted. |
| `DUMBMONIT_TLS_KEY` | *(unset)* | PEM private key (PKCS#8, PKCS#1 or SEC1) of `DUMBMONIT_TLS_CERT`. Both files are re-read within a minute of a change, or on `SIGHUP`. |
| `DUMBMONIT_TLS_NAMES` | *(empty)* | Extra DNS names and IP addresses of the self-signed certificate, comma-separated (`nas.lan,192.168.1.10`). |
| `DUMBMONIT_TRUSTED_PROXIES` | *(empty)* | Reverse proxies whose `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` are believed, as addresses or CIDR ranges separated by commas (`10.0.0.5, 172.16.0.0/12`). The login rate limiter and the security log then see the real client address instead of the proxy's; without it, every visitor behind the proxy shares one bucket. Status page links (RSS feed, subscription emails) and the single sign-on return address also use the forwarded host only from these proxies. The headers are ignored from any other address, so a client cannot pick its own bucket or the address of a link. |
| `DUMBMONIT_SCAN_SUBNETS` | *(empty)* | Networks offered by the monitor form's network scan, as IPv4 CIDR ranges separated by commas (`192.168.1.0/24, 10.0.5.0/24`). In Docker bridge mode the server only sees the Docker network, not your LAN, so set this to pre-fill the scan with the right one. Unset: the scan suggests the /24 networks of the monitors you already monitor, then the server's own networks (only when it does not run in a container), and otherwise leaves the field empty. |
| `DUMBMONIT_OIDC_AUTO_CREATE` | *(off)* | `true` to create an account for an unknown identity at its first single sign-on. Off, only existing accounts (already linked, or matched by a provider-verified email) can sign in. Like the other `DUMBMONIT_OIDC_*` variables, read only while no single sign-on settings are saved in the interface. |
| `DUMBMONIT_OIDC_ALLOW_HTTP` | *(off)* | `1` to accept an OpenID Connect issuer in plain `http://`. Off, the settings screen refuses anything but `https://`: a clear-text issuer hands the authorization code and client secret to whoever listens on the network. Only for a test provider on the loopback. |
| `DUMBMONIT_SETUP_CODE` | *(generated)* | Code `/setup` asks for while no administrator exists. Unset, a random one is printed in the server log at each start; set it for an automated deployment. See [Install with Docker](../install/docker.md). |
| `DUMBMONIT_PUBLIC_URL` | *(unset)* | Address browsers reach DumbMonit at (`https://monit.example.org`). Used for links in notifications and status-page emails, the single sign-on redirect URI and the contact given to push services (HTTPS only). A value saved in the interface (notification policy, SSO settings) wins. |
| `DUMBMONIT_API_CORS_ORIGINS` | *(any origin)* | Origins allowed to call the API cross-origin with a bearer token, and to reach the MCP endpoint: a comma-separated list, or `off` for none. See [HTTP API](api.md#cross-origin-requests-cors). |
| `DUMBMONIT_OIDC_ISSUER` | *(unset)* | Issuer URL of the OpenID Connect provider; discovery is read from `{issuer}/.well-known/openid-configuration`. Must be `https://` unless `DUMBMONIT_OIDC_ALLOW_HTTP` is set. |
| `DUMBMONIT_OIDC_CLIENT_ID` | *(unset)* | Client id registered with the provider. |
| `DUMBMONIT_OIDC_CLIENT_SECRET` | *(unset)* | Client secret. |
| `DUMBMONIT_OIDC_PROVIDER_NAME` | `SSO` | Label of the "Continue with …" button. |
| `DUMBMONIT_OIDC_SCOPES` | `openid profile email` | Scopes requested, separated by spaces. |
| `DUMBMONIT_OIDC_GROUPS_CLAIM` | `groups` | Token claim listing the user's groups. |
| `DUMBMONIT_OIDC_ADMIN_GROUPS` | *(empty)* | Comma-separated groups whose members become administrators. |
| `DUMBMONIT_OIDC_OPERATOR_GROUPS` | *(empty)* | Comma-separated groups whose other members become operators. See [Single sign-on](../using/settings.md#single-sign-on). |
| `DUMBMONIT_LDAP_URL` | *(unset)* | LDAP / Active Directory server, `ldaps://host:636` or `ldap://host:389` (with `DUMBMONIT_LDAP_STARTTLS=1`). Like the other `DUMBMONIT_LDAP_*` variables, read only while no LDAP settings are saved in the interface. See [LDAP / Active Directory](../using/ldap.md). |
| `DUMBMONIT_LDAP_STARTTLS` | *(off)* | `1` to upgrade an `ldap://` connection with StartTLS. |
| `DUMBMONIT_LDAP_TLS_VERIFY` | `true` | `false` to accept a server certificate that cannot be verified. The connection stays encrypted, but anyone on the path could pose as the directory. |
| `DUMBMONIT_LDAP_CA_CERT` | *(unset)* | PEM of the authority that issued the server certificate; when set, only it is trusted. |
| `DUMBMONIT_LDAP_BIND_DN` | *(unset)* | Service account used to search users. Empty: anonymous search. |
| `DUMBMONIT_LDAP_BIND_PASSWORD` | *(unset)* | Its password. |
| `DUMBMONIT_LDAP_BASE_DN` | *(unset)* | Where users are searched. Required with the URL. |
| `DUMBMONIT_LDAP_USER_FILTER` | `(&(objectClass=user)(sAMAccountName={username}))` | User search filter; `{username}` is the typed username, escaped (RFC 4515). |
| `DUMBMONIT_LDAP_USERNAME_ATTRIBUTE` | `sAMAccountName` | Attribute that gives the DumbMonit username. |
| `DUMBMONIT_LDAP_DISPLAY_NAME_ATTRIBUTE` | `displayName` | Attribute that gives the display name. |
| `DUMBMONIT_LDAP_EMAIL_ATTRIBUTE` | `mail` | Display name fallback. |
| `DUMBMONIT_LDAP_GROUP_MODE` | `memberof` | `memberof` (the user's `memberOf` attribute) or `search` (a group search). |
| `DUMBMONIT_LDAP_GROUP_BASE_DN` | *(base DN)* | Where groups are searched, in `search` mode. |
| `DUMBMONIT_LDAP_GROUP_FILTER` | `(&(objectClass=groupOfNames)(member={dn}))` | Group search filter; `{dn}` is the user's DN, `{username}` the username. |
| `DUMBMONIT_LDAP_ADMIN_GROUPS` | *(empty)* | Groups (name or DN) whose members become admins, separated by `;`. |
| `DUMBMONIT_LDAP_OPERATOR_GROUPS` | *(empty)* | Groups whose other members become operators. |
| `DUMBMONIT_LDAP_VIEWER_GROUPS` | *(empty)* | Groups whose other members become viewers. Once any of the three lists is set, only members of a listed group can sign in. |
| `DUMBMONIT_LDAP_TIMEOUT_SECONDS` | `5` | Timeout of each LDAP operation, 1 to 30. |
| `DUMBMONIT_LDAP_ALLOW_PLAIN` | *(off)* | `1` to accept `ldap://` without StartTLS. Off, it is refused: every password would cross the network in clear text. Only for a test directory. |
| `DUMBMONIT_METRICS_PUBLIC` | *(off)* | `1` to serve `/metrics`, `/federate` and `/prometheus/api/v1/…` without an API token. Anyone who can reach the port then reads every measurement of every monitor and the state of the instance; only set it where something else already restricts access to the port. See [Metrics](metrics.md#scraping-dumbmonit). |
| `DUMBMONIT_ALERT_INTERVAL_SECS` | `30` | Alert evaluation period. Values below 10 are raised to 10. |
| `DUMBMONIT_ALERT_HISTORY_DAYS` | `90` | Retention of alert history, in days. |
| `DUMBMONIT_AUDIT_KEEP` | `5000` | How many entries of the security audit log (Settings → Team → Audit log) are kept, 100 to 1 000 000; the oldest are removed first. |
| `DUMBMONIT_BACKUP_ENABLED` | `1` | Scheduled local backups of the database. `0`, `false`, `no` or `off` to stop writing them. A variable that is set wins over Settings → Backup. See [Backup and restore](../install/backup.md#scheduled-local-backups). |
| `DUMBMONIT_UPDATE_CHECK` | `1` | `off` stops the daily look at the latest GitHub release (air-gapped instances). See [Update](../install/update.md). |
| `DUMBMONIT_BACKUP_DIR` | `<data dir>/backups` | Where the scheduled backups are written. Point it at a second volume to survive losing the first. |
| `DUMBMONIT_BACKUP_INTERVAL_HOURS` | `24` | Hours between two backups, 1 to 8760. The first one happens one interval after startup, not at startup. |
| `DUMBMONIT_DEMO` | *(off)* | `1` for a read-only demo with a fixed fictional estate, sign-in `demo` / `demo`. **Recreates the database at every start**: never on a real data volume. See [Run a demo instance](../install/demo.md). |
| `DUMBMONIT_BACKUP_KEEP` | `7` | How many backups are kept, 1 to 365. The oldest are removed with their `.key`. |
| `DUMBMONIT_CONFIG_DIR` | empty | A folder of YAML files applied (with prune) at startup and on every change. See [Configuration files](../using/config-files.md). |

Baseline retention (60 days) and the 14-day learning period are not
configurable.

### Checking your configuration {#checking-your-configuration}

`dumbmonit --check-config` reads the environment exactly as the server does at
startup, prints the problems it finds and the effective non-secret values (data
directory, ports, VictoriaMetrics, backup schedule, log filter…), and exits `0`
if the configuration is valid, `1` if not. It starts nothing: no database, no
network. The instance secret, passwords and URL credentials are never printed.

```sh
docker run --rm --env-file dumbmonit.env ghcr.io/selfhostr/dumbmonit:latest --check-config
```

The same list is shown to administrators in Settings → System → About.

### High availability {#high-availability}

Off unless `DUMBMONIT_HA_DIR` is set. See
[High availability](../install/high-availability.md).

| Variable | Default | Meaning |
|---|---|---|
| `DUMBMONIT_HA_DIR` | *(unset: off)* | Directory shared by the active server and the standby (lease, database replicas), absolute path in the container. |
| `DUMBMONIT_HA_NODE` | *(required with the directory)* | Name of this server, different on each: 1 to 32 lowercase letters, digits, `-` or `_`. |
| `DUMBMONIT_HA_ROLE` | *(required with the directory)* | `primary` or `standby`. |
| `DUMBMONIT_HA_TAKEOVER_MINUTES` | *(unset: manual)* | Automatic takeover after this many minutes without a heartbeat, 2 at least. |
| `DUMBMONIT_HA_SNAPSHOT_SECS` | `30` | Database replication period, 10 to 3600 seconds. |

### Email server {#email-server}

The SMTP server of the instance, which mails status page subscribers and the
scheduled reports that have no email channel. It is normally set in
**Settings → Notifications → Email server** (see
[Email server](../notifications.md#email-server)). When `DUMBMONIT_SMTP_HOST`
is set, these variables describe it instead: they win over the saved setting,
which the interface then shows read-only.

| Variable | Default | Meaning |
|---|---|---|
| `DUMBMONIT_SMTP_HOST` | *(unset: use the setting)* | Host name or address of the SMTP server. |
| `DUMBMONIT_SMTP_PORT` | from the encryption | `587` for STARTTLS, `465` for TLS, `25` for none. |
| `DUMBMONIT_SMTP_SECURITY` | `starttls` | `starttls`, `tls` (encrypted from the first byte) or `none` (only towards a relay on your own network). |
| `DUMBMONIT_SMTP_USERNAME` | *(empty: no authentication)* | Username. |
| `DUMBMONIT_SMTP_PASSWORD` | *(empty)* | Password, used with the username. |
| `DUMBMONIT_SMTP_FROM` | *(required with the host)* | Sender address, `status@example.com`. |
| `DUMBMONIT_SMTP_FROM_NAME` | *(empty)* | Sender name shown by mail clients. |

### Former `EZYMONIT_*` names

DumbMonit was called EzyMonit until September 2026. Every `DUMBMONIT_*`
variable is also read under its old `EZYMONIT_*` name, as a fallback, with one
warning at startup per variable still set that way (`EZYMONIT_X is deprecated,
use DUMBMONIT_X`). The old database file `/data/ezymonit.db` is renamed to
`dumbmonit.db` at startup, and agent tokens `ezym_…` remain valid (new enrolment
secrets start with `dmon_`). The session cookie is now `__Host-dumbmonit_session`:
the first start after the rename logs everyone out once.

### Embedded VictoriaMetrics

The image ships the VictoriaMetrics binary. Unless `DUMBMONIT_VM_URL` is set,
the server starts it as a child process at startup, forwards its log lines into
the server log, restarts it with a backoff if it dies, and stops it at shutdown
(`SIGTERM`, 20 s of grace, then kill; the Compose file's `stop_grace_period`
leaves room for that).

| Variable | Default | Role |
|---|---|---|
| `DUMBMONIT_VM_BINARY` | `/victoria-metrics-prod` | Path of the VictoriaMetrics binary to start. |
| `DUMBMONIT_VM_LISTEN` | `127.0.0.1:8428` | Listen address, inside the container. Loopback by default: only the server talks to it. The development overlay sets `0.0.0.0:8428` and publishes the port. |
| `DUMBMONIT_VM_RETENTION` | `12` | Retention, in the syntax of `-retentionPeriod`: a number of months, or a duration such as `30d` or `2y`. |
| `DUMBMONIT_VM_MEMORY` | `256MB` | Budget for VictoriaMetrics' caches (`-memory.allowedBytes`). Without a budget VM sizes them for 60 % of the host's RAM and its resident memory grows for days. A few dozen monitors fit in 256 MB; raise it for hundreds of hosts. |

Its data lives in `<data dir>/vm`, on the same volume as the database. Three
flags are fixed by the server and not configurable:
`-search.maxUniqueTimeseries=100000` (a runaway query cannot exhaust the
budget), `-search.maxConcurrentRequests=4` (one UI plus the alert engine) and
`-dedup.minScrapeInterval=10s` (duplicate points closer than ten seconds are
collapsed; nothing is lost at the 30 s cadence of the collectors).

The raw retention can also be changed in **Settings → System → Data &
retention**; a value saved there wins over `DUMBMONIT_VM_RETENTION`.

With `DUMBMONIT_VM_URL` set, none of this applies: the server uses the
instance you name and the embedded one is never started. `GET /api/health`
says which case you are in (`victoria.embedded`).

### Long-term history

Besides the raw data, the server writes **5-minute aggregates** into a second
VictoriaMetrics, started the same way, and answers queries whose range goes past
the raw retention from it. See [Sizing](sizing.md) for what it costs.

| Variable | Default | Role |
|---|---|---|
| `DUMBMONIT_VM_LONG_URL` | *(unset)* | URL of an external VictoriaMetrics for the aggregates. When set, the embedded long-term store is not started. Needed to have a long-term history next to an external `DUMBMONIT_VM_URL`. |
| `DUMBMONIT_VM_LONG_LISTEN` | `127.0.0.1:8429` | Listen address of the embedded long-term store, inside the container. |
| `DUMBMONIT_VM_LONG_MEMORY` | `64MB` | Memory budget of its caches (`-memory.allowedBytes`). |
| `DUMBMONIT_VM_LONG_RETENTION` | `730d` | Retention of the aggregates, same syntax as `DUMBMONIT_VM_RETENTION`. Also set in **Settings → System**. |

Its data lives in `<data dir>/vm-long`. It runs the same VictoriaMetrics binary
(`DUMBMONIT_VM_BINARY`). Metric names are kept (average), with the suffixes
`:5m_min`, `:5m_max` and, for availability metrics, `:5m_sum` and `:5m_count`.

### Embedded VictoriaLogs

Optional and **off by default**: turn it on in **Settings → System**. The server
then starts VictoriaLogs, sends its own log lines to it, and the admin-only
**Logs** page searches them with LogsQL.

| Variable | Default | Role |
|---|---|---|
| `DUMBMONIT_VLOGS_BINARY` | `/victoria-logs-prod` | Path of the VictoriaLogs binary to start. |
| `DUMBMONIT_VLOGS_LISTEN` | `127.0.0.1:9428` | Listen address, inside the container. |
| `DUMBMONIT_VLOGS_MEMORY` | `64MB` | Memory budget of its caches. |

Its data lives in `<data dir>/vlogs`; the retention is set in **Settings →
System** (30 days by default).

### Central logs: syslog server and SNMP traps

Both receivers write into the embedded VictoriaLogs (turn the log store on
first) and are **off by default**. They listen on unprivileged ports, since the
container does not run as root; publish the ports you use in
`docker-compose.yml` (the lines are commented there). See
[Logs](../using/logs.md).

| Variable | Default | Role |
|---|---|---|
| `DUMBMONIT_SYSLOG_ENABLED` | `false` | Start the syslog server. |
| `DUMBMONIT_SYSLOG_PORT` | `5514` | UDP and TCP port. |
| `DUMBMONIT_SYSLOG_BIND` | `0.0.0.0` | Listen address. |
| `DUMBMONIT_SYSLOG_UDP` / `DUMBMONIT_SYSLOG_TCP` | `true` | Enable each transport. |
| `DUMBMONIT_TRAPS_ENABLED` | `false` | Start the SNMP trap receiver (v1, v2c). |
| `DUMBMONIT_TRAPS_PORT` | `1162` | UDP port. |
| `DUMBMONIT_TRAPS_BIND` | `0.0.0.0` | Listen address. |
| `DUMBMONIT_TRAPS_COMMUNITIES` | empty | Comma-separated communities to accept; empty accepts any. |

On the machine of an agent, `DUMBMONIT_AGENT_LOGS=off` forbids log collection
there whatever the server asks.

## Compose-level settings

The shipped `docker-compose.yml` reads no variable and needs no `.env` file.
Everything is set under `environment:` of the service or by editing the file;
see [Advanced options](../install/docker.md#advanced-options) (port, secret,
retention and memory, external VictoriaMetrics, password reset).

## Ports

| Port | Container | Purpose |
|---|---|---|
| `5514/udp`, `5514/tcp` | `dumbmonit` | Syslog server, only when `DUMBMONIT_SYSLOG_ENABLED=true`. Commented out in the Compose file. |
| `1162/udp` | `dumbmonit` | SNMP trap receiver, only when `DUMBMONIT_TRAPS_ENABLED=true`. Commented out in the Compose file. |
| `8080/tcp` | `dumbmonit` | Web UI, API, agent installers and downloads, public status pages. HTTPS only. |
| `8443/tcp` | `dumbmonit` | Encrypted agent channel (TLS 1.3, client certificates): agent ingest, commands, relay tunnels. Publish it when agents run on other networks. |
| `443/tcp` | `dumbmonit` | Optional single port (`DUMBMONIT_SINGLE_PORT`): web UI, API and agent channel on one port. Commented out in the Compose file. |
| `8428/tcp`, `8429/tcp`, `9428/tcp` | `dumbmonit` | Embedded VictoriaMetrics, long-term store and VictoriaLogs, on the container's loopback. Never published. |

See [Network ports](../install/ports.md) for who must reach what, or **Settings → System → Network ports** for the list computed from your own configuration.

Outbound: SNMP (UDP 161 by default) towards monitors, HTTPS towards Proxmox,
PBS, Synology and notification services, plus whatever your service monitors
target. ICMP ping needs the `net.ipv4.ping_group_range` sysctl set in the
Compose file (no capability).

### Dedicated agent address and SNI passthrough {#dedicated-agent-address}

`DUMBMONIT_AGENT_PUBLIC_URL=https://agents.example.com:443` gives the agents an
address of their own. It wins over the host the browser opened and over
`DUMBMONIT_AGENT_PUBLIC_PORT` (still supported) in the install commands
(`--tls-url`, and `--url` too for a relay or with `agent_port_only=true`), and
in the channel announcement agents receive at enrolment and with every reply:
an agent installed without `--tls-url` switches to it by itself. Only
`https://host[:port]` is accepted, checked at startup: `http://`, a path, a user
name or a closed agent channel (`DUMBMONIT_AGENT_LISTEN=off`) stop the server
with a message.

The usual case: one public IP whose port 443 already serves websites. A TCP
proxy reads the server name (SNI) of the TLS ClientHello and passes the
connection, still encrypted, to port 8443 of DumbMonit. **Never terminate TLS
there**: the agent's client certificate and the pinned authority must reach
DumbMonit untouched.

HAProxy (`web` stands for whatever already serves 443):

```haproxy
frontend https_in
    bind :443
    mode tcp
    tcp-request inspect-delay 5s
    tcp-request content accept if { req.ssl_hello_type 1 }
    use_backend dumbmonit_agents if { req.ssl_sni -i agents.example.com }
    default_backend web

backend dumbmonit_agents
    mode tcp
    server dumbmonit 192.168.1.5:8443

backend web
    mode tcp
    server proxy 127.0.0.1:8444
```

Traefik v3 (file provider; the entry point `websecure` listens on 443):

```yaml
tcp:
  routers:
    dumbmonit-agents:
      entryPoints: [websecure]
      rule: "HostSNI(`agents.example.com`)"
      service: dumbmonit-agents
      tls:
        passthrough: true
  services:
    dumbmonit-agents:
      loadBalancer:
        servers:
          - address: "192.168.1.5:8443"
```

- Use a DNS name in the URL: the name travels in the SNI, an IP does not.
- The server sees the proxy's address: failed enrolment attempts are counted
  per address, so every agent behind the proxy shares one counter.
- Check from outside: `curl -k https://agents.example.com/install.sh` returns
  the install script (the agent port serves it without a client certificate).
- Cloudflare Tunnel, or any proxy that terminates TLS, cannot carry the agent
  channel.

### Single port {#single-port}

`DUMBMONIT_SINGLE_PORT=443` opens one more port that serves everything. Each
connection is sorted on its TLS ClientHello, before any certificate is chosen:

| The client announces | It gets |
|---|---|
| ALPN `dumbmonit-agent/1` (every up-to-date agent and relay) | The agent channel: internal authority, client certificate, the same routes and checks as 8443. The agent then speaks HTTP/2 when the server offers it. |
| Anything else (browser, `curl`, PowerShell, TLS 1.2 included) | The web interface and its certificate (provided or self-signed), as on 8080. |
| Plain HTTP | The redirect to `https://` of 8080, nothing else. |

- **Backward compatible.** 8080 and 8443 keep listening. An agent older than
  this option does not announce the protocol: it would get the web
  certificate, which it does not trust, so it keeps using 8443. Update the
  agents (from the interface) before you close 8443. A new agent talking to an
  older server just gets HTTP/1.1, the server ignores the protocol name.
- **Install commands.** Open the interface through the single port (for
  example `https://monit.example.com`, 443 implied) before you copy a command:
  `--url` and `--tls-url` then both point at it. The script and the binaries
  are read there under the web certificate, so the command pins that one
  (`--pinned-key` for a self-signed certificate) and the authority of the agent
  channel (`--ca-pin`), as usual.
- **Port forward.** If the container listens on another number than the one
  agents dial (`DUMBMONIT_SINGLE_PORT=8444`, published as `443:8444`), set
  `DUMBMONIT_AGENT_PUBLIC_URL=https://monit.example.com:443`.
- The value is a port (`443`, all IPv4 addresses) or an address (`[::]:443`).
  It must differ from `DUMBMONIT_BIND` and `DUMBMONIT_AGENT_LISTEN`, and the
  agent channel must be open: otherwise the server does not start.
- A proxy that terminates TLS still cannot carry the agent channel; an SNI
  passthrough can.

```yaml
    environment:
      DUMBMONIT_SINGLE_PORT: "443"
    ports:
      - "443:443"
    sysctls:
      net.ipv4.ping_group_range: "0 2147483647"
      net.ipv4.ip_unprivileged_port_start: "443"
```

## Volumes and files

One volume, `dumbmonit-data`, mounted on `/data`. Its name is fixed in the
Compose file, whatever the project is called.

| Path (in the container) | Volume | Contents |
|---|---|---|
| `/data/dumbmonit.db` | `dumbmonit-data` | Monitors, credentials (encrypted), rules, alert state and history, baselines, silences, channels, agent tokens, password hash, sessions. |
| `/data/secret.key` | `dumbmonit-data` | The instance secret. **Back it up.** Without it, encrypted credentials are unrecoverable and the server refuses to start. |
| `/data/backups/` | `dumbmonit-data` | Scheduled local backups: `dumbmonit-<timestamp>.db` (an online SQLite copy) and the matching `.key`. Daily by default, seven kept. |
| `/data/vm/` | `dumbmonit-data` | Embedded VictoriaMetrics time series, 12 months by default. Empty when `DUMBMONIT_VM_URL` is set. |
| `/data/vm-long/` | `dumbmonit-data` | 5-minute aggregates (long-term history), 730 days by default. Empty when `DUMBMONIT_VM_LONG_URL` is set. |
| `/data/vlogs/` | `dumbmonit-data` | VictoriaLogs data, only when the log store is enabled. |
| `/agents/` | image | Agent binaries. Mount another directory and set `DUMBMONIT_AGENT_DIR` to ship your own. |
| `/etc/ssl/certs/ca-certificates.crt` | image | TLS roots for outbound HTTPS. Mount your own bundle here to trust a private authority. |

## Resetting the password

```bash
# add DUMBMONIT_RESET_PASSWORD: "1" under environment: in docker-compose.yml
docker compose up -d
# open the UI: /setup asks for a new password
# remove the line again, then:
docker compose up -d
```

The reset clears the password and every session; monitors, rules and channels
are untouched.

## Agent

The agent has its own variables and configuration file: see
[Linux, macOS, FreeBSD and Windows agent](../devices/agent.md#configuration-file).

### Collection scope

What the agent measures, and therefore how many series one machine produces.
The defaults keep a host with dozens of containers to a few hundred series;
each key is a `agent.yaml` entry, overridable by its environment variable.

| Key | Variable | Default | Role |
|---|---|---|---|
| `interfaces_ignore` | `DUMBMONIT_AGENT_INTERFACES_IGNORE` | `^(veth\|br-\|docker\|virbr\|lo$\|vEthernet)` | Network interfaces left out: container and VM interfaces, the loopback. A list of exact names or regular expressions (unanchored; anything with a metacharacter is a regex). The value replaces the default; an empty list keeps every interface. |
| `interfaces_only` | `DUMBMONIT_AGENT_INTERFACES_ONLY` | *(empty)* | Allow list, same syntax. When set, only these interfaces are reported and `interfaces_ignore` is not consulted. |
| `mounts_ignore` | `DUMBMONIT_AGENT_MOUNTS_IGNORE` | `^/(var/lib/docker/\|run/(user\|docker\|containerd\|snapd\|credentials\|systemd\|lock\|udev\|netns\|lxc\|lxd)/\|sys/\|proc/\|dev/\|snap/)` | Mount points left out of filesystem usage and disk I/O, same syntax. Removable drives under `/run/media/` stay. Pseudo filesystems (`overlay`, `tmpfs`, `squashfs`, `fuse.portal`, `cgroup2`…) are always skipped, whatever the path. |
| `cpu_per_core` | `DUMBMONIT_AGENT_CPU_PER_CORE` | `false` | One `cpu_core_usage_percent` series per core, on top of the global usage. `cpu_count` is reported either way. |
| `docker_max_containers` | `DUMBMONIT_AGENT_DOCKER_MAX_CONTAINERS` | `200` | Containers detailed per cycle, running ones first. Beyond, containers are only counted (`container_count`, `container_running_count` stay exact) and `container_series_skipped` says how many. `0` counts without detailing. |

In the environment, lists are comma-separated (`^veth, ^br-, docker0`); in
YAML, either a list or one comma-separated string. Disk I/O counters
(`disk_read_bytes`, `disk_written_bytes`) carry the `device` label only: one
series per block device, however many times it is mounted.
