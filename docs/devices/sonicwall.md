# SonicWall SonicOS (community pack)

A SonicWall firewall through the SonicOS 7 API: the link of every interface bound
to a zone. Deliberately minimal: only the API paths SonicWall publishes are read.

!!! note "Community pack"
    SonicWall is no longer built into DumbMonit: it ships as the community pack
    [`packs/sonicwall`](https://github.com/selfhostr/dumbmonit/tree/main/packs/sonicwall),
    written without a device to try it on, which anyone can correct without
    waiting for a release. Install it under **Settings → Integration packs**
    (paste or upload `pack.yaml`), then add a monitor of type **SonicWall
    SonicOS**. See [integration packs](../packs/index.md).

    Existing SonicWall monitors were moved to the pack's type with their user
    name and password. Until the pack is installed they show *Install the
    community pack "sonicwall"*.

## Network ports to open

From the DumbMonit server (or its relay agent) to the device:

| Port | Protocol | Purpose |
|---|---|---|
| 443 | TCP | Management interface (HTTPS, SonicOS API) |

## Set it up

1. **Device → Settings → Administration → Audit/SonicOS API**: enable the
   SonicOS API and *RFC-2617 HTTP Basic Access authentication*.
2. **Device → Users → Local Users & Groups → Add User**: `dumbmonit`, a long
   random password, member of *SonicWall Read-Only Admins*.
3. Allow HTTPS management on the interface DumbMonit reaches the firewall
   through.
4. In DumbMonit, enter the firewall address with that user name and password.

Each poll opens an API session (`POST /api/sonicos/auth`), reads, and always
closes it (`DELETE`): SonicOS allows a single API session at a time.

## What it watches

All metrics are prefixed `dumbmonit_sonicwall_`.

| Metric | What | Labels |
|---|---|---|
| `version_info` | value 1 | `version`, `model` |
| `interface_link_up` | 1 when the interface has a link, 0 for *No link* | `interface`, `zone` |

## Alert rules

Installed with the pack, under the uid `pack:sonicwall:<name>`:

| Rule | Fires when |
|---|---|
| `interface_down` | an interface bound to a zone (not *Unassigned*) has no link for 5 minutes |
