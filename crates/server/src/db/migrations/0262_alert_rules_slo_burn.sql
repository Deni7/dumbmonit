-- Règles « vitesse de consommation du budget d'erreur » des SLO
-- (`kind = 'slo_burn'`), calculées par `crate::slo` et non par une requête.
--
-- Même réécriture de la contrainte CHECK que `0214_alert_rules_forecast_drift.sql` :
-- SQLite ne la modifie pas en place, et les lignes existantes la respectent.
PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'CHECK (kind IN (''threshold'', ''anomaly'', ''predict'', ''log_pattern'', ''forecast'', ''drift''))',
    'CHECK (kind IN (''threshold'', ''anomaly'', ''predict'', ''log_pattern'', ''forecast'', ''drift'', ''slo_burn''))'
)
WHERE type = 'table' AND name = 'alert_rules';

PRAGMA writable_schema = RESET;
