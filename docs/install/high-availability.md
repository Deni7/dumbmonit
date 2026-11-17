# High availability (active/standby)

A second DumbMonit server can wait on another machine and take over when the
first one is gone. It is off by default: without `DUMBMONIT_HA_DIR` nothing
described here happens, and a single server stays the simplest choice for a
homelab. Turn it on only if an hour without monitoring is a real problem.

**Only one server is active at a time, always.** The standby opens neither its
database nor any port and probes nothing until it becomes active.

## What you need

- **Two machines** running the **same DumbMonit version**. Upgrade both
  together: a database written by a newer version does not start on an older
  one.
- **An external VictoriaMetrics** both servers reach (`DUMBMONIT_VM_URL`, see
  [External VictoriaMetrics](../reference/external-victoriametrics.md)), on a
  third machine or as a cluster. With the embedded one each server keeps its
  own metrics, and the history does not follow a switch-over (the server says
  so in its log).
- **A shared directory** both servers can write: an NFS or SMB export, or any
  volume both hosts mount. It holds the lease and the copies of the database
  (a few megabytes), nothing else. The server runs as uid 65532 (or `PUID`):
  give that user write access to it (`chown 65532:65532` on the export, or
  `anonuid`/`all_squash` on NFS). Keep it on reliable storage: when it is
  unreachable, nothing can change hands. Protect it like `/data`: it holds a
  copy of the database (configuration, password hashes; credentials stay
  encrypted by the instance secret, which is never written there).
- **The same instance secret on both**: either `DUMBMONIT_SECRET` with the
  same value, or a copy of `/data/secret.key` from the first server put in the
  standby's volume **before its first start**. The standby checks every copy
  of the database against its own secret and refuses to take over if it cannot
  decrypt it.
- **Agents that know both addresses**: `server_urls` in their `agent.yaml`
  (see [Backup servers](../devices/agent.md#backup-servers)).

## How it works

- **The lease.** A small file in the shared directory says which server is
  active. The active one renews it every 10 seconds, and reads it back first:
  if another server holds it, it stops at once (exit code 75) and the
  container restarts it as the standby.
- **The replica.** Every 30 seconds (`DUMBMONIT_HA_SNAPSHOT_SECS`), when the
  database changed, the active server writes a consistent copy of it
  (`VACUUM INTO`, like the [scheduled local backups](backup.md)) to the shared
  directory, under its own lease only. The standby copies each new one, checks
  its SHA-256, its integrity and that its secret decrypts it, and keeps it
  ready.
- **Taking over.** The standby becomes active when you promote it, or on its
  own after `DUMBMONIT_HA_TAKEOVER_MINUTES` without a heartbeat. It takes the
  lease, waits 15 seconds and reads it back (if two servers try at once, only
  one wins), puts its old database aside in `/data/ha/previous.db`, installs
  the latest replica and starts like any server: probes, alerts, web port and
  agent port.
- **What follows the replica.** Everything in the database: monitors, alert
  rules and states, users, channels, credentials, the **agent authority** (CA)
  and the self-signed HTTPS certificate, all encrypted by the instance secret.
  Agents therefore reach the new active server with the certificate they
  already have: no re-enrolment.
- **What can be lost.** Changes made on the old active server after its last
  replica: up to `DUMBMONIT_HA_SNAPSHOT_SECS` of configuration and alert state.
  Metrics live in VictoriaMetrics, and agents keep their measurements while no
  server answers, then send them to the new one.
- **Clean stop.** On `docker compose stop` the active server writes a last
  replica and releases the lease. The server whose role is `primary` takes a
  released lease back at once; the standby waits for its takeover delay, so a
  simple restart of the primary does not move anything.

## Settings

| Variable | Default | Meaning |
|---|---|---|
| `DUMBMONIT_HA_DIR` | *(unset: off)* | Shared directory, absolute path inside the container (`/ha`). |
| `DUMBMONIT_HA_NODE` | *(required)* | Name of this server, different on each: 1 to 32 lowercase letters, digits, `-` or `_` (`a`, `b`). |
| `DUMBMONIT_HA_ROLE` | *(required)* | `primary` on the server that normally runs, `standby` on the other. |
| `DUMBMONIT_HA_TAKEOVER_MINUTES` | *(unset: manual)* | Automatic takeover after this many minutes without a heartbeat (2 at least). The active server stops itself after half of it without a successful renewal, so it is gone before the standby takes over. |
| `DUMBMONIT_HA_SNAPSHOT_SECS` | `30` | How often the active server publishes its replica, 10 to 3600 seconds. |

## Two-node example

Host C runs VictoriaMetrics, the NAS exports `/export/dumbmonit-ha` over NFS.
On host A:

```yaml
name: dumbmonit
services:
  dumbmonit:
    image: ghcr.io/selfhostr/dumbmonit:latest
    ports:
      - "8080:8080"
      - "8443:8443"
    environment:
      DUMBMONIT_VM_URL: http://monitor-c.lan:8428
      DUMBMONIT_SECRET: ${DUMBMONIT_SECRET}   # same value on both, from .env
      DUMBMONIT_HA_DIR: /ha
      DUMBMONIT_HA_NODE: a
      DUMBMONIT_HA_ROLE: primary
      # DUMBMONIT_HA_TAKEOVER_MINUTES: "5"    # automatic takeover, optional
    volumes:
      - dumbmonit-data:/data
      - dumbmonit-ha:/ha
    restart: unless-stopped                  # required: a fenced server restarts as standby
    stop_grace_period: 30s
    read_only: true
    tmpfs: [/tmp]
    user: "0:0"
    cap_drop: [ALL]
    cap_add: [CHOWN, DAC_READ_SEARCH, SETUID, SETGID]
    security_opt: ["no-new-privileges:true"]

volumes:
  dumbmonit-data:
  dumbmonit-ha:
    driver_opts:
      type: nfs
      o: addr=nas.lan,rw,nfsvers=4.1
      device: ":/export/dumbmonit-ha"
```

On host B, the same file with `DUMBMONIT_HA_NODE: b` and
`DUMBMONIT_HA_ROLE: standby`. On host C:

```yaml
services:
  victoriametrics:
    image: victoriametrics/victoria-metrics:v1.156.0
    command: ["-storageDataPath=/storage", "-retentionPeriod=12"]
    ports:
      - "8428:8428"
    volumes:
      - vm-data:/storage
    restart: unless-stopped

volumes:
  vm-data:
```

Port 8428 must be reachable from A and B only (a firewall rule, or a private
network): VictoriaMetrics has no authentication of its own.

Start A first and wait for *this server is now the active one* in its log,
then start B: it logs *standby: waiting*. On an existing server, add the
variables to it (it becomes `primary` and keeps its data), copy its
`/data/secret.key` to B's volume, then start B.

On Kubernetes, the Helm chart (`deploy/helm/dumbmonit`) runs one server. For
this setup, run two releases, each with its own data volume, these variables
and a `ReadWriteMany` volume mounted at `DUMBMONIT_HA_DIR`.

## Agents

Add the standby to every agent:

```yaml
server_url: https://monitor-a.lan:8080
server_urls:
  - https://monitor-b.lan:8080
```

After three failed attempts in a row the agent moves to the next address, with
the same certificate. While on a backup it tries the primary every two
minutes, and goes back as soon as it answers as an active server. The machine
page shows **Server: primary**, or **Backup** with the address and since when.

## Day-to-day commands

Run them in the container (`docker compose exec dumbmonit /dumbmonit ha …`):

- `ha status`: this server's state, who holds the lease and whether it renews
  it, the last replica and any problem (secret, version, storage).
- `ha promote`: make this standby active. Refused while the active server
  still renews its lease; stop it first.
- `ha promote --force`: take over even though the active server renews its
  lease. It stops itself at its next renewal (within 10 seconds), but its
  changes since its last replica are lost. Use it when the active server is
  cut off from you but not from the shared directory.

The container health check reports a standby that is doing its job as
healthy, although no port is open.

### Planned switch-over

1. `docker compose stop` on the active server: last replica, lease released.
2. `docker compose exec dumbmonit /dumbmonit ha promote` on the other one.
3. Start the first one again: it waits as the standby. To switch back, do the
   same the other way round.

### After an outage

When the active server dies, promote the standby (or let the automatic
takeover do it). When the old server comes back, it sees that the other one
holds the lease and waits as the standby, even if its role is `primary`.

## Why not Litestream or LiteFS

Both were evaluated (November 2026) and remain usable around DumbMonit if you
prefer them; neither is needed.

- **[Litestream](https://litestream.io/)** (v0.5.18) streams the SQLite WAL to
  S3, SFTP, a file path and more, with a lower data loss window than 30
  seconds. It runs as a separate process (a sidecar sharing `/data`), takes
  over WAL checkpointing (it asks the application to disable its own) and
  leaves the one-writer-at-a-time rule to you; it has no lease and no
  promotion, a restore is `litestream restore` by hand.
- **[LiteFS](https://fly.io/docs/litefs/)** (v0.5.14, April 2025, still beta)
  replicates through a FUSE file system: it needs `/dev/fuse` and
  `CAP_SYS_ADMIN` in the container, and Consul for an automatic primary
  election.

The built-in mode needs no other process, no extra privilege and no new
dependency: one shared directory, a lease that fences the old active server,
and a copy of the database the standby checks before trusting it. Its data
loss window is the replication period; for a monitoring server, whose metrics
live elsewhere, that is the right trade.
