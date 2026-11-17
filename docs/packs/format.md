# Pack format reference

A pack is one YAML file, `pack.yaml`, in format version 1. Unknown keys are
errors, not ignored settings: a typo such as `treshold` is reported.

```text
my-pack/
  pack.yaml          the pack itself: the only file the server needs
  fixtures/          one captured response per source, named after its id
    status.json
  expected.prom      what the extraction must produce from the fixtures
```

## Header

| Key | Required | Meaning |
|---|---|---|
| `schema` | yes | Always `1`. |
| `id` | yes | Lowercase letters, digits and single dashes, starting with a letter, up to 40 characters: `shelly-plug`. The monitor type becomes `pack.<id>`, the metric prefix `dumbmonit_<id with underscores>_`. |
| `version` | yes | [Semantic version](https://semver.org) of the pack: `1.2.0`. |
| `requires` | no | DumbMonit versions the pack works with, as a semver requirement: `">=0.2"`. A server outside the range refuses the pack. |
| `label` | yes | Name shown in the monitor type list. |
| `summary` | no | One sentence: what the pack watches. |
| `examples` | no | Products it applies to, shown as hints. |
| `address_hint` | no | Placeholder of the address field: `192.168.1.50`. |
| `default_port` | no | Port used when the address has none. |
| `scheme` | no | `http` (default) or `https`; the monitor can override it. |
| `setup` | no | The notice shown next to the monitor form: `title`, `steps` (a list; what follows a line break in a step is shown as a copyable command), `warning`, `doc_url`. |
| `credentials` | no | Accepted credential kinds, in order of preference: `none`, `api_token`, `username_password`. Default: `[none]`. An entry can be `{kind, label, help}` to reword the form. |
| `options` | no | Settings shown in the monitor form (below). |

## Options

Options are stored on the monitor and read by `{{option.<key>}}` in paths,
headers and bodies, and by rule thresholds.

```yaml
options:
  - key: switch_id          # lowercase, digits, underscores
    label: Switch channel
    help: 0 for a plug; 0, 1, 2… for each channel of a relay.
    input: number           # text (default), number, boolean or select
    default: 0
    required: false
    placeholder: ""
    choices: []             # the values of a select
```

Every pack that polls also gets five connection options, which it cannot
redeclare: `scheme`, `port`, `insecure_tls`, `request_timeout_seconds` (1 to
120, default 10) and `allow_private_targets`.

## Sources

A source is one request per poll, always sent to the device's own address.

```yaml
sources:
  - id: status              # lowercase, digits, underscores
    type: http              # http or prometheus
    path: /rpc/Switch.GetStatus?id={{option.switch_id}}
    method: GET             # GET (default) or POST
    format: json            # http only: json (default), xml or text
    headers:
      Accept: application/json
    body: '{"id": {{option.switch_id}}}'   # POST only
    auth: auto              # auto (default), none, bearer or basic
    optional: false         # true: a refusal or an HTTP error skips this source
```

- `path` must start with a single `/`. The request URL is the monitor's address
  followed by the path; after substitution it must keep the same scheme, host
  and port, or the request is refused. Option values are percent-encoded in
  paths, so a value cannot add a segment or change the host.
- `auth: auto` sends an `api_token` credential as `Authorization: Bearer …` and
  a `username_password` credential as basic authentication. `none` sends
  nothing implicitly, so a header can carry the credential instead:
  `X-Api-Key: "{{credential.token}}"`. `{{credential.token}}`,
  `{{credential.username}}` and `{{credential.password}}` are accepted **in
  headers only**: never in a path or a body, which end up in logs.
- Redirects are followed only to the same scheme, host and port, three at most.
  `401` and `403` are reported as authentication errors, other non-2xx statuses
  as protocol errors.
- A poll makes at most 16 requests and reads at most 4 MiB per response.
- `optional: true`: when this source answers with a refusal (`401`, `403`), an
  HTTP error or an unreadable body, it produces nothing and the poll goes on.
  A device that does not answer at all (unreachable, timeout) still fails the
  poll. For reads a narrower role, a licence or a firmware may refuse.

### XML sources

`format: xml` reads the answer as XML converted to JSON, so the same `json`
paths, `discover` rows and labels apply:

- the document becomes `{"<root>": …}`;
- an element without attributes nor child elements becomes its text, trimmed:
  `<state>up</state>` → `"up"`;
- otherwise an object: attributes under `@<name>`, text under `#text`, each
  child under its name (namespace prefixes dropped);
- several children with the same name become a list. Names listed in `lists`
  are always lists, even when the answer carries a single one, so
  `$.response.result.entry[*]` reads one entry like ten.

```yaml
sources:
  - id: licenses
    type: http
    path: /api/?type=op&cmd=%3Crequest%3E%3Clicense%3E%3Cinfo%3E%3C%2Finfo%3E%3C%2Flicense%3E%3C%2Frequest%3E
    format: xml
    lists: [entry]
metrics:
  - name: answer_ok
    source: licenses
    json: "$.response['@status']"
    map: {success: 1, "*": 0}
```

A DTD is refused (no entity is expanded).

### Session

Some APIs want a session opened before reading and closed afterwards, such as
SonicOS, which allows one API session at a time. `session` declares both
requests; they follow the same rules as sources (the device's own address,
credentials in headers or `auth` only) and their answer is not read, only its
status:

```yaml
session:
  login:
    path: /api/sonicos/auth
    method: POST            # GET or POST
    auth: basic
  logout:                   # optional; sent even when a source failed
    path: /api/sonicos/auth
    method: DELETE          # GET, POST or DELETE
    auth: none
```

A failed login fails the poll. Nothing is copied from the login answer into
the following requests: the session must be kept by the device itself (per
client, or through the credential sent again). The logout does not count in
the 16 requests of the sources.

### Prometheus sources

```yaml
sources:
  - id: metrics
    type: prometheus
    path: /metrics
    keep: [node_load1, node_memory_MemAvailable_bytes]
    rename:
      node_memory_MemAvailable_bytes: memory_available_bytes
    drop:
      fstype: ^(tmpfs|overlay)$
```

- `keep` lists the families imported as they are, with all their labels. A
  family name also covers its histogram and summary series (`_bucket`, `_sum`,
  `_count`). Counters, histograms and summary sums stay counters; everything
  else is a gauge.
- `rename` gives a kept family its DumbMonit name. A name that is not
  lowercase letters, digits and underscores must be renamed.
- `drop` discards every sample whose label matches the regular expression.
- Labels DumbMonit sets itself (`target`, `host`, `instance`, `job`, `tag_*`…)
  are kept as `exported_<name>`.

## Metrics

Each metric has a name *without* prefix: `power_watts` in the `shelly-plug`
pack is written `dumbmonit_shelly_plug_power_watts`. Names are lowercase
letters, digits and underscores, and unique within the pack.

```yaml
metrics:
  - name: power_watts
    help: Active power drawn by the load.
    source: status
    kind: gauge             # gauge (default) or counter
    json: $.apower          # exactly one of json, regex or prom
    scale: 1                # multiplier, e.g. 0.001 for ms → s
    map: {on: 1, off: 0}    # text value → number
    labels: {}
```

| Extractor | Source | Value | `labels` |
|---|---|---|---|
| `json: <JSONPath>` | `http`, format `json` or `xml` | Every node the path matches ([RFC 9535](https://www.rfc-editor.org/rfc/rfc9535) JSONPath, wildcards and filters included) makes one series. | label → JSONPath evaluated on the object that *contains* the value: `$.sensors[*].value` with `sensor: $.name` labels each value with its sibling `name`. |
| `regex: <expression>` | `http`, format `text` | Every match makes one series; the value is the group named `value`, else the first group. | label → name of a capture group. |
| `prom: <sample name>` | `prometheus` | Every sample with this exact name. | label → source label. Empty: all source labels are kept. |

Values: numbers are taken as they are; `true`/`false` become 1/0; text is
looked up in `map`, or parsed as a number when there is no `map`. A value that
is missing, `null`, or outside the `map` produces nothing for this poll. The
key `"*"` (quoted) of a `map` stands for every other text: `{OK: 1, "*": 0}`
turns any status but `OK` into 0, and `{"*": 1}` makes an *info* series whose
labels carry the text (a version, a model).

`date` reads a date instead of a number, written in Unix seconds: `rfc3339`
(`2026-10-31T00:00:00Z`), or a [strftime pattern](https://docs.rs/chrono/latest/chrono/format/strftime/index.html)
such as `"%B %d, %Y"` (`November 18, 2026`). Without a time zone the date is
UTC, without a time it is midnight. A text that is not a date (`Never`)
produces nothing; `date` and `map` exclude each other. A rule then compares it
with `time()`:

```yaml
metrics:
  - name: license_expiration_timestamp_seconds
    source: licenses
    json: $.expirationDate
    date: "%b %d, %Y UTC"
rules:
  - name: license_expiring
    expr: (dumbmonit_my_pack_license_expiration_timestamp_seconds - time()) / 86400
    op: "<"
    threshold: 30
    unit: d
```

## Discover

`discover` turns each element of a list into a set of series sharing labels:
one row per disk, sensor or queue.

```yaml
discover:
  - name: disks
    source: list
    rows: $.disks[*]        # JSONPath: one row per match
    max_rows: 100           # default 100, at most 1,000
    labels:                 # JSONPath evaluated on each row
      disk: $.name
      model: $.info.model
    metrics:
      - name: disk_used_bytes
        json: $.used        # JSONPath evaluated on each row
      - name: disk_healthy
        json: $.state
        map: {ok: 1, degraded: 0}
```

Row metrics accept `name`, `help`, `kind`, `json`, `scale`, `map` and `date`.
Rows beyond `max_rows` are ignored.

When each row carries a list of its own (the ports of a switch, the uplinks of
a router), `each` reads the metrics on every element of that list, which keeps
the labels of its row and adds `each_labels`:

```yaml
discover:
  - name: uplinks
    source: uplinks
    rows: $[*]              # one row per router
    labels:
      serial: $.serial
    each: $.uplinks[*]      # JSONPath evaluated on each row
    each_labels:            # JSONPath evaluated on each element
      interface: $.interface
    metrics:
      - name: uplink_up
        json: $.status      # evaluated on each element
        map: {active: 1, failed: 0}
```

`max_rows` then bounds the total number of elements read.

## Rules

Rules are installed with the pack, as regular alert rules with the uid
`pack:<id>:<name>`. They then belong to you: an update of the pack adds the
rules it does not have yet, and never rewrites one you changed or deleted.

```yaml
rules:
  - name: overheating       # lowercase, digits, underscores
    title: Shelly plug overheating
    description: The internal temperature stayed above the limit for five minutes.
    expr: dumbmonit_shelly_plug_temperature_celsius
    op: ">"                 # >, >=, < or <=
    threshold: "{{option.max_temperature}}"   # or a number
    for: 5m                 # 30s, 5m, 1h, 1d
    severity: warning       # info, warning (default) or critical
    unit: °C
```

- `expr` is [MetricsQL](https://docs.victoriametrics.com/metricsql/) and must
  use at least one metric of the pack, by its full name. Any other
  `dumbmonit_*` name it cites must be produced by the pack too
  (`dumbmonit_up` excepted): a pack cannot alert on another type's series.
- A `threshold` of `{{option.x}}` installs the option's default value, and each
  monitor's own value of that option becomes its threshold for this rule (a
  per-monitor override, rewritten whenever the monitor is saved).

## SNMP profiles

A pack can bring SNMP profiles, written exactly like the built-in ones in
[`profiles/`](https://github.com/selfhostr/dumbmonit/tree/main/profiles). They
join the SNMP catalogue and are applied by `sysObjectID` like the others; they
do not create a monitor type of their own.

```yaml
snmp_profiles:
  - id: acme-ups            # the pack id, or starting with "<pack id>-"
    name: ACME UPS
    match:
      sysobjectid: [1.3.6.1.4.1.99999]
    include: [system]
    metrics:
      - name: acme_ups_battery_percent   # must start with the pack prefix
        oid: 1.3.6.1.4.1.99999.1.1.0
        kind: gauge
        label: Battery charge             # optional, shown on the device page
        unit: "%"                         # optional: %, s, B, °C, bool…
        view: timeseries                  # optional: stat, timeseries, topn or heatmap
        view_title: Battery               # metrics sharing a view and a title share a chart
```

The SNMP catalogue is read at startup: restart DumbMonit after installing or
updating a pack with SNMP profiles.

## Limits

| Limit | Value |
|---|---|
| Pack size | 256 KiB |
| Sources (requests per poll, redirects included) | 16 |
| Response size | 4 MiB |
| Series per poll | 5,000 (the rest is dropped and logged) |
| Metrics declared | 256 |
| Rules | 64 |
| Rows per `discover` | 1,000 |
| Label value length | 200 bytes |

## Fixtures and expected output

`dumbmonit pack test <dir>` reads `fixtures/<source id>.json`, `.xml`, `.txt`
or `.prom`, runs the same extraction as the server, and compares the result with
`expected.prom`: the Prometheus text format, sorted, without timestamps and
without the monitor labels (`target`, `host`, `tag_*`) the server adds.
`--update` writes `expected.prom` from the fixtures; review the diff before
committing it.
