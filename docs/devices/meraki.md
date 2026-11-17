# Cisco Meraki (community pack)

A Cisco Meraki organization through the Dashboard API: devices offline, Internet
uplinks that failed, license out of compliance or expiring.

!!! note "Community pack"
    Meraki is no longer built into DumbMonit: it ships as the community pack
    [`packs/meraki`](https://github.com/selfhostr/dumbmonit/tree/main/packs/meraki),
    which anyone can correct without waiting for a release. Install it under
    **Settings → Integration packs** (paste or upload `pack.yaml`), then add a
    monitor of type **Cisco Meraki**. See [integration packs](../packs/index.md).

    Existing Meraki monitors were moved to the pack's type with their API key.
    Until the pack is installed they show *Install the community pack "meraki"*.
    The **Organization** option now takes the organization's numeric ID (not its
    name), and the network filter is gone.

## Network ports to open

From the DumbMonit server to the Internet:

| Port | Protocol | Purpose |
|---|---|---|
| 443 | TCP | Meraki Dashboard API (`api.meraki.com`, or the country dashboard: `api.meraki.ca`, `api.meraki.cn`, `api.meraki.in`, `api.gov-meraki.com`) |

## Set it up

1. In the Meraki dashboard, **Organization → Settings**: tick *Enable access to
   the Cisco Meraki Dashboard API*.
2. Create a read-only organization administrator for DumbMonit, sign in with it
   and generate an API key under **My profile → API access**.
3. Find the organization ID: `curl -H "Authorization: Bearer <key>" https://api.meraki.com/api/v1/organizations`.
4. In DumbMonit, enter `api.meraki.com` as the address, paste the key and the
   organization ID.

A pack only talks to the address of the monitor. If the dashboard redirects your
organization to another server, the error names it (`n123.meraki.com`): enter
that server as the address.

## What it watches

All metrics are prefixed `dumbmonit_meraki_`.

| Metric | What | Labels |
|---|---|---|
| `device_online`, `device_offline`, `device_alerting` | 1 or 0 per device; dormant devices have none | `device`, `serial`, `model`, `product_type`, `network_id` |
| `device_last_report_timestamp_seconds` | when the device last reported | same |
| `uplink_up`, `uplink_failed` | per uplink of a security appliance; `not connected` (an empty socket) is not a failure | `serial`, `network_id`, `interface` |
| `license_ok`, `license_expiration_timestamp_seconds` | co-termination licensing: status OK, expiration date | |
| `licenses_expired` | per-device licensing: licenses that expired | |

## Alert rules

Installed with the pack, under the uid `pack:meraki:<name>`:

| Rule | Fires when |
|---|---|
| `device_offline` | a device stays offline for 10 minutes |
| `uplink_failed` | a configured uplink lost Internet access for 5 minutes |
| `license_problem` | the license is out of compliance, or per-device licenses expired (1 hour) |
| `license_expiring` | the co-termination license expires in less than 30 days |
