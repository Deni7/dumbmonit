//! Catalogue des types de canaux : ce que l'interface doit demander pour chacun.
//!
//! L'interface ne connaît aucun service : elle rend les champs annoncés ici, et
//! c'est tout. Chaque entrée est relevée sur le constructeur du notificateur
//! correspondant — les clés lues dans `settings` et `secrets`, ce qui est
//! obligatoire, les valeurs admises, les défauts — et les tests du module
//! vérifient que le formulaire ainsi décrit suffit réellement à construire le
//! canal. Ajouter une clé dans un notificateur sans la déclarer ici la rend
//! inaccessible depuis l'interface : c'est ce que ces tests rappellent.
//!
//! Le contrat de [`Field`] et [`KindInfo`] est partagé avec l'interface
//! (`web/src/lib/api/types.ts`) : les noms de champs n'y changent pas.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;

use super::CHANNEL_KINDS;

mod chat;
mod custom;
mod direct;
mod gateways;
mod oncall;
mod tickets;

/// Un champ de formulaire.
#[derive(Debug, Clone, Serialize)]
pub struct Field {
    /// Clé dans `settings` ou `secrets`.
    pub key: &'static str,
    pub label: &'static str,
    pub required: bool,
    /// `text`, `url`, `number`, `boolean`, `select`, `textarea`, `password`.
    pub input: &'static str,
    pub help: &'static str,
    pub placeholder: &'static str,
    /// Valeurs proposées quand `input == "select"`.
    pub options: &'static [&'static str],
    /// Forme de la valeur attendue par le serveur : `scalar` (une chaîne, un nombre
    /// ou un booléen selon `input`), `list` (un tableau JSON de chaînes, une par
    /// ligne dans la zone de saisie) ou `object` (un objet JSON, saisi tel quel).
    ///
    /// Le notificateur ne lit une liste que sous forme de tableau : une chaîne
    /// « a, b » déposée dans `to` passerait pour « aucun destinataire ». C'est donc
    /// à l'interface de découper, et elle a besoin de savoir où.
    pub shape: &'static str,
    /// Valeur appliquée par le serveur quand le champ est laissé vide, vide s'il
    /// n'y en a pas. Sert à préremplir les listes de choix et à documenter le reste.
    pub default: &'static str,
    /// Vrai si le champ dit *où* partent les secrets (adresse du serveur, hôte
    /// SMTP, port, chiffrement, région). Le modifier impose de ressaisir les
    /// secrets : l'API refuse de les envoyer vers une destination qu'ils n'ont
    /// pas connue, et l'interface les affiche alors comme obligatoires.
    pub destination: bool,
    /// Liste de choix que l'interface va chercher elle-même, vide sinon :
    /// `oncall_policies` (politiques d'escalade) pour le canal d'astreinte. Une
    /// interface qui ne la connaît pas affiche le champ selon `input`.
    pub lookup: &'static str,
}

/// Un type de canal, prêt à être présenté.
#[derive(Debug, Clone, Serialize)]
pub struct KindInfo {
    pub kind: &'static str,
    pub label: &'static str,
    pub summary: &'static str,
    /// Lien absolu vers la section du canal dans la documentation en ligne
    /// (dumbmonit.selfhostr.io/docs), qui publie `docs/notifications.md`.
    pub doc_url: &'static str,
    pub settings: Vec<Field>,
    pub secrets: Vec<Field>,
    /// Comment le canal présente les liens d'action signés (acquitter, mettre
    /// en pause) : `buttons` (boutons natifs), `links` (liens dans le message)
    /// ou `none` (le service a son propre acquittement, PagerDuty, Opsgenie).
    /// Voir [`action_style`].
    pub actions: &'static str,
    /// Format des gabarits de message de ce type (`plain`, `slack_mrkdwn`,
    /// `html`, `telegram_markdown_v2`), vide s'il n'en prend pas. Voir
    /// [`super::message_template::format_for`].
    pub template_format: &'static str,
}

/// Page de la documentation en ligne qui décrit chaque canal ; chaque `doc_url`
/// en est une ancre.
pub const NOTIFICATIONS_DOC: &str = "https://dumbmonit.selfhostr.io/docs/notifications/";

/// Tous les types de canaux, dans l'ordre de [`CHANNEL_KINDS`] — celui du menu.
pub fn all() -> Vec<KindInfo> {
    CHANNEL_KINDS.iter().filter_map(|kind| describe(kind)).collect()
}

/// Clés que le catalogue range parmi les secrets d'un type de canal.
///
/// C'est ce que l'API consulte pour refuser qu'un secret soit déposé dans
/// `settings` : la liste vient du catalogue lui-même, si bien qu'un secret annoncé
/// à l'interface ne peut pas échapper au refus. Le catalogue est figé au premier
/// appel, ce qui donne des tranches statiques à un coût nul ensuite.
pub fn destination_keys(kind: &str) -> &'static [&'static str] {
    static BY_KIND: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    BY_KIND
        .get_or_init(|| {
            all()
                .into_iter()
                .map(|info| {
                    let keys = info.settings.iter().filter(|f| f.destination).map(|f| f.key);
                    (info.kind, keys.collect())
                })
                .collect()
        })
        .get(kind)
        .map_or(&[], Vec::as_slice)
}

/// Voir [`destination_keys`] : même mécanique, pour les secrets.
pub fn secret_keys(kind: &str) -> &'static [&'static str] {
    static BY_KIND: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    BY_KIND
        .get_or_init(|| {
            all()
                .into_iter()
                .map(|info| (info.kind, info.secrets.iter().map(|field| field.key).collect()))
                .collect()
        })
        .get(kind)
        .map_or(&[], Vec::as_slice)
}

// --------------------------------------------------------------------------
// Construction des champs
// --------------------------------------------------------------------------

/// Champ facultatif, scalaire, sans liste de choix : le cas général.
fn field(
    key: &'static str,
    label: &'static str,
    input: &'static str,
    help: &'static str,
    placeholder: &'static str,
) -> Field {
    Field {
        key,
        label,
        required: false,
        input,
        help,
        placeholder,
        options: &[],
        shape: "scalar",
        default: "",
        destination: false,
        lookup: "",
    }
}

fn text(key: &'static str, label: &'static str, help: &'static str, ph: &'static str) -> Field {
    field(key, label, "text", help, ph)
}

/// Une adresse est, par défaut, une destination ; voir [`Field::destination`].
fn url(key: &'static str, label: &'static str, help: &'static str, ph: &'static str) -> Field {
    field(key, label, "url", help, ph).destination(true)
}

fn password(key: &'static str, label: &'static str, help: &'static str, ph: &'static str) -> Field {
    field(key, label, "password", help, ph)
}

fn number(key: &'static str, label: &'static str, help: &'static str, ph: &'static str) -> Field {
    field(key, label, "number", help, ph)
}

fn textarea(key: &'static str, label: &'static str, help: &'static str, ph: &'static str) -> Field {
    field(key, label, "textarea", help, ph)
}

/// Liste de choix fermée ; le premier choix est toujours le défaut du serveur.
fn select(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    options: &'static [&'static str],
) -> Field {
    Field { options, default: options[0], ..field(key, label, "select", help, "") }
}

impl Field {
    fn destination(self, destination: bool) -> Self {
        Self { destination, ..self }
    }

    fn required(self) -> Self {
        Self { required: true, ..self }
    }

    fn lookup(self, lookup: &'static str) -> Self {
        Self { lookup, ..self }
    }

    fn with_default(self, default: &'static str) -> Self {
        Self { default, ..self }
    }

    /// Une valeur par ligne, transmise comme tableau JSON.
    fn list(self) -> Self {
        Self { shape: "list", input: "textarea", ..self }
    }

    /// Un objet JSON saisi tel quel.
    fn object(self) -> Self {
        Self { shape: "object", input: "textarea", ..self }
    }
}

/// Adresse de serveur, obligatoire : le champ le plus fréquent du catalogue.
fn server_url(help: &'static str, placeholder: &'static str) -> Field {
    url("server_url", "Server address", help, placeholder).required()
}

/// URL de webhook entrant, toujours secrète : elle contient le jeton.
fn webhook_url(help: &'static str, placeholder: &'static str) -> Field {
    password("webhook_url", "Webhook URL", help, placeholder).required()
}

fn info(
    kind: &'static str,
    label: &'static str,
    summary: &'static str,
    doc_url: &'static str,
    settings: Vec<Field>,
    secrets: Vec<Field>,
) -> KindInfo {
    let template_format = super::message_template::format_for(kind).map_or("", |f| f.as_str());
    KindInfo {
        kind,
        label,
        summary,
        doc_url,
        settings,
        secrets,
        actions: action_style(kind),
        template_format,
    }
}

// --------------------------------------------------------------------------
// Le catalogue
// --------------------------------------------------------------------------

/// Décrit un type de canal, `None` s'il n'est pas catalogué.
///
/// Chaque bras reflète le constructeur du notificateur du même nom ; le test
/// `chaque_type_de_canal_est_catalogue` garantit qu'aucun bras ne manque.
fn describe(kind: &'static str) -> Option<KindInfo> {
    let mut info = describe_base(kind)?;
    if supports_insecure_tls(kind) {
        info.settings.push(insecure_tls_field());
    }
    Some(info)
}

/// Présentation des liens d'action d'un type de canal (voir [`KindInfo::actions`]).
///
/// `none` les retire du message ; `buttons` et `links` les laissent au
/// notificateur, qui les rend lui-même ou les reçoit en fin de texte
/// ([`super::Notifier::renders_actions`]).
pub fn action_style(kind: &str) -> &'static str {
    match kind {
        // Astreinte avec son propre acquittement, et SMS (un lien signé de ~150
        // caractères coûterait plusieurs segments).
        "pagerduty"
        | "opsgenie"
        | "grafana_oncall"
        | "signl4"
        | "vonage"
        | "telnyx"
        | "aspsms"
        | crate::oncall::CHANNEL_KIND
        | "jira"
        | "glpi"
        | "zammad"
        | "github_issues"
        | "gitlab_issues" => "none",
        "ntfy" | "telegram" | super::webpush::KIND => "buttons",
        _ => "links",
    }
}

/// Types de canaux dont le serveur est typiquement auto-hébergé, et qui acceptent
/// donc le réglage facultatif [`INSECURE_TLS_KEY`] (certificat auto-signé ou d'une
/// autorité privée). Les services publics (Discord, Slack, Pushover…) n'en ont
/// pas : leur certificat est toujours valide, un contournement n'y servirait qu'à
/// affaiblir la connexion.
pub fn supports_insecure_tls(kind: &str) -> bool {
    matches!(
        kind,
        "matrix"
            | "mattermost"
            | "rocketchat"
            | "zulip"
            | "ntfy"
            | "gotify"
            | "bark"
            | "apprise"
            | "homeassistant"
            | "signal"
            | "webhook"
            | "jira"
            | "glpi"
            | "zammad"
            | "github_issues"
            | "gitlab_issues"
    )
}

/// Clé du réglage booléen, dans `settings`.
pub const INSECURE_TLS_KEY: &str = "insecure_tls";

/// Réglage « Interactive buttons » (voir [`super::interactive`]).
fn interactive_field(help: &'static str) -> Field {
    field(super::interactive::INTERACTIVE_KEY, "Interactive buttons", "boolean", help, "")
}

fn insecure_tls_field() -> Field {
    field(
        INSECURE_TLS_KEY,
        "Accept an unverifiable certificate",
        "boolean",
        "Off by default. Turn it on only for a server of yours with a self-signed or \
         private-CA certificate: the connection stays encrypted, but the server's identity \
         is no longer checked.",
        "",
    )
}

fn describe_base(kind: &'static str) -> Option<KindInfo> {
    chat::describe(kind)
        .or_else(|| direct::describe(kind))
        .or_else(|| gateways::describe(kind))
        .or_else(|| oncall::describe(kind))
        .or_else(|| tickets::describe(kind))
        .or_else(|| custom::describe(kind))
}

#[cfg(test)]
mod tests;
