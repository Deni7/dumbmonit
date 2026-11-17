-- Contrôle CVE par équipement.
--
-- Jusqu'ici le contrôle des failles connues n'avait qu'un interrupteur global.
-- `cve_scan` permet d'écarter un équipement (un banc d'essai, une machine qu'on
-- ne maintient pas) sans toucher aux autres : à 0, le moteur l'ignore et ses
-- résultats ne sont plus montrés. Ils reviennent à la réactivation.
-- Par défaut 1 : une installation existante ne change pas de comportement.
ALTER TABLE targets ADD COLUMN cve_scan INTEGER NOT NULL DEFAULT 1 CHECK (cve_scan IN (0, 1));
