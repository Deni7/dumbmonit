-- Transport du dernier lot chiffré d'une machine à agent : 1 s'il est arrivé
-- par la WebSocket de l'interface (`GET /api/agent/ws`, derrière Cloudflare
-- Tunnel ou un mandataire), 0 en direct sur le port des agents ou par un
-- relais. Constaté par le serveur, jamais déclaré par l'agent.
ALTER TABLE agent_hosts ADD COLUMN tls_websocket INTEGER NOT NULL DEFAULT 0;
