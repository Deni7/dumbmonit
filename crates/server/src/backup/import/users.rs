//! Comptes, jetons et équipes.

use super::*;

pub(super) async fn restore_users(
    tx: &mut Transaction<'_, Sqlite>,
    cipher: &Cipher,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("users");
    for user in &bundle.users {
        if crate::auth::users::Role::parse(&user.role).is_none() {
            report.refused(
                &format!("Account \"{}\"", user.username),
                &anyhow::anyhow!("unknown role \"{}\"", user.role),
            );
            continue;
        }

        let existing = sqlx::query("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
            .bind(&user.username)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche du compte")?;
        if existing.is_some() {
            // Jamais écrasé : un fichier ne doit pas pouvoir changer le rôle ou
            // le mot de passe d'un compte existant.
            report.skipped += 1;
            report.notes.push(format!(
                "Account \"{}\" already exists here and was left untouched.",
                user.username
            ));
            continue;
        }

        // `oidc_subject` est unique : s'il est déjà pris, le compte est créé
        // sans son rattachement SSO plutôt que refusé en entier.
        let mut subject = user.oidc_subject.clone();
        if let Some(value) = &subject {
            let taken = sqlx::query("SELECT 1 FROM users WHERE oidc_subject = ?")
                .bind(value)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche du rattachement SSO")?;
            if taken.is_some() {
                report.notes.push(format!(
                    "Account \"{}\": its SSO identity already belongs to another account here; \
                     it was restored without the link.",
                    user.username
                ));
                subject = None;
            }
        }

        // Même règle pour l'identité SAML, unique par IdP.
        let mut saml = user.saml_subject.clone().zip(user.saml_issuer.clone());
        if let Some((subject, issuer)) = &saml {
            let taken =
                sqlx::query("SELECT 1 FROM users WHERE saml_subject = ? AND saml_issuer = ?")
                    .bind(subject)
                    .bind(issuer)
                    .fetch_optional(&mut **tx)
                    .await
                    .context("recherche du rattachement SAML")?;
            if taken.is_some() {
                report.notes.push(format!(
                    "Account \"{}\": its SAML identity already belongs to another account here; \
                     it was restored without the link.",
                    user.username
                ));
                saml = None;
            }
        }
        let (saml_subject, saml_issuer) = saml.unzip();

        // `scim_id` est unique : déjà pris, le compte est restauré sans son
        // rattachement SCIM, que le fournisseur refera.
        let mut scim_id = user.scim_id.clone();
        if let Some(value) = &scim_id {
            let taken = sqlx::query("SELECT 1 FROM users WHERE scim_id = ?")
                .bind(value)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche du rattachement SCIM")?;
            if taken.is_some() {
                report.notes.push(format!(
                    "Account \"{}\": its SCIM identity already belongs to another account here; \
                     it was restored without the link.",
                    user.username
                ));
                scim_id = None;
            }
        }

        let totp_secret = user
            .totp_secret
            .as_ref()
            .and_then(|raw| BASE64.decode(raw).ok())
            .map(|plain| cipher.encrypt(&plain))
            .transpose()?;

        let row = sqlx::query(
            "INSERT INTO users (username, display_name, role, password_hash, oidc_subject,
                                oidc_issuer, saml_subject, saml_issuer, disabled, totp_secret,
                                totp_enabled, scim_id, scim_external_id, scim_email,
                                disabled_by_scim)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(&user.role)
        .bind(&user.password_hash)
        .bind(&subject)
        .bind(&user.oidc_issuer)
        .bind(&saml_subject)
        .bind(&saml_issuer)
        .bind(i64::from(user.disabled))
        .bind(totp_secret.as_deref())
        .bind(i64::from(user.totp_enabled && totp_secret.is_some()))
        .bind(&scim_id)
        .bind(scim_id.as_ref().and(user.scim_external_id.as_ref()))
        .bind(scim_id.as_ref().and(user.scim_email.as_ref()))
        .bind(i64::from(user.disabled && user.disabled_by_scim))
        .fetch_one(&mut **tx)
        .await
        .context("création du compte")?;
        let id: i64 = row.try_get("id")?;

        for code in &user.recovery_codes {
            let Ok(hash) = BASE64.decode(code) else { continue };
            sqlx::query("INSERT INTO totp_recovery_codes (user_id, code_hash) VALUES (?, ?)")
                .bind(id)
                .bind(hash)
                .execute(&mut **tx)
                .await
                .context("création d'un code de secours")?;
        }
        report.created += 1;
    }
    Ok(report)
}

pub(super) async fn restore_agent_tokens(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("agent_tokens");
    for token in &bundle.agent_tokens {
        let existing = sqlx::query("SELECT 1 FROM agent_tokens WHERE token_hash = ?")
            .bind(&token.token_hash)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche du jeton d'agent")?;
        if existing.is_some() {
            report.skipped += 1;
            continue;
        }
        // Les jetons d'enrôlement n'ont plus de nom lisible : une sauvegarde
        // ancienne est normalisée en identifiant opaque.
        let name = if agent::is_token_name(&token.name) {
            token.name.clone()
        } else {
            loop {
                let candidate = agent::generate_token_name();
                let taken = sqlx::query("SELECT 1 FROM agent_tokens WHERE name = ?")
                    .bind(&candidate)
                    .fetch_optional(&mut **tx)
                    .await
                    .context("recherche du nom du jeton d'agent")?;
                if taken.is_none() {
                    break candidate;
                }
            }
        };
        sqlx::query(
            "INSERT INTO agent_tokens (name, token_hash, prefix, created_at, revoked_at)
             VALUES (?, ?, ?, COALESCE(?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')), ?)",
        )
        .bind(&name)
        .bind(&token.token_hash)
        .bind(&token.prefix)
        .bind(&token.created_at)
        .bind(&token.revoked_at)
        .execute(&mut **tx)
        .await
        .context("création du jeton d'agent")?;
        report.created += 1;
    }
    Ok(report)
}

pub(super) async fn restore_api_tokens(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("api_tokens");
    for token in &bundle.api_tokens {
        let Ok(hash) = BASE64.decode(&token.token_hash) else {
            report.refused(
                &format!("API token \"{}\"", token.name),
                &anyhow::anyhow!("its fingerprint is not valid base64"),
            );
            continue;
        };
        let existing = sqlx::query("SELECT 1 FROM api_tokens WHERE token_hash = ?")
            .bind(&hash)
            .fetch_optional(&mut **tx)
            .await
            .context("recherche du jeton d'API")?;
        if existing.is_some() {
            report.skipped += 1;
            continue;
        }
        let owner: Option<i64> = match &token.user {
            None => None,
            Some(username) => sqlx::query("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
                .bind(username)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche du propriétaire du jeton")?
                .map(|row| row.try_get("id"))
                .transpose()?,
        };
        match sqlx::query(
            "INSERT INTO api_tokens (name, prefix, token_hash, scope, created_at, revoked_at,
                                     user_id)
             VALUES (?, ?, ?, ?, COALESCE(?, strftime('%Y-%m-%dT%H:%M:%SZ', 'now')), ?, ?)",
        )
        .bind(&token.name)
        .bind(&token.prefix)
        .bind(&hash)
        .bind(&token.scope)
        .bind(&token.created_at)
        .bind(&token.revoked_at)
        .bind(owner)
        .execute(&mut **tx)
        .await
        {
            Ok(_) => report.created += 1,
            Err(error) => report.refused(&format!("API token \"{}\"", token.name), &error.into()),
        }
    }
    Ok(report)
}

/// Équipes : créées ou mises à jour par nom. Les droits sont remplacés par ceux
/// du lot ; les membres du lot sont ajoutés (un compte absent est signalé), sans
/// retirer ceux que l'instance a déjà.
pub(super) async fn restore_teams(
    tx: &mut Transaction<'_, Sqlite>,
    bundle: &Bundle,
) -> Result<SectionReport> {
    let mut report = SectionReport::new("teams");
    if let Some(settings) = &bundle.teams_settings
        && serde_json::from_value::<crate::auth::teams::TeamSettings>(settings.clone()).is_ok()
    {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES (?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(crate::auth::teams::SETTINGS_KEY)
        .bind(serde_json::to_vec(settings)?)
        .execute(&mut **tx)
        .await
        .context("réglages des équipes")?;
    }
    for team in &bundle.teams {
        let Ok(name) = crate::auth::teams::validate_name(&team.name) else {
            report.skipped += 1;
            report.notes.push(format!("team \"{}\" has no valid name", team.name));
            continue;
        };
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT id FROM teams WHERE name = ? COLLATE NOCASE")
                .bind(&name)
                .fetch_optional(&mut **tx)
                .await
                .context("recherche de l'équipe")?;
        let id: i64 = match existing {
            Some(id) => {
                sqlx::query("UPDATE teams SET description = ?, idp_groups = ? WHERE id = ?")
                    .bind(&team.description)
                    .bind(serde_json::to_string(&team.idp_groups)?)
                    .bind(id)
                    .execute(&mut **tx)
                    .await
                    .context("mise à jour de l'équipe")?;
                report.count(RestoreOutcome::Updated);
                id
            }
            None => {
                let id = sqlx::query_scalar(
                    "INSERT INTO teams (name, description, idp_groups) VALUES (?, ?, ?)
                     RETURNING id",
                )
                .bind(&name)
                .bind(&team.description)
                .bind(serde_json::to_string(&team.idp_groups)?)
                .fetch_one(&mut **tx)
                .await
                .context("création de l'équipe")?;
                report.count(RestoreOutcome::Created);
                id
            }
        };
        // Rattachement SCIM : repris seulement sur une équipe qui n'en a pas, et
        // si aucune autre ne porte déjà cet identifiant.
        if let Some(scim_id) = &team.scim_id {
            sqlx::query(
                "UPDATE teams SET scim_id = ?, scim_external_id = ?, scim_created = ?
                 WHERE id = ? AND scim_id IS NULL
                   AND NOT EXISTS (SELECT 1 FROM teams WHERE scim_id = ?)",
            )
            .bind(scim_id)
            .bind(&team.scim_external_id)
            .bind(i64::from(team.scim_created))
            .bind(id)
            .bind(scim_id)
            .execute(&mut **tx)
            .await
            .context("rattachement SCIM de l'équipe")?;
        }
        sqlx::query("DELETE FROM team_folder_grants WHERE team_id = ?")
            .bind(id)
            .execute(&mut **tx)
            .await
            .context("droits de l'équipe")?;
        for grant in &team.grants {
            sqlx::query(
                "INSERT INTO team_folder_grants (team_id, folder, level) VALUES (?, ?, ?)
                 ON CONFLICT(team_id, folder) DO UPDATE SET level = excluded.level",
            )
            .bind(id)
            .bind(crate::auth::teams::normalize_folder(&grant.folder))
            .bind(grant.level.as_str())
            .execute(&mut **tx)
            .await
            .context("droit de l'équipe")?;
        }
        for member in &team.members {
            let source = match member.source.as_str() {
                "idp" | "scim" => member.source.as_str(),
                _ => "manual",
            };
            let user: Option<i64> =
                sqlx::query_scalar("SELECT id FROM users WHERE username = ? COLLATE NOCASE")
                    .bind(&member.username)
                    .fetch_optional(&mut **tx)
                    .await
                    .context("membre de l'équipe")?;
            let Some(user) = user else {
                report.notes.push(format!(
                    "team \"{name}\": account \"{}\" not found, membership skipped",
                    member.username
                ));
                continue;
            };
            sqlx::query(
                "INSERT INTO team_members (team_id, user_id, source) VALUES (?, ?, ?)
                 ON CONFLICT(team_id, user_id) DO NOTHING",
            )
            .bind(id)
            .bind(user)
            .bind(source)
            .execute(&mut **tx)
            .await
            .context("membre de l'équipe")?;
        }
    }
    Ok(report)
}
