# Agents behind a proxy

Agents normally reach DumbMonit on the agent port (8443), with their own TLS
session: TLS 1.3, a client certificate per machine, the server's authority
pinned. That port must reach DumbMonit untouched, which rules out Cloudflare
Tunnel, a reverse proxy that terminates TLS, or a corporate network where only
web traffic goes out.

In those cases agents go through the **web address** instead. The agent opens
a WebSocket to `https://your-address/api/agent/ws` and carries its usual TLS
session *inside* it, end to end to DumbMonit:

- the tunnel or proxy only sees an HTTP request, then opaque binary messages;
- its own certificate proves nothing about the agent: the agent is still
  identified by its client certificate, and still checks DumbMonit by the
  pinned authority — a proxy that inspects TLS reads nothing more;
- no other port to open: 8443 can stay closed to the Internet.

The agent port must still be enabled in DumbMonit (`DUMBMONIT_AGENT_LISTEN`,
on by default): the WebSocket hands its streams to it.

## Install command

When you open DumbMonit through a tunnel or a reverse proxy that presents
another certificate than DumbMonit's own, the install command shown in
**Add → Agent** already carries `--transport=websocket` (`-Transport websocket`
on Windows), and the check above it opens the agent WebSocket at that address
instead of the agent port. Nothing else changes: the script and the agent come
from the same address.

When the check says the agent port does not answer (refused, timeout, another
service on that port), it offers **Use the web address (WebSocket)**: the
command is issued again with `--transport=websocket` and the check runs
through the WebSocket. **Use the agent port** goes back when the WebSocket
does not answer either, or when the server chose it for you but the agent
port is reachable after all.

Through the API, ask for it with `transport=websocket`
(`GET /api/agent/install?base_url=…&transport=websocket`, or `"transport":
"websocket"` when creating a token).

## Choosing the transport on an agent

`transport` in `agent.yaml` (or `DUMBMONIT_AGENT_TRANSPORT`):

| Value | The agent reaches its channel… |
|---|---|
| `auto` (default) | on the agent port first. After repeated connection failures, it tries the WebSocket of the server URL, switches if it answers, and remembers it (`agent-tls.json`) until it fails in turn. |
| `direct` | on the agent port only. |
| `websocket` | through the server URL only (`server_url` + `/api/agent/ws`). |

```yaml
server_url: https://monit.example.com
transport: websocket
```

The WebSocket follows `server_url`: its certificate is checked by the system's
authorities, or by `server_pin` for a self-signed one. The outbound proxy of
the agent (`proxy:`, `HTTPS_PROXY`) is used for it. A relay can use this
transport too: its multiplexed connection, with all the agents of its site,
goes through the same WebSocket. With [backup servers](../devices/agent.md#backup-servers)
(`server_urls`), the WebSocket moves with the server in use.

Inside the agent, a local bridge listens on the loopback (`127.0.0.1`, a port
picked by the system): the agent's HTTP client dials it as it would dial the
agent port, and each connection becomes one WebSocket to the server. Another
program on the machine that connects to it gets nothing it could not get by
opening the web address itself: the TLS session it would carry is checked by
DumbMonit, and the bridge holds no credential.

The machine page shows **Channel: WebSocket via the web address** for an agent
whose last encrypted batch came that way. The server sees it from the
connection; the agent does not declare it.

## Settings on the proxy

The proxy must forward WebSocket upgrades (`Upgrade` and `Connection` headers)
to DumbMonit's **HTTPS** port, without buffering, and must not cut a quiet
connection too early. The agent sends a ping every 20 seconds, so any idle
timeout above a minute is enough.

=== "Cloudflare Tunnel"

    WebSockets are on by default for every Cloudflare zone (**Network →
    WebSockets**). In the tunnel, point the public hostname at DumbMonit's
    HTTPS port and skip the verification of its self-signed certificate:

    ```yaml
    # cloudflared config.yml
    ingress:
      - hostname: monit.example.com
        service: https://dumbmonit:8080
        originRequest:
          noTLSVerify: true
      - service: http_status:404
    ```

    In the dashboard: **Type** `HTTPS`, **URL** `dumbmonit:8080`, then
    **Additional application settings → TLS → No TLS Verify**.
    Cloudflare Access in front of the whole host would also stop agents: add a
    **Bypass** policy for the path `/api/agent/ws` (the agent's own certificate
    is checked by DumbMonit).

=== "nginx"

    ```nginx
    map $http_upgrade $connection_upgrade {
        default upgrade;
        ''      close;
    }

    server {
        listen 443 ssl;
        server_name monit.example.com;
        # ssl_certificate …

        location /api/agent/ws {
            proxy_pass https://dumbmonit:8080;
            proxy_ssl_verify off;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection $connection_upgrade;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_buffering off;
            proxy_read_timeout 3600s;
            proxy_send_timeout 3600s;
        }

        location / {
            proxy_pass https://dumbmonit:8080;
            proxy_ssl_verify off;
            proxy_set_header Host $host;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            client_max_body_size 16m;
        }
    }
    ```

    Without `proxy_http_version 1.1` and the two `proxy_set_header` lines, nginx
    drops the upgrade and the agent sees `HTTP 400` or `426`.

=== "Traefik"

    Traefik forwards WebSockets without any extra setting. Only the backend
    must be DumbMonit's HTTPS port:

    ```yaml
    labels:
      - traefik.http.routers.monit.rule=Host(`monit.example.com`)
      - traefik.http.routers.monit.tls.certresolver=letsencrypt
      - traefik.http.services.monit.loadbalancer.server.port=8080
      - traefik.http.services.monit.loadbalancer.server.scheme=https
      - traefik.http.services.monit.loadbalancer.serverstransport=insecure@file
    ```

    `insecure` is a `serversTransport` with `insecureSkipVerify: true`, declared
    in the dynamic configuration. Leave the entry point's
    `respondingTimeouts.readTimeout` at its default (none), or above a minute.

=== "Caddy"

    `reverse_proxy` forwards WebSockets as is; the block from
    [HTTPS and encryption](https.md#behind-a-reverse-proxy) is enough.

## On the server

| Variable | Default | Effect |
|---|---|---|
| `DUMBMONIT_AGENT_WEBSOCKET` | `on` | `off` closes `/api/agent/ws` (404). |
| `DUMBMONIT_AGENT_WS_MAX_PER_IP` | `256` | WebSocket connections at once from one address. |
| `DUMBMONIT_AGENT_WS_RATE_KIB` | `16384` | Incoming rate of each WebSocket connection, in KiB/s. |

Behind a proxy, every connection comes from the proxy's address. Declare it in
`DUMBMONIT_TRUSTED_PROXIES` so the limits and the enrolment attempt counters
apply per agent address (`X-Forwarded-For`). Those headers are only used for
counting: never for identity.

## What the route accepts

`/api/agent/ws` is no way around the agent channel:

- nothing reaches a route before a complete TLS handshake under the agents'
  authority, inside the WebSocket (10 seconds at most);
- without a client certificate, only enrolment answers (with an enrolment
  token, attempts counted per address), and the session is closed after one
  minute; install scripts are not served there;
- the route opens no session and reads no cookie: a request with an `Origin`
  header (any browser page) or the session cookie is refused;
- messages are capped at 128 KiB, a connection silent for two minutes is
  closed.

## Troubleshooting

The agent logs the transport it uses at startup (`transport=websocket wss://…`)
and the cause of a failed WebSocket:

| Message | Fix |
|---|---|
| `HTTP 404 … at the agent WebSocket` | The proxy does not forward `/api/agent/ws`, or the server predates this transport (update it). |
| `HTTP 400` / `HTTP 426 … Upgrade and Connection headers` | The proxy drops the upgrade: see the nginx settings above. |
| `HTTP 502/503/504` | The proxy or tunnel cannot reach DumbMonit's HTTPS port. |
| `a WebSocket answers at this address, but not the DumbMonit agent channel` | Another service answers at that path. |
| `HTTP 429` | Too many connections from one address: declare the proxy in `DUMBMONIT_TRUSTED_PROXIES`, or raise `DUMBMONIT_AGENT_WS_MAX_PER_IP`. |
