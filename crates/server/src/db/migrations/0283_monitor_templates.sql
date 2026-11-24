-- Modèles de moniteur : de quoi préremplir le formulaire d'ajout (type,
-- options et étiquettes, période, canaux). Jamais un secret ni une adresse :
-- un modèle sert à créer d'autres moniteurs du même genre. Le nom sert de clé
-- naturelle à la sauvegarde.
CREATE TABLE monitor_templates (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL UNIQUE CHECK (name <> ''),
    kind          TEXT    NOT NULL CHECK (kind <> ''),
    interval_secs INTEGER NOT NULL CHECK (interval_secs >= 10),
    -- Options du type et étiquettes libres, en JSON, secrets écartés.
    tags          TEXT    NOT NULL DEFAULT '{}',
    created_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now')),
    updated_at    TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%S', 'now'))
);

CREATE TABLE monitor_template_channels (
    template_id INTEGER NOT NULL REFERENCES monitor_templates(id) ON DELETE CASCADE,
    channel_id  INTEGER NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    PRIMARY KEY (template_id, channel_id)
) WITHOUT ROWID;
