-- Liens à usage unique des comptes : réinitialisation du mot de passe et
-- invitations. Le lien porte `<identifiant>.<secret>` ; la base ne garde que
-- l'empreinte SHA-256 du secret, comme pour les sessions : une copie de la
-- base ne donne aucun lien utilisable. Horodatages UTC sans suffixe.

CREATE TABLE password_resets (
    id           TEXT    PRIMARY KEY,
    token_hash   BLOB    NOT NULL,
    user_id      INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    -- `self` : demandé depuis l'écran de connexion ; sinon le nom de
    -- l'administrateur qui a créé le lien.
    requested_by TEXT    NOT NULL,
    created_at   TEXT    NOT NULL,
    expires_at   TEXT    NOT NULL,
    used_at      TEXT
);

CREATE INDEX idx_password_resets_user ON password_resets(user_id);

CREATE TABLE user_invitations (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    token_id         TEXT    NOT NULL UNIQUE,
    token_hash       BLOB    NOT NULL,
    email            TEXT    NOT NULL,
    -- Jamais `wall` : un écran mural ne s'invite pas par courriel.
    role             TEXT    NOT NULL CHECK (role IN ('admin', 'operator', 'viewer')),
    -- Équipes rejointes à l'acceptation : tableau JSON d'identifiants.
    teams            TEXT    NOT NULL DEFAULT '[]',
    invited_by       TEXT    NOT NULL,
    created_at       TEXT    NOT NULL,
    expires_at       TEXT    NOT NULL,
    last_sent_at     TEXT,
    accepted_at      TEXT,
    accepted_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    revoked_at       TEXT
);

CREATE INDEX idx_user_invitations_email ON user_invitations(email);
