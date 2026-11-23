# The monitors page

## The list

The **Monitors** page shows every monitor in one of two layouts, remembered in
your browser:

- **Compact** (the default): one line per monitor, in fixed columns — icon,
  name, address, location, when it was last seen (short: `2min`, `3h`, `5d`;
  hover for the date), security grade, alerts and the state. A column stays in
  place whether or not a line has something to show in it. Children are
  indented under their parent and dim when the parent is unreachable. On a
  narrow window the address moves under the name, then the age and the grade
  are dropped.
- **Tiles**: one card per monitor, with a sparkline of a representative metric.
  A child names the monitor it sits behind.

A monitor's location appears as a two-letter code (`CR` for Crissier; hover for
the name). You choose the code when you edit the location in Settings; by
default it is the first two letters of the name.

Devices are grouped by folder; a folder header shows its state only when
something inside is not reporting. Administrators reorder monitors by dragging
them (Compact layout, unfiltered and unsorted list) or with the keyboard
(Space, arrows, Space).

| State | Meaning |
|---|---|
| Reporting | The last probe succeeded, within three periods. |
| Unreachable | The monitor did not answer (timeout, connection refused), or no probe for more than three periods (at least 90 s). |
| Misconfigured | The last probe failed on our side: wrong credentials, invalid address or option, unexpected answer. Shown in the advisory colour with the error; it raises no alert. |
| Waiting | Added, not probed yet. |
| Disabled | Paused: not checked, raises no alert. |

One toolbar holds everything:

- the **search** box (name, address, kind or tag);
- **Filter**, which opens the state (All, Needs attention, Reporting,
  Disabled), kind, site, sort and tag filters — the number on the button counts the
  ones in use, and **Clear filters** resets them. The tag filter is one button
  that opens a searchable checklist and summarises the chosen tags on one line,
  with a × to clear them. The sort can also be by availability over the last
  24 hours (worst first) or by the date the monitor was added. Filters live in
  the address, so a filtered list can be bookmarked or shared; opened without
  any, the list comes back with the view, filters and sort you left it with
  (the search text is not remembered);
- **Compact** / **Tiles**;
- **Create folder** (administrators): a name field opens under the toolbar;
- **Select** (administrators), which turns on a checkbox per monitor and a
  floating bar at the bottom with the bulk actions (**Edit**, enable, disable,
  silence for 1 hour to 7 days, move to a folder, delete, and under **More**
  location, relay and CVE check); **Done** or Escape leaves the mode;
- **⋯** (more actions): the topology **Map**, **Import websites (CSV)** and
  **Import from Uptime Kuma**;
- **Add a monitor**, the primary action.

**Edit** changes the whole selection in one go: the check interval, the
notification channels (**Replace with**, **Add** or **Remove** the ticked
ones), free tags (add one as `key=value`, or tick the tags of the selection to
remove them) and the folder. Only what you change is applied; the rest of each
monitor stays as it is. Every monitor is checked as if it were edited on its
own: one managed by Docker labels or a configuration file is left out, and so
is one your teams do not let you manage. The others are saved together. When
some are left out, the list names them with the reason and keeps them selected.
A setting of a monitor's type (a port, a path) is not a tag: it is changed in
the monitor's own form. The same edit is available to scripts as
`PATCH /api/targets/bulk`.

With thousands of monitors, the compact list only draws the lines near the
screen and fills the rest with empty space as you scroll, so it stays fast.
Folders, nested sub-monitors, selection and the `j` / `k` keys work as usual.
While a line is being dragged (or picked up with Space) every line is drawn,
so the drop can land anywhere.

Each line ends with a **⋯** menu (shown on hover or focus, always on a phone):
**Probe now**, **Enable** / **Disable** (administrators) and **Copy address**. A
monitor's free tags show as small chips after its kind on wide lists.

## A monitor

The monitor page shows its state plate, kind, address, interval, last probe, and
for services the certificate status. It is for reading data: actions are icon
buttons (hover or long-press for the name), and everything that changes the
monitor is behind the gear. Actions:

| Action | Effect |
|---|---|
| Probe now (pulse icon) | Runs one probe immediately and reports how many samples and series it produced. The first thing to try when a monitor shows no data. |
| Snooze (bell icon) | A small menu: **Snooze 1 h**, or **Snooze until…** for a custom end. A one-off maintenance window on this monitor; the menu lifts it again. |
| Logs | Opens the log search filtered on this monitor. |
| Duplicate | Opens the add form filled in from this monitor, its name ending in "(copy)". Secrets (credentials, secret-looking options, HTTP headers) are never copied: type them again. Not offered for agents or monitors managed by Docker labels. |
| Save as template | Administrators. Keeps the type, its settings and free tags, the check interval and the notification channels under a name of your choice. Never the address or a secret: credentials, secret-looking options and HTTP headers are left out, and of an HTTP scenario only each step's name, method and URL remain. Not offered for agents or monitors managed by Docker labels. |
| Settings (gear) | Opens the same form as when adding, prefilled. Leaving the credential empty keeps the stored one. For an agent it also holds the relay mode, log collection, the services to watch on its machine and re-enrolment. |
| More (⋯), Delete | Asks "Delete for good?" inline for five seconds, then deletes the monitor. Its series stay in VictoriaMetrics until retention expires. |

Beside the address, a button copies it, and the arrows at the top right of the
page go to the previous and next monitor in the list order. The tags and the
**note** (free text set in the form's *More options*) appear under the title,
and a **Maintenance windows** block lists the active and upcoming windows that
cover this monitor, with a link to manage them.

Below: **Alerts on this monitor** — each active alert offers *Ack*, *Snooze*
and *Ignore*, and a rule ignored on this monitor is listed separately with
**Stop ignoring** — then **Metrics** over a time range (1 h, 6 h, 24 h,
7 days), one chart per metric name with one line per series. See
[Getting rid of an alert](alerts.md#getting-rid-of-an-alert).

Services (HTTP, TCP, DNS, ping, TLS) get instruments instead: **Availability**
over the last 24 hours or 7 days, **Response time**, the number of **Checks**,
and a **History** bar of slots (oldest on the left) showing when the service
was down.

## Container policies

On a machine with an agent and Docker, a **Docker** strip sits under the
header: container count, how many run, updates available, and how many
containers have a policy. **Manage containers** opens the list; **Policies…**
opens a table with two switches per container and an "Apply to all" row:

- **Restart if down** — the server restarts the container within a minute of
  it stopping (at most once every ten minutes);
- **Auto-update in maintenance windows** — when the registry has a newer image
  for the same tag, the container is recreated during a scheduled maintenance
  window (Alerts page → Maintenance) and rolled back if it fails its healthcheck.

Each switch saves on its own. Details and safety rules: [Docker
containers](../devices/agent.md#docker-containers).

## Adding and editing

**Add a monitor** starts with the site: pick a location of the
[world map](world-map.md), or **No site**. DumbMonit then proposes what to add
there: devices a [network scan](discovery.md) found through that site's relay
agent and nobody monitors yet (**Add** creates them at once, except SNMP devices,
which open the form for the community), and the monitor types that fit the site
(the ones its devices already use, then the common ones). Every proposal is
filled in with the site's relay agent, its location and its usual folder, and you
can still change them in the form. **No site** is a normal choice: the monitor is then
not tied to a place on the map. Without any location the page is the plain
list of types. In MSP mode only the current client's sites are offered. The
setup wizard's *Add your first monitors* step offers the same choice.

See [Add your first monitor](../install/first-device.md) for the form and the
[monitor types](../devices/index.md) for what each type needs. When editing, the
form opens **More options** by itself if the monitor already uses one.

**From a template**, above the monitor types, lists the saved templates:
choosing one opens its type's form filled in from it, as **Duplicate** does,
with the name, the address and the credential left for you to type.
Administrators rename (pencil) or delete (bin) templates there. Deleting a
template does not touch the monitors created from it. Templates are part of the
[backup](../install/backup.md).

**Notification channels** (in the form, once a channel exists) pick where this
monitor's alerts go. An alert whose rule sends to every channel goes to the
ticked channels only; a rule that names its own channels keeps them. Nothing
ticked is the default: every channel the rules choose.

The check interval is a preset (30 s to 15 min) or **Custom…**: a whole number
and a unit (seconds to days), from 10 seconds to 7 days. Leaving the form with
unsaved changes asks for confirmation.

## Deleting

Deleting a monitor removes it from the configuration, clears its alerts on the
spot — nothing is notified, the history keeps a `device removed or disabled`
line — and deletes its time series from the embedded VictoriaMetrics a few
seconds later (best effort: a failure is only logged, and the series then age
out with the 12-month retention). Pausing a monitor clears its alerts the same
way; its series stay.
