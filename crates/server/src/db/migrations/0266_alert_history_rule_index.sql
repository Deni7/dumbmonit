-- Index couvrant pour « dernier déclenchement » de chaque règle (`last_fired_by_rule`),
-- relu à chaque lecture de la liste des règles.
CREATE INDEX IF NOT EXISTS idx_alert_history_rule_firing ON alert_history(rule_uid, to_phase, at);
