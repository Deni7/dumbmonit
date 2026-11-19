-- Abonnement par courriel d'une page de statut, ouvert d'un interrupteur :
-- les courriels partent par le serveur de courriel de l'instance (réglage
-- `mail.server` de `settings`), ou par le canal SMTP choisi s'il y en a un
-- (`subscribe_channel_id`, prioritaire). 0 : fermé.
ALTER TABLE status_pages ADD COLUMN subscribe_email INTEGER NOT NULL DEFAULT 0;

-- Une page qui passait déjà par un canal reste ouverte.
UPDATE status_pages SET subscribe_email = 1 WHERE subscribe_channel_id IS NOT NULL;
