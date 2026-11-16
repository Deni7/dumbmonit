# Palo Alto PAN-OS (community pack)

A Palo Alto Networks firewall (PA-Series, VM-Series) through the PAN-OS XML API:
interfaces without link, HA peer lost or out of sync, session table filling up,
management plane CPU and memory, licenses expiring.

!!! note "Community pack"
    PAN-OS is no longer built into DumbMonit: it ships as the community pack
    [`packs/paloalto`](https://github.com/selfhostr/dumbmonit/tree/main/packs/paloalto),
    which anyone can correct without waiting for a release. Install it under
    **Settings → Integration packs** (paste or upload `pack.yaml`), then add a
    monitor of type **Palo Alto PAN-OS**. See [integration packs](../packs/index.md).

    Existing PAN-OS monitors were moved to the pack's type with their credential.
    Until the pack is installed they show *Install the community pack
    "paloalto"*. The pack takes an **API key** only: a monitor that used a user
    name and password needs a key generated for that account (step 3 below).
    The last commit is no longer watched.

    Configuration backups of PAN-OS ([network configuration](netconfig.md)) are
    not affected.

## Network ports to open

From the DumbMonit server (or its relay agent) to the device:

| Port | Protocol | Purpose |
|---|---|---|
| 443 | TCP | Management interface (HTTPS, XML API) |

## Set it up

1. **Device → Admin Roles → Add**: a role whose XML API tab enables
   *Operational Requests* only; Web UI and REST API disabled.
2. **Device → Administrators → Add** an administrator `dumbmonit` with that
   role and a long random password, then **Commit**.
3. Generate its API key from the DumbMonit host:
   `curl -k -X POST https://firewall.lan/api/ -d type=keygen -d user=dumbmonit --data-urlencode password=<password>`
4. In DumbMonit, enter the firewall address and paste the key (between `<key>`
   and `</key>`). Turn on *Accept an unverifiable certificate* while the
   firewall keeps its factory certificate.

The key travels in the `X-PAN-KEY` header, never in the URL.

## What it watches

All metrics are prefixed `dumbmonit_paloalto_`. Only `show system info` is
required; the other reads are optional.

| Metric | What | Labels |
|---|---|---|
| `version_info` | value 1 | `version`, `model`, `system_name` |
| `cpu_idle_percent`, `memory_total_kib`, `memory_free_kib`, `memory_cache_kib` | management plane, from `top` | |
| `sessions`, `sessions_max`, `session_setup_rate`, `packets_per_second`, `throughput_kbps` | session table and traffic | |
| `ha_enabled`, `ha_local_functional`, `ha_peer_up`, `ha_peer_connected`, `ha_in_sync` | HA pair | |
| `license_expired`, `license_expiration_timestamp_seconds` | per license; *Never* has no date | `license` |
| `interface_link_up`, `interface_speed_mbps` | per physical port | `interface` |
| `interface_info` | value 1 per logical interface | `interface`, `zone` |

## Alert rules

Installed with the pack, under the uid `pack:paloalto:<name>`:

| Rule | Fires when |
|---|---|
| `interface_down` | an interface bound to a zone has no link for 5 minutes |
| `ha_peer_lost` | the HA peer is neither active nor passive, or its link is down (2 minutes) |
| `ha_out_of_sync` | the HA configuration is not synchronized for 15 minutes |
| `ha_not_functional` | this firewall is suspended or non-functional in its pair (5 minutes) |
| `session_table_full` | the session table stays above 90 % for 10 minutes |
| `cpu_high`, `memory_high` | the management plane stays above 90 % for 15 minutes |
| `license_expiring` | a license expires in less than 30 days, or has expired |
