-- Post-mortem d'un incident : résumé, impact, cause, chronologie et actions.
-- Un brouillon s'écrit à tout moment ; il ne se publie qu'une fois l'incident
-- résolu, et n'apparaît sur la page publique qu'une fois publié.
--
-- La chronologie (`[{"at": "…Z", "text": "…"}]`) et les actions
-- (`[{"text": "…", "done": false}]`) sont des listes ordonnées, gardées en JSON :
-- elles se lisent et s'écrivent toujours d'un bloc avec le post-mortem.
CREATE TABLE incident_postmortems (
    incident_id  INTEGER PRIMARY KEY REFERENCES incidents(id) ON DELETE CASCADE,
    summary      TEXT    NOT NULL DEFAULT '',
    impact       TEXT    NOT NULL DEFAULT '',
    root_cause   TEXT    NOT NULL DEFAULT '',
    timeline     TEXT    NOT NULL DEFAULT '[]',
    actions      TEXT    NOT NULL DEFAULT '[]',
    published    INTEGER NOT NULL DEFAULT 0,
    published_at TEXT,
    created_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at   TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);
