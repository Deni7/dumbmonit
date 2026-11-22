-- Repères de changement (« annotations ») : un déploiement, une mise à jour,
-- une maintenance posés sur la frise des graphiques. Un instant, ou une fenêtre
-- (`until`). Les dates sont en UTC, au format de `db::timestamp` (comparables
-- comme du texte). `scope` est du JSON (`annotations::Scope`) : global, une
-- liste de moniteurs, un dossier ou des étiquettes de moniteurs.
CREATE TABLE annotations (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    at         TEXT NOT NULL,
    until      TEXT,
    title      TEXT NOT NULL,
    text       TEXT NOT NULL DEFAULT '',
    link       TEXT NOT NULL DEFAULT '',
    tags       TEXT NOT NULL DEFAULT '[]',
    scope      TEXT NOT NULL DEFAULT '{"kind":"global"}',
    source     TEXT NOT NULL DEFAULT 'api' CHECK (source IN ('api', 'ui', 'ci')),
    author     TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);
CREATE INDEX annotations_at ON annotations (at);
