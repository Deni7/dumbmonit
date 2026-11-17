-- Serveur que l'agent emploie quand il en connaît plusieurs (`server_urls`,
-- serveur de secours : voir docs/install/high-availability.md). JSON
-- `ServerLink` : rang de l'adresse en service (0 : la primaire), nombre
-- d'adresses, adresse, moment de la bascule et dernière erreur de la
-- primaire, tel que son dernier lot le rapporte. NULL : une seule adresse.
ALTER TABLE agent_hosts ADD COLUMN server_link TEXT;
