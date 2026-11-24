-- Passerelle d'un relais telle qu'il l'a vraiment ouverte (JSON `GatewayState` :
-- adresse demandée, adresse liée ou cause de l'échec, réglage local, canal
-- chiffré), selon son dernier lot. NULL : pas de passerelle demandée, ou relais
-- antérieur, qui ne dit que le port (`relay_gateway_port`).
ALTER TABLE agent_hosts ADD COLUMN relay_gateway_state TEXT;
