-- Jetons des boutons Telegram à rappel. Un `callback_data` tient en 64 octets,
-- bien moins qu'un jeton signé : le bouton porte l'aléa du jeton (`nonce`), le
-- jeton lui-même attend ici le clic, le temps de sa validité. Il est vérifié
-- au clic comme celui d'un lien ; une ligne seule n'autorise rien.
CREATE TABLE chat_action_tokens (
    nonce      TEXT PRIMARY KEY,
    token      TEXT NOT NULL,
    expires_at TEXT NOT NULL
) WITHOUT ROWID;

CREATE INDEX chat_action_tokens_expires ON chat_action_tokens (expires_at);
