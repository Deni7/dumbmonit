-- Langues d'une page de statut publique : la langue par défaut de son interface
-- (vide : l'anglais) et, par langue, les textes écrits par l'administrateur.
-- `labels` : objet JSON « libellé écrit -> traduction » pour les noms de
-- services et de groupes.
ALTER TABLE status_pages ADD COLUMN default_lang TEXT NOT NULL DEFAULT '';

CREATE TABLE status_page_translations (
    page_id     INTEGER NOT NULL REFERENCES status_pages(id) ON DELETE CASCADE,
    lang        TEXT NOT NULL,
    title       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    footer_text TEXT NOT NULL DEFAULT '',
    labels      TEXT NOT NULL DEFAULT '{}',
    PRIMARY KEY (page_id, lang)
);
