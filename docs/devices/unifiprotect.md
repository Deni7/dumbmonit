# UniFi Protect (community pack)

The cameras of a UniFi Protect console, through the integration API of UniFi OS:
which ones are disconnected. Not to be confused with the UniFi Network monitor
(access points, switches, gateway).

!!! note "Community pack"
    UniFi Protect is no longer built into DumbMonit: it ships as the community
    pack [`packs/unifiprotect`](https://github.com/selfhostr/dumbmonit/tree/main/packs/unifiprotect),
    which anyone can correct without waiting for a release. Install it under
    **Settings → Integration packs** (paste or upload `pack.yaml`), then add a
    monitor of type **UniFi Protect**. See [integration packs](../packs/index.md).

    The pack reads the official integration API with an **API key**, where the
    former built-in type signed in with a local account. Existing Protect
    monitors were moved to the pack's type; once the pack is installed, edit
    each one and replace its user name and password with an API key. The
    integration API does not publish the recording state of the cameras nor the
    health of the recorder's disks: those are no longer watched.

## Network ports to open

From the DumbMonit server (or its relay agent) to the device:

| Port | Protocol | Purpose |
|---|---|---|
| 443 | TCP | UniFi OS (HTTPS) |

## Set it up

1. In UniFi OS, **Settings → Control Plane → Integrations**: create an API key
   named DumbMonit and copy it (it is shown once).
2. In DumbMonit, enter the console address and paste the key as API key. Turn
   on *Accept an unverifiable certificate* while the console keeps its factory
   certificate.

The key is sent in the `X-API-KEY` header. The integration API has no read-only
key: keep it as secret as an administrator password.

## What it watches

All metrics are prefixed `dumbmonit_unifiprotect_`.

| Metric | What | Labels |
|---|---|---|
| `camera_online` | 1 while the camera is connected, 0 otherwise | `camera`, `camera_id` |

## Alert rules

Installed with the pack, under the uid `pack:unifiprotect:<name>`:

| Rule | Fires when |
|---|---|
| `camera_offline` | a camera stays disconnected for 5 minutes |
