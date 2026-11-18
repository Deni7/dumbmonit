-- Code court d'un emplacement : la pastille de deux caractères qui le désigne
-- dans la liste des moniteurs (« CR » pour Crissier).
--
-- Les emplacements existants reçoivent les deux premiers caractères de leur nom,
-- en majuscules et sans les accents courants. Ce que SQL ne sait pas normaliser
-- (alphabets non latins, symbole en tête, espace en deuxième position) reste
-- vide : le serveur calcule alors le même code par défaut à la lecture.
ALTER TABLE locations ADD COLUMN code TEXT NOT NULL DEFAULT '';

UPDATE locations SET code = UPPER(SUBSTR(
REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(name, 'à', 'a'), 'á', 'a'), 'â', 'a'), 'ä', 'a'), 'å', 'a'), 'À', 'a'), 'Á', 'a'), 'Â', 'a'), 'Ä', 'a'), 'Å', 'a'), 'ç', 'c'), 'Ç', 'c'), 'è', 'e'), 'é', 'e'), 'ê', 'e'), 'ë', 'e'), 'È', 'e'), 'É', 'e'), 'Ê', 'e'), 'Ë', 'e'), 'ì', 'i'), 'í', 'i'), 'î', 'i'), 'ï', 'i'), 'Ì', 'i'), 'Í', 'i'), 'Î', 'i'), 'Ï', 'i'), 'ñ', 'n'), 'Ñ', 'n'), 'ò', 'o'), 'ó', 'o'), 'ô', 'o'), 'ö', 'o'), 'Ò', 'o'), 'Ó', 'o'), 'Ô', 'o'), 'Ö', 'o'), 'ù', 'u'), 'ú', 'u'), 'û', 'u'), 'ü', 'u'), 'Ù', 'u'), 'Ú', 'u'), 'Û', 'u'), 'Ü', 'u'), 1, 2))
WHERE UPPER(SUBSTR(
REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(REPLACE(name, 'à', 'a'), 'á', 'a'), 'â', 'a'), 'ä', 'a'), 'å', 'a'), 'À', 'a'), 'Á', 'a'), 'Â', 'a'), 'Ä', 'a'), 'Å', 'a'), 'ç', 'c'), 'Ç', 'c'), 'è', 'e'), 'é', 'e'), 'ê', 'e'), 'ë', 'e'), 'È', 'e'), 'É', 'e'), 'Ê', 'e'), 'Ë', 'e'), 'ì', 'i'), 'í', 'i'), 'î', 'i'), 'ï', 'i'), 'Ì', 'i'), 'Í', 'i'), 'Î', 'i'), 'Ï', 'i'), 'ñ', 'n'), 'Ñ', 'n'), 'ò', 'o'), 'ó', 'o'), 'ô', 'o'), 'ö', 'o'), 'Ò', 'o'), 'Ó', 'o'), 'Ô', 'o'), 'Ö', 'o'), 'ù', 'u'), 'ú', 'u'), 'û', 'u'), 'ü', 'u'), 'Ù', 'u'), 'Ú', 'u'), 'Û', 'u'), 'Ü', 'u'), 1, 2)) GLOB '[A-Z0-9][A-Z0-9]';
