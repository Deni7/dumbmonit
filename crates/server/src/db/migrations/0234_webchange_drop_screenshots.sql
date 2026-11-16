-- Les captures d'écran de la surveillance des sites sont retirées : la colonne
-- qui disait si un instantané en avait une n'a plus d'objet. Les fichiers
-- `<data>/webchange` sont effacés au démarrage du serveur.
ALTER TABLE webchange_snapshots DROP COLUMN has_screenshot;

-- L'option « screenshots » des anciennes surveillances ne veut plus rien dire.
UPDATE targets SET tags = json_remove(tags, '$.screenshots')
 WHERE kind = 'webchange' AND json_valid(tags) AND json_extract(tags, '$.screenshots') IS NOT NULL;
