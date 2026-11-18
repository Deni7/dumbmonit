-- L'astreinte devient un réglage d'instance (`crate::instance`, clé `instance`
-- de `settings`, champ `oncall_enabled`), coupé par défaut. Une instance qui a
-- déjà un planning, une politique d'escalade ou un canal d'astreinte l'arrive
-- activé : rien ne change pour elle. Une base neuve n'écrit rien ici.

-- Réglage d'instance déjà écrit : le champ s'y ajoute, le reste est conservé.
UPDATE settings
SET value = CAST(json_set(CAST(value AS TEXT), '$.oncall_enabled', json('true')) AS BLOB),
    updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
WHERE key = 'instance'
  AND json_valid(CAST(value AS TEXT))
  AND (EXISTS (SELECT 1 FROM oncall_schedules)
       OR EXISTS (SELECT 1 FROM oncall_policies)
       OR EXISTS (SELECT 1 FROM notification_channels WHERE kind = 'oncall'));

-- Pas encore de réglage d'instance (assistant non terminé) : il est créé, avec
-- l'assistant marqué terminé puisque l'instance est déjà configurée.
INSERT INTO settings (key, value)
SELECT 'instance', CAST('{"setup_completed":true,"oncall_enabled":true}' AS BLOB)
WHERE NOT EXISTS (SELECT 1 FROM settings WHERE key = 'instance')
  AND (EXISTS (SELECT 1 FROM oncall_schedules)
       OR EXISTS (SELECT 1 FROM oncall_policies)
       OR EXISTS (SELECT 1 FROM notification_channels WHERE kind = 'oncall'));
