-- Note libre d'un moniteur : un texte pour les humains (à qui appartient
-- l'équipement, où il est branché, quoi faire s'il tombe), affiché sur sa page.
-- Vide par défaut : une installation existante ne change pas.
ALTER TABLE targets ADD COLUMN note TEXT NOT NULL DEFAULT '';
