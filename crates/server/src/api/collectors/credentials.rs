//! Formes d'identifiants des équipements d'infrastructure (SNMP, Proxmox,
//! Synology, Redfish, pare-feu, messagerie) et des sondes HTTP.

use super::*;

pub(super) const NO_AUTH: CredentialView = CredentialView {
    kind: "none",
    label: "No authentication",
    help: "Nothing is sent: the monitor answers without credentials.",
    fields: &[],
};

pub(super) const SNMP_COMMUNITY: CredentialView = CredentialView {
    kind: "snmp_community",
    label: "SNMP v1 / v2c (community)",
    help: "The simplest form: one shared word, sent unencrypted.",
    fields: &[cred_secret(
        "community",
        "SNMP community",
        "Most devices ship with \"public\". A read-only community is enough.",
        "public",
        true,
    )],
};

pub(super) const SNMP_V3: CredentialView = CredentialView {
    kind: "snmp_v3",
    label: "SNMP v3 (user, authentication, privacy)",
    help: "Authenticated and encrypted. Needs a USM user on the device.",
    fields: &[
        cred_text("username", "User name", "The SNMP v3 user configured on the device.", "monitor"),
        cred_select(
            "auth_protocol",
            "Authentication protocol",
            "Must match what the device was configured with.",
            &["sha256", "sha512", "sha384", "sha224", "sha1", "md5"],
        ),
        cred_secret(
            "auth_passphrase",
            "Authentication passphrase",
            "At least 8 characters, as set on the device.",
            "",
            true,
        ),
        cred_select(
            "privacy_protocol",
            "Privacy protocol",
            "Encryption of the SNMP traffic. \"none\", or a blank passphrase on a new monitor, \
             means authentication without encryption.",
            &["aes128", "aes192", "aes256", "des", "none"],
        ),
        cred_secret(
            "privacy_passphrase",
            "Privacy passphrase",
            "Leave blank for authentication without encryption (authNoPriv).",
            "",
            false,
        ),
    ],
};

/// Identifiant d'un jeton Proxmox (VE ou PBS) : `user@realm!nom`, tel que le
/// produit l'affiche à la création.
pub(super) const fn token_id_field(placeholder: &'static str) -> CredentialField {
    CredentialField {
        key: "token_id",
        label: "Token ID",
        help: "User, realm and token name, exactly as shown when the token was created.",
        placeholder,
        input: "text",
        choices: &[],
        required: true,
    }
}

/// Secret d'un jeton Proxmox : l'UUID montré une seule fois.
pub(super) const TOKEN_SECRET_FIELD: CredentialField = cred_secret(
    "secret",
    "Secret",
    "The UUID shown once when the token was created. Stored encrypted, never shown again.",
    "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx",
    true,
);

pub(super) const PROXMOX_TOKEN_LABEL: &str = "API token (recommended)";
pub(super) const PROXMOX_TOKEN_HELP: &str =
    "No expiry and no session opened: the right choice for monitoring.";

/// Jeton d'API Proxmox VE, en deux morceaux tels que le produit les affiche.
pub(super) const PROXMOX_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: PROXMOX_TOKEN_LABEL,
    help: PROXMOX_TOKEN_HELP,
    fields: &[token_id_field("dumbmonit@pve!monitor"), TOKEN_SECRET_FIELD],
};

pub(super) const PBS_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: PROXMOX_TOKEN_LABEL,
    help: PROXMOX_TOKEN_HELP,
    fields: &[token_id_field("dumbmonit@pbs!monitor"), TOKEN_SECRET_FIELD],
};

/// Nom d'utilisateur Proxmox, realm compris.
pub(super) const fn proxmox_user_field(placeholder: &'static str) -> CredentialField {
    cred_text("username", "User name", "User and realm, as in \"dumbmonit@pve\".", placeholder)
}

pub(super) const PROXMOX_LOGIN_LABEL: &str = "Username / password";
pub(super) const PROXMOX_LOGIN_HELP: &str =
    "Opens a two-hour session, renewed automatically. Use it only if tokens are not an option.";
pub(super) const PASSWORD_FIELD: CredentialField =
    cred_secret("password", "Password", "", "", true);

/// Connexion par mot de passe à l'API Proxmox VE : ouvre un ticket de deux heures.
pub(super) const PROXMOX_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: PROXMOX_LOGIN_LABEL,
    help: PROXMOX_LOGIN_HELP,
    fields: &[proxmox_user_field("dumbmonit@pve"), PASSWORD_FIELD],
};

/// Jeton d'API Proxmox Datacenter Manager. La console ne rend son ticket de
/// session que dans un cookie `HttpOnly` : le jeton est la seule forme
/// d'identifiant utilisable par une sonde, et de toute façon la bonne.
pub(super) const PDM_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: PROXMOX_TOKEN_LABEL,
    help: PROXMOX_TOKEN_HELP,
    fields: &[token_id_field("dumbmonit@pdm!monitor"), TOKEN_SECRET_FIELD],
};

/// Connexion à Proxmox Mail Gateway : un utilisateur et son mot de passe.
///
/// PMG 9 ne délivre pas de jeton d'API — c'est la seule des quatre consoles
/// Proxmox dans ce cas : le ticket est le mécanisme normal, et le seul que
/// l'interface propose de créer.
pub(super) const PMG_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password (recommended)",
    help: "Opens a two-hour session, renewed automatically. Proxmox Mail Gateway does not issue API tokens: this is its normal way in.",
    fields: &[
        cred_text(
            "username",
            "User name",
            "User and realm, as in \"dumbmonit@pmg\".",
            "dumbmonit@pmg",
        ),
        PASSWORD_FIELD,
    ],
};

/// Jeton d'API, pour une passerelle qui en accepterait un.
///
/// PMG 9 n'en crée pas ; l'option existe pour une version ultérieure ou une
/// passerelle placée derrière un frontal qui en attend un, et le collecteur
/// envoie alors `Authorization: PMGAPIToken=…` comme pour PVE et PBS.
pub(super) const PMG_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "API token",
    help: "Only if your gateway offers API tokens: Proxmox Mail Gateway 9 and earlier do not create any. Use the username and password instead.",
    fields: &[token_id_field("dumbmonit@pmg!monitor"), TOKEN_SECRET_FIELD],
};

pub(super) const PBS_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: PROXMOX_LOGIN_LABEL,
    help: PROXMOX_LOGIN_HELP,
    fields: &[proxmox_user_field("dumbmonit@pbs"), PASSWORD_FIELD],
};

pub(super) const SYNOLOGY_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "DSM account",
    help: "",
    fields: &[
        cred_text("username", "User name", "The DSM account created for monitoring.", "dumbmonit"),
        cred_secret(
            "password",
            "Password",
            "Two-step verification must be off for this account: a monitor cannot type a one-time code.",
            "",
            true,
        ),
    ],
};

/// Compte du contrôleur de gestion : utilisateur et mot de passe, envoyés en
/// Basic ou échangés contre une session selon l'option `auth`.
pub(super) const REDFISH_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Management controller account",
    help: "",
    fields: &[
        cred_text(
            "username",
            "User name",
            "The read-only account created on the controller for monitoring.",
            "dumbmonit",
        ),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Clé d'API OPNsense : le fichier téléchargé à la création contient `key=…`
/// et `secret=…`, et le pare-feu les attend en authentification HTTP « basic ».
pub(super) const OPNSENSE_KEY: CredentialView = CredentialView {
    kind: "username_password",
    label: "API key and secret",
    help: "The two lines of the file OPNsense downloads when the key is created.",
    fields: &[
        cred_text(
            "username",
            "API key",
            "The \"key\" line of the file OPNsense downloaded when you created the key.",
            "nJ8kQ2vF...",
        ),
        cred_secret(
            "password",
            "API secret",
            "The \"secret\" line of the same file. Stored encrypted, never shown again.",
            "",
            true,
        ),
    ],
};

/// Clé d'API TrueNAS, envoyée en `Authorization: Bearer`.
pub(super) const TRUENAS_KEY: CredentialView = CredentialView {
    kind: "api_token",
    label: "API key",
    help: "The key TrueNAS shows once when it is created, as \"3-AbCd…\".",
    fields: &[cred_secret(
        "token",
        "API key",
        "Starts with a number and a dash. Stored encrypted, never shown again.",
        "3-…",
        true,
    )],
};

/// Compte de l'API XML de MDaemon : adresse de messagerie complète et mot de
/// passe, envoyés en HTTP Basic. Facultatif : sans lui, seuls les services sont
/// vérifiés.
pub(super) const MDAEMON_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "XML API account (optional, version only)",
    help: "Also reads the version through the XML API. Leave the credential empty to watch the mail services only.",
    fields: &[
        cred_text(
            "username",
            "Email address",
            "The full email address of the account: MDaemon refuses a bare user name.",
            "dumbmonit@example.com",
        ),
        PASSWORD_FIELD,
    ],
};

/// Clé d'API REST de SecurityGateway (12.5 et suivants), envoyée en
/// `Authorization: Bearer`. Facultative : sans elle, seuls les services sont
/// vérifiés.
pub(super) const SECURITY_GATEWAY_KEY: CredentialView = CredentialView {
    kind: "api_token",
    label: "API key (12.5 and later, recommended)",
    help: "Also reads the version and the performance counters through the REST API. Leave the credential empty to watch the services only.",
    fields: &[cred_secret(
        "token",
        "API key",
        "Shown once when the key is created. Stored encrypted, never shown again.",
        "",
        true,
    )],
};

/// Pas d'identifiant : les services seuls.
pub(super) const MAIL_SERVICES_ONLY: CredentialView = CredentialView {
    kind: "none",
    label: "Services only",
    help: "Nothing is sent: DumbMonit connects to the mail ports and reads their greetings.",
    fields: &[],
};

pub(super) const HTTP_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "Bearer token",
    help: "Sent as \"Authorization: Bearer …\" on every request.",
    fields: &[cred_secret("token", "Token", "Stored encrypted, never shown again.", "", true)],
};

pub(super) const HTTP_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "Sent as HTTP basic authentication.",
    fields: &[
        cred_text("username", "User name", "", ""),
        cred_secret("password", "Password", "", "", true),
    ],
};

/// Les identifiants d'un scénario : rien n'est envoyé d'office, chaque étape les
/// place elle-même avec `{{secret.…}}`.
pub(super) const HTTP_STEPS_LOGIN: CredentialView = CredentialView {
    kind: "username_password",
    label: "Username / password",
    help: "Written where a step says {{secret.username}} and {{secret.password}}, for example in a login form or a JSON body.",
    fields: &[
        cred_text("username", "User name", "", ""),
        cred_secret("password", "Password", "Stored encrypted, never shown again.", "", true),
    ],
};

pub(super) const HTTP_STEPS_TOKEN: CredentialView = CredentialView {
    kind: "api_token",
    label: "Token",
    help: "Written where a step says {{secret.token}}, for example in an \"Authorization: Bearer {{secret.token}}\" header.",
    fields: &[cred_secret("token", "Token", "Stored encrypted, never shown again.", "", true)],
};
