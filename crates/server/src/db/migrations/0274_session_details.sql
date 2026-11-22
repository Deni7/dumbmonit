-- Ce que l'on retient d'une session pour la reconnaître dans « mes sessions » :
-- dernière activité (rafraîchie au plus toutes les cinq minutes), adresse vue
-- en dernier, navigateur (`User-Agent` tronqué) et façon dont elle a été
-- ouverte (`password`, `passkey`, `ldap`, `oidc`, `saml`). Toutes facultatives :
-- les sessions ouvertes avant cette migration restent valides, sans détail.
ALTER TABLE auth_sessions ADD COLUMN last_seen_at TEXT;
ALTER TABLE auth_sessions ADD COLUMN ip TEXT;
ALTER TABLE auth_sessions ADD COLUMN user_agent TEXT;
ALTER TABLE auth_sessions ADD COLUMN via TEXT;
