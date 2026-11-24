-- Rôle `wall` : un compte d'écran mural, qui ne voit que le mode mur (`/wall`)
-- et ne lit que les routes dont celui-ci a besoin (`auth::wall::WALL_ROUTES`).
--
-- Même méthode que `0043_operator_role.sql` : SQLite ne modifie pas une
-- contrainte CHECK en place, et recréer `users` viderait en cascade les
-- sessions, les jetons et les équipes qui la référencent. On réécrit le texte
-- de la contrainte dans le schéma (les colonnes ajoutées depuis par
-- `ALTER TABLE … ADD COLUMN` n'y ont pas touché) ; élargir la liste des
-- valeurs admises ne change rien au format des pages, et toutes les lignes
-- existantes la respectent déjà.
PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'CHECK (role IN (''admin'', ''operator'', ''viewer''))',
    'CHECK (role IN (''admin'', ''operator'', ''viewer'', ''wall''))'
)
WHERE type = 'table' AND name = 'users';

PRAGMA writable_schema = RESET;

-- Toute instruction DDL incrémente la version du schéma : les autres
-- connexions du pool relisent alors le schéma et voient la nouvelle contrainte.
DROP INDEX IF EXISTS idx_users_role;
CREATE INDEX idx_users_role ON users(role);
