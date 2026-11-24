-- Annuaire des pages de statut (`GET /api/public/status`) : une page publique
-- (publiée, sans mot de passe ni compte) y figure, sauf si l'administrateur l'en
-- retire. Les pages privées n'y figurent jamais, quelle que soit cette valeur.
ALTER TABLE status_pages ADD COLUMN listed INTEGER NOT NULL DEFAULT 1;
