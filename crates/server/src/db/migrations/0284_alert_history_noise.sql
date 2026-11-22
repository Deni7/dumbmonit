-- Rapport de bruit des alertes (`alerting/noise.rs`, `db/alert_insights.rs`).
--
-- Une ligne `to_phase = 'firing'` ouvre un épisode ; deux colonnes le
-- complètent :
--   * `acked_at` : premier acquittement (ou « j'ai compris ») de l'épisode,
--     RFC 3339 UTC. Sert au délai moyen d'acquittement (MTTA).
--   * `notifications` : notifications prises en charge par le moteur pour
--     l'épisode — déclenchement, rappels, escalades, résolution.
-- Les lignes écrites avant cette migration gardent 0 : le rapport compte
-- alors leur drapeau `notified`.
ALTER TABLE alert_history ADD COLUMN acked_at TEXT;
ALTER TABLE alert_history ADD COLUMN notifications INTEGER NOT NULL DEFAULT 0;
