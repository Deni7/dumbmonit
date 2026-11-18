-- Rapports SLA personnalisables : mise en page et périmètre d'un calendrier.
--
-- `layout` : JSON `reports::layout::Layout` — gabarit (`detailed` ou
-- `summary`), papier (`a4` ou `letter`), sections incluses, titre, notes,
-- logo affiché ou non. `{}` : le rapport d'avant (détaillé, A4, tout inclus).
-- `scope` : JSON `reports::layout::Scope` — identifiants de moniteurs et noms
-- de groupes (union) ; `{}` : toute l'instance (ou tout le client).
ALTER TABLE report_schedules ADD COLUMN layout TEXT NOT NULL DEFAULT '{}';
ALTER TABLE report_schedules ADD COLUMN scope TEXT NOT NULL DEFAULT '{}';
