-- Canaux de notification propres à un moniteur. Sans ligne, rien ne change :
-- une alerte part vers les canaux de sa règle, à défaut vers tous. Avec des
-- lignes, une alerte dont la règle ne nomme aucun canal ne part que vers
-- ceux-ci ; une règle qui nomme ses canaux garde le dernier mot.
CREATE TABLE target_channels (
    target_id  INTEGER NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
    channel_id INTEGER NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    PRIMARY KEY (target_id, channel_id)
) WITHOUT ROWID;

CREATE INDEX idx_target_channels_channel ON target_channels(channel_id);
