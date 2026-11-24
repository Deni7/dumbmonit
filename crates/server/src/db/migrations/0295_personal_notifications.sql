-- Notifications personnelles : chaque compte peut recevoir lui-même, par
-- courriel ou Web Push, les alertes que ses préférences retiennent, en plus des
-- canaux de l'instance. Les préférences sont un objet JSON
-- (`notify::personal::Prefs`) : sévérité minimale, périmètre (tout, ses
-- équipes, ou des dossiers et étiquettes choisis), heures calmes.

CREATE TABLE user_notification_prefs (
    user_id    INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    prefs      TEXT    NOT NULL,
    updated_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);

-- Ce qui est parti vers chaque compte : délai minimal entre deux messages pour
-- la même empreinte, « ne résoudre que ce qui a été annoncé », plafond horaire.
CREATE TABLE user_notify_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- Équipement du message ; un message par équipement et par cycle.
    target_id   INTEGER,
    fingerprint TEXT    NOT NULL,
    reason      TEXT    NOT NULL,
    at          TEXT    NOT NULL
);

CREATE INDEX idx_user_notify_log_user ON user_notify_log(user_id, fingerprint, at);
CREATE INDEX idx_user_notify_log_at ON user_notify_log(at);
