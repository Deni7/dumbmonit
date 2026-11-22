-- Destinations hors site des sauvegardes locales (`crate::backup::offsite`) :
-- S3 et compatibles, WebDAV, SFTP.
--
-- `settings` (JSON) ne contient rien de secret : point d'accès, seau, dossier,
-- compte, empreinte confirmée de la clé d'hôte SFTP. Les secrets (clé d'accès
-- secrète, mot de passe, clé privée) sont dans `secrets`, chiffrés par la clé
-- d'instance, et `secret_fields` dit seulement lesquels sont enregistrés.
--
-- Le dernier envoi (`last_*`) est un constat : il ne voyage pas avec le lot.
-- `last_success_at` n'avance que sur un succès, c'est ce que publie
-- `dumbmonit_backup_offsite_last_success_timestamp_seconds`.
CREATE TABLE backup_destinations (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    kind             TEXT    NOT NULL,
    enabled          INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    keep             INTEGER NOT NULL DEFAULT 7 CHECK (keep BETWEEN 1 AND 365),
    -- Vrai : `secret.key` part avec la base. Faux par défaut.
    include_key      INTEGER NOT NULL DEFAULT 0 CHECK (include_key IN (0, 1)),
    settings         TEXT    NOT NULL DEFAULT '{}',
    secrets          BLOB,
    secret_fields    TEXT    NOT NULL DEFAULT '',
    last_at          TEXT,
    last_ok          INTEGER,
    last_file        TEXT,
    last_bytes       INTEGER,
    last_duration_ms INTEGER,
    last_error       TEXT,
    last_success_at  TEXT,
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now')),
    updated_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%d %H:%M:%S', 'now'))
);
