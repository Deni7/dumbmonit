-- Rattrapage des relais hors ligne (voir crates/server/src/collectors/relay.rs).
--
-- Pendant une coupure du lien, un relais sonde seul et range ses résultats dans
-- un tampon disque numéroté ; au retour, il les renvoie par lots. Cette table
-- dit jusqu'où le serveur a reçu le tampon de chaque relais : un lot renvoyé
-- parce que son accusé de réception s'est perdu est écarté sur ce numéro, sans
-- rien écrire deux fois.
--
-- `spool_id` : identité du tampon côté relais, tirée au hasard à sa création ;
-- un relais réinstallé en change, et sa numérotation repart de 1.
-- `dropped` : résultats que le relais dit avoir perdus (tampon plein), cumul.
CREATE TABLE relay_backlog_cursors (
    agent_id   INTEGER PRIMARY KEY REFERENCES targets(id) ON DELETE CASCADE,
    spool_id   TEXT NOT NULL,
    last_seq   INTEGER NOT NULL,
    dropped    INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
