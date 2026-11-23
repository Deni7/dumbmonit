//! Validation et normalisation des identifiants et adresses saisis.

/// Vérifie la forme d'un identifiant : court, sans espace, en ASCII imprimable.
///
/// La règle est volontairement souple — un courriel passe — parce qu'un
/// fournisseur OIDC peut envoyer l'un ou l'autre et qu'on voudra les faire
/// correspondre.
pub fn validate_username(username: &str) -> Result<(), String> {
    let trimmed = username.trim();
    if trimmed.is_empty() {
        return Err("Enter a username.".into());
    }
    if trimmed.chars().count() > 64 {
        return Err("The username must be 64 characters or fewer.".into());
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("The username cannot contain spaces.".into());
    }
    Ok(())
}

/// Longueur maximale d'une adresse électronique (RFC 5321 : 254 utiles).
pub const EMAIL_MAX_LEN: usize = 254;

/// Vérifie et normalise une adresse électronique saisie : vide vaut « aucune ».
///
/// La vérification reste celle qu'un humain attend — une partie locale, un `@`,
/// un domaine avec un point, ni espace ni caractère de contrôle — sans prétendre
/// valider la RFC 5322 : l'adresse ne sert qu'à savoir qui joindre.
pub fn normalize_email(raw: &str) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let invalid = || format!("\"{trimmed}\" is not a valid email address.");
    if trimmed.chars().count() > EMAIL_MAX_LEN {
        return Err(format!("The email address must be {EMAIL_MAX_LEN} characters or fewer."));
    }
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid());
    }
    let Some((local, domain)) = trimmed.rsplit_once('@') else { return Err(invalid()) };
    let domain_ok = domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !domain.contains('@');
    if local.is_empty() || local.contains('@') || !domain_ok {
        return Err(invalid());
    }
    Ok(Some(format!("{local}@{}", domain.to_lowercase())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emails_are_optional_checked_and_normalised() {
        assert_eq!(normalize_email(""), Ok(None));
        assert_eq!(normalize_email("   "), Ok(None));
        assert_eq!(
            normalize_email(" Jane.Doe@Example.ORG "),
            Ok(Some("Jane.Doe@example.org".to_string()))
        );
        for bad in [
            "jane",
            "jane@",
            "@example.org",
            "jane@localhost",
            "ja ne@example.org",
            "jane@example..org",
            "jane@.example.org",
            "a@b@example.org",
        ] {
            assert!(normalize_email(bad).is_err(), "{bad}");
        }
        assert!(normalize_email(&format!("{}@example.org", "a".repeat(250))).is_err());
    }

    #[test]
    fn usernames_are_short_and_without_spaces() {
        assert!(validate_username("admin").is_ok());
        assert!(validate_username("jane.doe@example.org").is_ok());
        assert!(validate_username("").is_err());
        assert!(validate_username("jane doe").is_err());
        assert!(validate_username(&"a".repeat(65)).is_err());
    }
}
