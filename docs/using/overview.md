# The overview page

The home page answers "is everything fine?" in about one second, then lets you
drill down on demand.

## The status banner {#the-bulletin}

A band at the top shows a fixed night sky: twinkling stars, a crescent moon and a
pigeon that crosses it without ever landing. The sky does not change with the
weather; the sentence at its foot says how things are, in words. "All 146
monitors up." when nothing needs you, "pve-02 is down." or "pve-02 needs a look."
otherwise, followed in a quieter voice by what the system did about it ("3 guests
held back, alert sent."). Tap the pigeon and it rolls. Under
`prefers-reduced-motion`, the pigeon stays out of the sky and the stars hold
still.

## The first-run guide {#the-first-run-guide}

On a new instance, the page under the banner is not empty: it is a three-step
guide — add a monitor, connect a way to be told, check a message arrives. Each
step is one click, each can be waved off on its own, and **Skip the guide** puts
all three away. The choice is stored on the instance, so a second browser does
not start the guide over. See
[Add your first monitor](../install/first-device.md#the-three-first-steps).

## One screen {#one-screen}

The overview is a single screen. On a wide window it is exactly as tall as the
window: the page itself never scrolls, each panel scrolls on its own. On a
narrower one the panels stack and the page scrolls as usual.

```
| Act now                         | Coming up     |
| Services        | Fleet by site | Recent events |
```

"Updated 12 s ago", above the figures, says how fresh the data is.

## The three figures {#the-figures}

A short row of three tiles, each a name, a figure and a small chart:

- **Monitors up**, out of the monitors not paused, with one bar per monitor
  grouped by site (a monitor down stands tall; hover a bar for its name and
  state).
- **Availability 30 d**, with the daily availability and the objective as a
  dashed line.
- **Open problems**: monitors down, unreachable or misconfigured and firing
  warning or critical alerts, the same rows as **Act now**; the chart counts
  the alerts opened per day.

## Act now {#act-now}

The largest panel: what is broken now, worst first, one row each, with how long
it has lasted, who took it (or that nobody has, and whether anyone was told),
and the quick actions: acknowledge, snooze or mute. A problem someone took stays
in the list, below the open ones. With nothing broken, it says so and lists the
last alerts that resolved. Its title opens the [Alerts page](alerts.md).

## Coming up and recent events {#coming-up}

**Coming up** is one list, in time order: maintenance windows of the next 14
days (running ones first), then expiries, late backups, disk forecasts and
critical vulnerabilities of the next 90 days, and a DumbMonit update last.
**Recent events** is the log of the last seven days, newest first: alerts
firing, resolving and acknowledged, maintenance windows starting and ending,
monitors added.

## Services and fleet by site {#fleet-by-site}

**Services** lists the HTTP, DNS, TLS, SMTP, ping and TCP checks, worst first,
with their last checks, the 30-day availability and the p95 response time;
paused checks fold into one line. Without any check, **Fleet by site** takes
the whole bottom row.

**Fleet by site** shows, per site, how many monitors answer and its health in a
word, then one line per category with a status mark per monitor. A site is the
set of monitors reached through one [relay](../install/remote-site.md); monitors
watched directly by the server form one site named after the instance.

Nothing conveys a state by colour alone: a status mark carries a glyph (× down,
! degraded, a hollow ring paused), and every icon sits next to a word.

## Wall mode {#wall-mode}

`/wall` shows a board for a monitor in the room: the verdict, the clock, the five
figures, the health river, the open alerts and who is on call. Open it with the
**Wall mode** icon at the bottom of the left menu or the command palette; ++esc++ or **Exit**
returns to the console. See [Wall mode](wall.md).

## Install it on a phone {#install-it-on-a-phone}

The interface ships a web manifest, so a phone can add it to the home screen:
**Share → Add to Home Screen** on iOS, **Install app** in the Chrome menu on
Android. It then opens without the browser chrome, in the theme the phone is
set to.

There is deliberately **no offline mode and no service worker**. A monitoring
screen that shows a cached page is worse than one that says it cannot reach the
server: the whole point is that what you read is current. With no worker, an
upgraded server is also served straight away, with no stale build to clear.

## Command palette {#command-palette}

++cmd+k++ on macOS, ++ctrl+k++ elsewhere, opens the command palette: one
input, results in three groups.

| Group | Entries |
|---|---|
| Pages | Overview, Monitors, Alerts, Notifications, Status pages, Settings, Wall mode, Documentation |
| Actions | Add a monitor, Scan my network, Schedule maintenance, Add notification channel, New status page, Announce an incident |
| Monitors | Every monitor, with its state LED; ++enter++ opens it |

## Navigation {#navigation}

A slim **icon rail** on the left lists the destinations: Overview, Monitors,
Alerts (with the count of what needs you), Status pages and Settings (plus
Clients in [multi-client mode](msp.md)). The topology map, Security and Logs are
reached from the Monitors page, Reports from Status pages. The
[metrics explorer](dashboards.md) opens from the command palette. At its foot sit the
documentation, GitHub and your account. The header shows where you are, the
**LIVE** switch that pauses the console's own refreshes, the time range, the
palette, **Wall mode** and **New monitor**. On a phone, the rail becomes a bottom
tab bar with the essentials and **More**.

- **Overview**: is everything fine, and what needs you.
- **Monitors**: every source you monitor. See [Monitors page](devices.md).
- **Alerts**: what is firing, maintenance windows, history and the rules.
  See [Alerts page](alerts.md).
- **Settings**: your account, users and single sign-on, tokens, notification
  channels, appearance, about. See [Settings](settings.md).

The interface is dark, and nothing on it conveys status by colour alone: a
marker always comes with a word (Reporting, Unreachable, Advisory, Warning,
Suppressed by parent, Building up).
