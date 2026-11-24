//! Gabarits `{{nom}}` des étapes : URL, en-têtes et corps.
//!
//! Trois formes de marque, toutes entre doubles accolades :
//!
//! * `{{nom}}` : une variable extraite par une étape précédente ;
//! * `{{secret.username}}`, `{{secret.password}}`, `{{secret.token}}` : une part
//!   de l'identifiant chiffré de la cible, jamais écrite dans les étiquettes ;
//! * l'une ou l'autre suivie d'un filtre, `{{secret.password|json}}` (échappé
//!   pour une chaîne JSON, sans les guillemets) ou `{{jeton|url}}` (encodé pour
//!   une URL ou un formulaire).
//!
//! L'analyse a lieu une fois, à la lecture des options : une marque mal écrite
//! est une erreur de configuration, signalée avant tout appel réseau.

use std::collections::HashMap;

/// Longueur maximale d'un nom de variable.
pub const MAX_NAME_LEN: usize = 64;

/// Part de l'identifiant de la cible qu'une marque `{{secret.…}}` désigne.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SecretField {
    Username,
    Password,
    Token,
}

impl SecretField {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "username" => Some(Self::Username),
            "password" => Some(Self::Password),
            "token" => Some(Self::Token),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Username => "username",
            Self::Password => "password",
            Self::Token => "token",
        }
    }
}

/// Mise en forme appliquée à la valeur substituée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    None,
    /// Échappement d'une chaîne JSON, sans les guillemets d'encadrement.
    Json,
    /// Encodage « pour cent » de tout ce qui n'est pas non réservé (RFC 3986).
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Part {
    Text(String),
    Var { name: String, filter: Filter },
    Secret { field: SecretField, filter: Filter },
}

/// Les valeurs secrètes disponibles pour la substitution.
#[derive(Debug, Clone, Copy, Default)]
pub struct Secrets<'a> {
    pub username: Option<&'a str>,
    pub password: Option<&'a str>,
    pub token: Option<&'a str>,
}

impl<'a> Secrets<'a> {
    pub fn get(&self, field: SecretField) -> Option<&'a str> {
        match field {
            SecretField::Username => self.username,
            SecretField::Password => self.password,
            SecretField::Token => self.token,
        }
    }
}

/// Texte rendu, et s'il contient un secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    pub uses_secret: bool,
}

/// Un texte saisi, découpé en morceaux fixes et en marques.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    parts: Vec<Part>,
}

impl Template {
    /// Découpe `raw`. `Err` décrit la marque fautive, sans reprendre de valeur.
    pub fn parse(raw: &str) -> Result<Self, String> {
        let mut parts = Vec::new();
        let mut rest = raw;
        while let Some(open) = rest.find("{{") {
            if open > 0 {
                parts.push(Part::Text(rest[..open].to_string()));
            }
            let after = &rest[open + 2..];
            let close =
                after.find("}}").ok_or_else(|| "a \"{{\" is never closed by \"}}\"".to_string())?;
            parts.push(parse_mark(after[..close].trim())?);
            rest = &after[close + 2..];
        }
        if !rest.is_empty() {
            parts.push(Part::Text(rest.to_string()));
        }
        Ok(Self { parts })
    }

    /// Vrai si le texte ne dépend d'aucune valeur : il peut être vérifié d'avance.
    pub fn is_static(&self) -> bool {
        self.parts.iter().all(|part| matches!(part, Part::Text(_)))
    }

    /// Les variables citées.
    pub fn variables(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().filter_map(|part| match part {
            Part::Var { name, .. } => Some(name.as_str()),
            _ => None,
        })
    }

    /// Les parts d'identifiant citées.
    pub fn secrets(&self) -> impl Iterator<Item = SecretField> {
        self.parts.iter().filter_map(|part| match part {
            Part::Secret { field, .. } => Some(*field),
            _ => None,
        })
    }

    /// Remplace chaque marque par sa valeur.
    ///
    /// Les absences ont été écartées à la configuration (voir
    /// `Scenario::check`) ; `Err` reste possible si l'appelant n'a pas fait la
    /// vérification, et nomme la marque sans en donner de valeur.
    pub fn render(
        &self,
        variables: &HashMap<String, String>,
        secrets: &Secrets<'_>,
    ) -> Result<Rendered, String> {
        let mut text = String::new();
        let mut uses_secret = false;
        for part in &self.parts {
            match part {
                Part::Text(fixed) => text.push_str(fixed),
                Part::Var { name, filter } => {
                    let value = variables
                        .get(name)
                        .ok_or_else(|| format!("variable \"{{{{{name}}}}}\" has no value"))?;
                    text.push_str(&apply(*filter, value));
                }
                Part::Secret { field, filter } => {
                    let value = secrets.get(*field).ok_or_else(|| {
                        format!("\"{{{{secret.{}}}}}\" is not in the credential", field.as_str())
                    })?;
                    uses_secret = true;
                    text.push_str(&apply(*filter, value));
                }
            }
        }
        Ok(Rendered { text, uses_secret })
    }
}

/// Lit le contenu d'une marque : `nom`, `secret.champ`, suivi ou non de `|filtre`.
fn parse_mark(inner: &str) -> Result<Part, String> {
    let (name, filter) = match inner.split_once('|') {
        Some((name, filter)) => (name.trim(), parse_filter(filter.trim())?),
        None => (inner, Filter::None),
    };
    if let Some(field) = name.strip_prefix("secret.") {
        let field = SecretField::parse(field).ok_or_else(|| {
            format!(
                "\"{{{{secret.{field}}}}}\" is unknown: use secret.username, secret.password \
                 or secret.token"
            )
        })?;
        return Ok(Part::Secret { field, filter });
    }
    if !valid_name(name) {
        return Err(format!(
            "\"{{{{{name}}}}}\" is not a valid variable: letters, digits and \"_\", \
             not starting with a digit"
        ));
    }
    Ok(Part::Var { name: name.to_string(), filter })
}

fn parse_filter(raw: &str) -> Result<Filter, String> {
    match raw {
        "json" => Ok(Filter::Json),
        "url" => Ok(Filter::Url),
        other => Err(format!("unknown filter \"{other}\": use \"json\" or \"url\"")),
    }
}

/// Un nom de variable : `[A-Za-z_][A-Za-z0-9_]*`, au plus [`MAX_NAME_LEN`]
/// caractères, et pas `secret`, réservé aux identifiants.
pub fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    name.len() <= MAX_NAME_LEN
        && name != "secret"
        && (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn apply(filter: Filter, value: &str) -> String {
    match filter {
        Filter::None => value.to_string(),
        Filter::Json => {
            let quoted = serde_json::Value::String(value.to_string()).to_string();
            quoted[1..quoted.len() - 1].to_string()
        }
        Filter::Url => crate::http::percent_encode(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn un_texte_sans_marque_reste_tel_quel() {
        let template = Template::parse("/api/health").unwrap();
        assert!(template.is_static());
        let rendered = template.render(&HashMap::new(), &Secrets::default()).unwrap();
        assert_eq!(rendered.text, "/api/health");
        assert!(!rendered.uses_secret);
    }

    #[test]
    fn les_variables_sont_remplacees_partout_ou_elles_figurent() {
        let template = Template::parse("/items/{{id}}?page={{ page }}&again={{id}}").unwrap();
        assert!(!template.is_static());
        assert_eq!(template.variables().collect::<Vec<_>>(), ["id", "page", "id"]);
        let rendered =
            template.render(&vars(&[("id", "42"), ("page", "3")]), &Secrets::default()).unwrap();
        assert_eq!(rendered.text, "/items/42?page=3&again=42");
    }

    #[test]
    fn les_secrets_sont_substitues_et_signales() {
        let raw = r#"{"user":"{{secret.username}}","pass":"{{secret.password|json}}"}"#;
        let template = Template::parse(raw).unwrap();
        assert_eq!(
            template.secrets().collect::<Vec<_>>(),
            [SecretField::Username, SecretField::Password]
        );
        let secrets = Secrets { username: Some("admin"), password: Some(r#"p"a\ss"#), token: None };
        let rendered = template.render(&HashMap::new(), &secrets).unwrap();
        assert_eq!(rendered.text, r#"{"user":"admin","pass":"p\"a\\ss"}"#);
        assert!(rendered.uses_secret);
    }

    #[test]
    fn le_filtre_url_encode_les_caracteres_reserves() {
        let template = Template::parse("q={{terme|url}}").unwrap();
        let rendered =
            template.render(&vars(&[("terme", "a b&c=d/é")]), &Secrets::default()).unwrap();
        assert_eq!(rendered.text, "q=a%20b%26c%3Dd%2F%C3%A9");
    }

    #[test]
    fn une_marque_mal_ecrite_est_refusee_a_la_lecture() {
        for raw in [
            "{{",
            "/a/{{id",
            "{{}}",
            "{{1abc}}",
            "{{a-b}}",
            "{{secret}}",
            "{{secret.api_key}}",
            "{{id|base64}}",
        ] {
            assert!(Template::parse(raw).is_err(), "« {raw} » aurait dû être refusé");
        }
    }

    #[test]
    fn une_valeur_absente_nest_pas_inventee() {
        let template = Template::parse("{{jeton}}").unwrap();
        let error = template.render(&HashMap::new(), &Secrets::default()).unwrap_err();
        assert!(error.contains("jeton"), "{error}");
        let secret = Template::parse("{{secret.token}}").unwrap();
        assert!(secret.render(&HashMap::new(), &Secrets::default()).is_err());
    }

    #[test]
    fn les_noms_de_variable_sont_bornes() {
        assert!(valid_name("jeton_2"));
        assert!(valid_name("_x"));
        assert!(!valid_name(""));
        assert!(!valid_name("2x"));
        assert!(!valid_name("secret"));
        assert!(!valid_name(&"a".repeat(MAX_NAME_LEN + 1)));
    }
}
