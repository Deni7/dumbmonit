# HTTP scenario

**HTTP scenario** plays several requests in a row, the way a user would: log
in, keep the session cookie or the token, open a protected page and check what
it shows. Where the [HTTP](services.md#http) monitor checks one page, a scenario
checks a journey: it catches a login that no longer works while the home page
still answers 200.

A scenario is made of 1 to 10 **steps**, played in order. Each step is one
request (method, URL, headers, body) followed by checks on its response, and it
can keep a value from that response for the next steps. The first step that
fails stops the scenario, and the monitor is reported down.

## Network ports to open

From the DumbMonit server (or its relay agent) to the device:

| Port | Protocol | Purpose |
|---|---|---|
| 443 | TCP | Web pages or API over HTTPS (default port) |
| 80 | TCP | Only with http:// URLs (default port) |

## Set it up

1. In the address, paste the base URL of the service: steps can then use paths such as /login.

2. Add the steps in order: method, URL, headers, body and the status codes accepted.

3. To reuse a value from a response, add an extraction (JSON path, header or regular expression) and write its name as {{name}} in a later step.

4. If a step logs in, choose a credential and write {{secret.username}}, {{secret.password}} or {{secret.token}} where it belongs.

**Required permissions**

- **A test account on the service**: Only if a step logs in: give it the fewest rights possible

!!! warning
    Never write a password or a token in the steps themselves: use the credential and its {{secret.…}} placeholders, which are stored encrypted.

Use **Test connection** before saving: it plays the whole scenario once and
shows the first error, if any.

## A step

| Field | Default | What it does |
|---|---|---|
| Name | *(empty)* | Shown on the monitor page and in error messages. Optional. |
| Method | `GET` | GET, HEAD, POST, PUT, PATCH, DELETE or OPTIONS. |
| URL | — | A path such as `/api/login`, joined to the address, or a full URL (another site, for example an identity provider). |
| Headers | *(none)* | Name and value pairs, for example `Content-Type: application/json`. |
| Body | *(empty)* | Sent as is with the request. |
| Expected status | `200-299` | Codes or ranges accepted, separated by commas: `200,302`. Any other code fails the step. |
| Checks | *(none)* | **Contains** a text, **Does not contain** a text, or **JSON value**: the value at a JSON path must equal the expected one. Text checks ignore upper and lower case. |
| Extractions | *(none)* | Values kept from the response under a name, for the following steps. |

Redirects are followed within a step (up to **Maximum redirects per step**),
each hop going through the same address checks as the
[HTTP](services.md#http) monitor. The headers of a step are only sent to the
origin of its own URL: they are dropped when a redirect leads to another site,
and a body that holds a secret is never sent there.

## Variables and extractions

An extraction keeps one value of the response under a name made of letters,
digits and `_` (not starting with a digit). Three sources:

| Source | Expression | Value kept |
|---|---|---|
| JSON | A path such as `$.token` or `data.items[0].id` | The value at that path. A string is kept without its quotes; a number, `true` or an object as JSON text. |
| Header | A header name such as `X-Request-Id` | The value of that response header. |
| Regex | A regular expression such as `csrf_token" value="([^"]+)"` | The first capture group, or the whole match without a group. |

A later step writes the name between double braces, in its URL, a header value
or its body: `/api/orders/{{order_id}}`, `Authorization: Bearer {{token}}`.

Two filters, written after a vertical bar, adapt a value to where it goes:

- `json` escapes quotes and backslashes for a JSON string:
  `{"password":"{{secret.password|json}}"}`.
- `url` percent-encodes the value for a URL or a form body:
  `/search?q={{term|url}}`.

Using a name that no earlier step extracts is a configuration error: it is
shown on the monitor before any request is sent, and it never raises an outage
alert. An extraction that finds nothing in the response, on the other hand,
fails its step with the reason **extract**.

## Cookies

Cookies set by a response are sent with the following requests of the same
scenario, following the usual rules: a cookie goes back to the host that set it
(or to its `Domain` and subdomains), within its `Path`, and a `Secure` cookie
only over HTTPS. Every check starts with no cookie: a session opened by one run
is never reused by the next, so each check really logs in again.

## Credentials

The credential is stored encrypted and never shown again. A scenario sends it
only where a step writes it:

| Credential | Placeholders |
|---|---|
| Username / password | `{{secret.username}}`, `{{secret.password}}` |
| Token | `{{secret.token}}` |

Nothing is added on its own: write `Authorization: Bearer {{secret.token}}` in a
header, or put `{{secret.username}}` and `{{secret.password|json}}` in a login
body. A step that uses a placeholder the credential does not provide is a
configuration error.

Changing the steps of a monitor that has a stored credential asks for the
credential again: a stored secret is never sent to a new destination.

## Example: log in, then read a protected page

Address: `https://app.example.com`, credential **Username / password**.

1. **Login**: `POST /api/login`, header `Content-Type: application/json`, body
   `{"user":"{{secret.username}}","password":"{{secret.password|json}}"}`,
   expected status `200`, check **JSON value** `$.ok` = `true`, extraction
   **JSON** `$.token` as `token`.
2. **Profile**: `GET /api/me`, header `Authorization: Bearer {{token}}`, check
   **Contains** `admin`.

Stored in the `steps` option, as written by the form:

```json
[
  {
    "name": "Login",
    "method": "POST",
    "url": "/api/login",
    "headers": [{ "name": "Content-Type", "value": "application/json" }],
    "body": "{\"user\":\"{{secret.username}}\",\"password\":\"{{secret.password|json}}\"}",
    "expect_status": "200",
    "assertions": [{ "type": "json", "path": "$.ok", "value": "true" }],
    "extract": [{ "name": "token", "from": "json", "expression": "$.token" }]
  },
  {
    "name": "Profile",
    "url": "/api/me",
    "headers": [{ "name": "Authorization", "value": "Bearer {{token}}" }],
    "assertions": [{ "type": "contains", "value": "admin" }]
  }
]
```

Check types are `contains`, `not_contains` and `json`; extraction sources are
`json`, `header` and `regex`.

## Options

| Key | Label | Default | Help |
|---|---|---|---|
| `steps` | Steps | — | The requests played in order, from 1 to 10 (see above). |
| `follow_redirects` | Follow redirects | `true` | Disable to check the redirect a step receives instead of the page it leads to. |
| `max_redirects` | Maximum redirects per step | `10` | Between 1 and 20. No effect if redirects are not followed. |
| `insecure_tls` | Accept an unverifiable certificate | `false` | The steps no longer fail on a self-signed certificate or one issued by a private authority. |
| `allow_private_targets` | Allow loopback and link-local targets | `false` | By default the steps refuse addresses only the DumbMonit host itself can reach (127.0.0.1, ::1, 169.254.x.x). Private LAN addresses (10.x, 192.168.x) are always allowed. |
| `check_certificate` | Read the certificate | `true` | Over HTTPS, also records the certificate expiry date of the address so you can be warned before it expires. |
| `max_body_bytes` | Maximum bytes read per step | `524288` | Beyond this, the rest of a response is not downloaded. Checks and extractions only see this part. |
| `user_agent` | Announced identity (User-Agent) | `DumbMonit/<version>` | Name the steps give to the server. |
| `timeout_seconds` | Timeout (seconds) | `5` | Time allowed for the whole scenario, every step included. Between 1 and 60: raise it for a long scenario. |

## Monitor page

A scenario gets the same page as the other service checks: availability,
history bar and response time (the whole scenario). Under them, **Last run**
lists each step with its HTTP status, its duration and its result: passed,
failed with the reason, or not run because an earlier step failed.

## Metrics

On top of `probe_success`, `probe_duration_seconds` (the whole scenario),
`probe_failure_info` and the certificate metrics of the address, one series per
step, with the label `step` (its position, from 1):

| Metric | Meaning |
|---|---|
| `probe_step_duration_seconds` | Time spent on the step, redirects included. |
| `probe_step_status_code` | HTTP status of the last response of the step. |
| `probe_step_result` | `1` passed, `0` failed, `2` not run (an earlier step failed). |

```
# Steps failing right now, by monitor and step:
dumbmonit_probe_step_result == 0
```

Step values (tokens, extracted text, secrets) never appear in labels or in
error messages: errors name the step and the placeholder, not what it held.
