-- Logo téléversé pour les rapports SLA (`reports::layout::LogoSource::Custom`) :
-- une seule ligne pour l'instance, l'image en base comme le logo d'un client,
-- pour qu'elle voyage avec la sauvegarde. PNG ou JPEG reconnu à sa signature,
-- 256 Kio au plus (vérifié par l'API).
CREATE TABLE report_logo (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    logo_type  TEXT NOT NULL,
    logo       BLOB NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);
