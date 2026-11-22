-- Adresse électronique facultative d'un compte, pour savoir qui joindre. Elle
-- ne sert jamais à se connecter ni à retrouver un compte : l'identifiant reste
-- la seule clé de connexion.
ALTER TABLE users ADD COLUMN email TEXT;
