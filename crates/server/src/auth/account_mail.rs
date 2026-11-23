//! Courriels des liens de compte : réinitialisation du mot de passe et
//! invitation. Envoyés par le serveur de courriel de l'instance
//! (`notify::mail_server`), en texte brut, en anglais comme les autres
//! courriels du serveur.

use sqlx::SqlitePool;

use crate::auth::users::Role;
use crate::crypto::Cipher;
use crate::notify::smtp::Smtp;

/// Un courriel prêt à partir.
pub struct Letter {
    pub subject: String,
    pub body: String,
}

/// Vrai quand le serveur de courriel de l'instance est réglé.
pub async fn ready(pool: &SqlitePool, cipher: &Cipher) -> bool {
    crate::notify::mail_server::channel(pool, cipher).await.is_some()
}

/// Envoie un courriel par le serveur de l'instance. L'erreur, déjà expurgée
/// de tout identifiant SMTP, est rendue lisible pour l'administrateur.
pub async fn send(
    pool: &SqlitePool,
    cipher: &Cipher,
    to: &str,
    letter: Letter,
) -> Result<(), String> {
    let Some(channel) = crate::notify::mail_server::channel(pool, cipher).await else {
        return Err("No email server is configured.".to_string());
    };
    let smtp = Smtp::sender(&channel).map_err(|error| error.to_string())?;
    smtp.send_to(to, &letter.subject, letter.body, None).await.map_err(|error| error.to_string())
}

/// Nom de l'instance, tel qu'il figure dans les courriels.
pub async fn instance_name(pool: &SqlitePool) -> String {
    plain(&crate::instance::load(pool).await.name).unwrap_or_else(|| "DumbMonit".to_string())
}

/// Texte d'une seule ligne, sans caractère de contrôle ; `None` s'il est vide.
fn plain(raw: &str) -> Option<String> {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).take(120).collect();
    let cleaned = cleaned.trim();
    (!cleaned.is_empty()).then(|| cleaned.to_string())
}

/// « 30 minutes », « 7 days », « 1 hour ».
pub fn duration_text(minutes: u32) -> String {
    let (count, unit) = if minutes.is_multiple_of(24 * 60) {
        (minutes / (24 * 60), "day")
    } else if minutes.is_multiple_of(60) {
        (minutes / 60, "hour")
    } else {
        (minutes, "minute")
    };
    if count == 1 { format!("1 {unit}") } else { format!("{count} {unit}s") }
}

/// Le courriel « mot de passe oublié ».
pub fn reset_letter(instance: &str, username: &str, url: &str, minutes: u32) -> Letter {
    let instance = plain(instance).unwrap_or_else(|| "DumbMonit".to_string());
    let username = plain(username).unwrap_or_default();
    let lasts = duration_text(minutes);
    Letter {
        subject: format!("Reset your {instance} password"),
        body: format!(
            "Hello {username},\n\n\
             Someone asked to reset the password of your account on {instance}. If it was \
             you, open this link to choose a new one:\n\n\
             {url}\n\n\
             The link works once and expires in {lasts}. Using it signs your account out \
             everywhere. Two-factor authentication, if you use it, stays on.\n\n\
             If you did not ask for this, ignore this email: your password does not change.\n"
        ),
    }
}

fn role_text(role: Role) -> &'static str {
    match role {
        Role::Admin => "an administrator",
        Role::Operator => "an operator (you can handle alerts)",
        Role::Viewer => "a viewer (read only)",
        Role::Wall => "a wall display",
    }
}

/// Le courriel d'invitation.
pub fn invite_letter(instance: &str, inviter: &str, role: Role, url: &str, minutes: u32) -> Letter {
    let instance = plain(instance).unwrap_or_else(|| "DumbMonit".to_string());
    let inviter = plain(inviter).unwrap_or_else(|| "An administrator".to_string());
    let lasts = duration_text(minutes);
    let role = role_text(role);
    Letter {
        subject: format!("You are invited to {instance}"),
        body: format!(
            "Hello,\n\n\
             {inviter} invites you to join {instance} as {role}.\n\n\
             Open this link to create your account:\n\n\
             {url}\n\n\
             The invitation works once and expires in {lasts}. If you were not expecting \
             it, ignore this email.\n"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_naturally() {
        assert_eq!(duration_text(30), "30 minutes");
        assert_eq!(duration_text(60), "1 hour");
        assert_eq!(duration_text(90), "90 minutes");
        assert_eq!(duration_text(7 * 24 * 60), "7 days");
        assert_eq!(duration_text(24 * 60), "1 day");
    }

    #[test]
    fn letters_carry_the_link_and_no_header_injection() {
        let letter = reset_letter("Home\r\nBcc: x@evil.example", "jane", "https://m/reset", 30);
        assert!(!letter.subject.contains('\n') && !letter.subject.contains('\r'));
        assert!(letter.body.contains("https://m/reset"));
        assert!(letter.body.contains("30 minutes"));
        let invite = invite_letter("", "Ada", Role::Operator, "https://m/invite", 7 * 24 * 60);
        assert_eq!(invite.subject, "You are invited to DumbMonit");
        assert!(invite.body.contains("Ada invites you"));
        assert!(invite.body.contains("7 days"));
    }
}
