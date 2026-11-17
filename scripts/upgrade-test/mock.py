#!/usr/bin/env python3
"""Faux services joints par le test de mise à jour (scripts/upgrade-test.sh).

Un seul processus HTTP, sans dépendance, qui joue trois rôles et note chaque
requête (une ligne JSON par requête) pour que le test puisse relire, après la
mise à jour, ce que le serveur a réellement présenté :

  - un Home Assistant : `GET /api/config` et `GET /api/states` n'acceptent que
    `Authorization: Bearer <jeton>` ; tout autre jeton reçoit 401. Une sonde qui
    réussit prouve donc que le jeton enregistré, chiffré en base, a bien été
    déchiffré par la nouvelle version ;
  - un serveur ntfy : `POST /` répond 200 et note l'en-tête Authorization, ce
    que l'« envoi de test » d'un canal fait avec son jeton chiffré ;
  - une page quelconque (`GET /`, 200) pour les moniteurs HTTP et TCP.

Usage : mock.py --port 0 --portfile F --log F --ha-token T
"""
import argparse
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=0)
    parser.add_argument("--portfile", required=True)
    parser.add_argument("--log", required=True)
    parser.add_argument("--ha-token", required=True)
    args = parser.parse_args()
    expected = f"Bearer {args.ha_token}"
    log = open(args.log, "a", buffering=1, encoding="utf-8")

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *_):  # le journal utile est le nôtre, en JSON
            pass

        def _note(self, status: int, **extra):
            entry = {
                "t": time.time(),
                "method": self.command,
                "path": self.path,
                "auth": self.headers.get("Authorization"),
                "status": status,
            }
            entry.update(extra)
            log.write(json.dumps(entry) + "\n")

        def _reply(self, status: int, body: str = "", ctype: str = "application/json", **extra):
            data = body.encode()
            self._note(status, **extra)
            self.send_response(status)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            if self.command != "HEAD":
                self.wfile.write(data)

        def _drain(self) -> int:
            length = int(self.headers.get("Content-Length") or 0)
            if length:
                self.rfile.read(length)
            return length

        def do_GET(self):
            path = self.path.split("?", 1)[0]
            if path in ("/api/config", "/api/states"):
                if self.headers.get("Authorization") != expected:
                    return self._reply(401, '{"message":"401: Unauthorized"}')
                if path == "/api/config":
                    return self._reply(200, '{"version":"2025.1.0","state":"RUNNING"}')
                return self._reply(200, "[]")
            return self._reply(200, "upgrade-test mock\n", "text/plain")

        do_HEAD = do_GET

        def do_POST(self):
            length = self._drain()
            return self._reply(200, '{"id":"mock"}', body_bytes=length)

    server = ThreadingHTTPServer(("0.0.0.0", args.port), Handler)
    server.daemon_threads = True
    with open(args.portfile, "w", encoding="utf-8") as handle:
        handle.write(str(server.server_address[1]))
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
