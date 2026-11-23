-- Rôles personnalisés : un nom, une description et un jeu de permissions pris
-- dans le catalogue fixe du serveur (`auth::permissions`), stocké en tableau
-- JSON de leurs noms (`["view","monitors.manage"]`). Un rôle personnalisé
-- contient toujours `view`.
--
-- Un compte a soit un rôle livré (`users.role`), soit un rôle personnalisé
-- (`users.custom_role_id`, son rôle livré reste alors `viewer`). Les comptes
-- existants ne changent pas. La suppression d'un rôle encore attribué est
-- refusée par l'API ; `SET NULL` n'est qu'un filet, qui ramène le compte au
-- rôle le plus faible (lecteur). Horodatages UTC sans suffixe.

CREATE TABLE custom_roles (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    description TEXT    NOT NULL DEFAULT '',
    permissions TEXT    NOT NULL DEFAULT '["view"]',
    created_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    updated_at  TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);

ALTER TABLE users ADD COLUMN custom_role_id INTEGER
    REFERENCES custom_roles(id) ON DELETE SET NULL;

CREATE INDEX idx_users_custom_role ON users(custom_role_id);
