-- Marque blanche d'une page de statut : rien de DumbMonit sur la page publique
-- (titre d'onglet, icône, nom d'application installée, description générique).
-- L'icône devient le logo de la page, ou aucune. 0 : comportement d'avant.
ALTER TABLE status_pages ADD COLUMN hide_branding INTEGER NOT NULL DEFAULT 0;
