-- Objectifs de niveau de service (SLO) : une disponibilité visée sur une
-- fenêtre glissante, pour un moniteur ou une sélection (moniteurs et dossiers,
-- même périmètre que les rapports SLA : `reports::layout::Scope` en JSON, vide
-- = tous les moniteurs). Le nom sert de clé naturelle à la sauvegarde.
CREATE TABLE slos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE,
    description TEXT    NOT NULL DEFAULT '',
    -- Disponibilité visée, en pour cent, strictement entre 0 et 100 : à 100 %,
    -- il n'y a plus de budget d'erreur à consommer.
    objective   REAL    NOT NULL CHECK (objective > 0 AND objective < 100),
    window_days INTEGER NOT NULL CHECK (window_days IN (7, 28, 30, 90)),
    scope       TEXT    NOT NULL DEFAULT '{}',
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
