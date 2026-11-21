//! Constructeurs des tables : champs d'identifiants (`cred_*`) et réglages
//! (`text`, `number`, `boolean`, `select`…).

use super::*;

pub(super) fn no_items() -> Vec<OptionItem> {
    Vec::new()
}

pub(super) fn no_columns() -> Vec<OptionColumn> {
    Vec::new()
}

/// Range un réglage sous un sous-titre, dans la zone « Plus d'options » d'un
/// type qui en compte beaucoup (Proxmox VE…). Sans appel à cette fonction, un
/// réglage reste sans groupe et l'interface retombe sur un classement
/// générique par préfixe de clé.
pub(super) const fn grouped(view: OptionView, group: &'static str) -> OptionView {
    OptionView { group, ..view }
}

/// Champ visible (nom d'utilisateur, identifiant de jeton).
pub(super) const fn cred_text(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    placeholder: &'static str,
) -> CredentialField {
    CredentialField { key, label, help, placeholder, input: "text", choices: &[], required: true }
}

/// Champ masqué avec un bouton « afficher » : mot de passe, secret, phrase de passe.
pub(super) const fn cred_secret(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    placeholder: &'static str,
    required: bool,
) -> CredentialField {
    CredentialField { key, label, help, placeholder, input: "password", choices: &[], required }
}

pub(super) const fn cred_select(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    choices: &'static [&'static str],
) -> CredentialField {
    CredentialField { key, label, help, placeholder: "", input: "select", choices, required: false }
}

/// Champ texte libre.
pub(super) const fn text(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    placeholder: &'static str,
    default: &'static str,
) -> OptionView {
    OptionView {
        key,
        label,
        help,
        placeholder,
        default,
        required: false,
        input: "text",
        choices: &[],
        group: "",
        items: no_items,
        columns: no_columns,
    }
}

/// Champ numérique. Le défaut est transmis tel qu'écrit dans le code du collecteur.
pub(super) const fn number(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    placeholder: &'static str,
    default: &'static str,
) -> OptionView {
    OptionView {
        key,
        label,
        help,
        placeholder,
        default,
        required: false,
        input: "number",
        choices: &[],
        group: "",
        items: no_items,
        columns: no_columns,
    }
}

/// Case à cocher. Les collecteurs lisent `true`/`false` mais aussi `1`/`0`,
/// `oui`/`non` : le défaut est donné sous la forme canonique `true`/`false`.
pub(super) const fn boolean(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: bool,
) -> OptionView {
    OptionView {
        key,
        label,
        help,
        placeholder: "",
        default: if default { "true" } else { "false" },
        required: false,
        input: "boolean",
        choices: &[],
        group: "",
        items: no_items,
        columns: no_columns,
    }
}

/// Choix de l'agent installé sur le serveur de l'équipement : l'interface
/// propose les agents connus (`input: agent`) et enregistre l'identifiant de la
/// cible choisie dans l'étiquette `key`. Un type qui porte ce réglage ne se
/// surveille complètement qu'avec cet agent (voir `docs/devices/mdaemon.md`).
pub(super) const fn agent_link(
    key: &'static str,
    label: &'static str,
    help: &'static str,
) -> OptionView {
    OptionView {
        key,
        label,
        help,
        placeholder: "",
        default: "",
        required: false,
        input: "agent",
        choices: &[],
        group: "",
        items: no_items,
        columns: no_columns,
    }
}

/// Liste de choix fermée.
pub(super) const fn select(
    key: &'static str,
    label: &'static str,
    help: &'static str,
    default: &'static str,
    choices: &'static [&'static str],
) -> OptionView {
    OptionView {
        key,
        label,
        help,
        placeholder: "",
        default,
        required: false,
        input: "select",
        choices,
        group: "",
        items: no_items,
        columns: no_columns,
    }
}
