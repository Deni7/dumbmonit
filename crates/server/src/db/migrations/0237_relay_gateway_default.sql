-- Passerelle des relais ouverte d'office, et essai de la passerelle par le
-- relais lui-même (voir docs/install/remote-site.md).
--
-- `relay_gateway_wanted` change de sens :
--   NULL  jamais réglée : un relais l'ouvre sur le port par défaut (8443) dès
--         sa première connexion, sans attendre qu'on la demande ;
--   0     fermée depuis l'interface ;
--   N     ouverte sur le port N.
--
-- Les relais déjà en place gardent leur état : une passerelle qu'on n'avait
-- jamais ouverte reste fermée (0), rien ne se met à écouter tout seul sur leur
-- réseau à la mise à jour. Les machines qui ne sont pas (encore) des relais
-- restent à NULL : elles ouvriront leur passerelle le jour où elles le
-- deviennent.
UPDATE agent_hosts SET relay_gateway_wanted = 0
WHERE relay_gateway_wanted IS NULL AND (relay = 1 OR relay_wanted = 1);

-- Dernier essai de sa passerelle par le relais (JSON `GatewayCheck` : adresse
-- composée, réussite, erreur, instant), tel que son dernier lot le rapporte.
ALTER TABLE agent_hosts ADD COLUMN relay_gateway_check TEXT;
