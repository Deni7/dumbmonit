//! Gabarits de message par canal : un titre et un corps réécrits par
//! l'utilisateur, avec des variables (`{{monitor}}`, `{{tags.room}}`…).
//!
//! Sans gabarit enregistré, rien ne change : le message part tel que
//! [`crate::notify::message::render`] l'a écrit. Les gabarits par défaut
//! ([`DEFAULT_TITLE`], [`DEFAULT_BODY`]) redonnent exactement ce texte : c'est
//! ce que l'interface propose au départ et ce que « Rétablir » remet.
//!
//! Le moteur est celui du canal personnalisé ([`crate::notify::template`]) :
//! des variables et rien d'autre — ni boucle, ni condition, ni expression. Ce
//! qui change ici est l'échappement, propre au format que le service lit
//! ([`TextFormat`]) : une valeur (un nom d'équipement, une étiquette) ne peut
//! jamais ajouter de mise en forme ni casser le message, le texte écrit par
//! l'utilisateur reste le sien.

use std::borrow::Cow;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::notify::action_link::Action;
use crate::notify::channel::ChannelConfig;
use crate::notify::error::NotifyError;
use crate::notify::message::{Message, format_duration, format_value};
use crate::notify::template::{Part, Template};

/// Clés des gabarits dans les réglages du canal (`settings`, en clair : un
/// gabarit n'est pas un secret). Distinctes du `body_template` du webhook.
pub const TITLE_KEY: &str = "message_title_template";
pub const BODY_KEY: &str = "message_body_template";
/// Gabarits par défaut : le titre et le corps habituels, lien compris.
pub const DEFAULT_TITLE: &str = "{{title}}";
pub const DEFAULT_BODY: &str = "{{summary}}\n{{url}}";
/// Longueur maximale d'un gabarit, en caractères.
pub const MAX_TEMPLATE_LEN: usize = 4000;
/// Préfixe des étiquettes de l'équipement (`{{tags.room}}`).
const TAG_PREFIX: &str = "tags.";

/// Variables des gabarits de message, avec leur description (documentation et
/// formulaire). `tags.<key>` désigne toute étiquette de l'équipement.
pub const MESSAGE_VARIABLES: &[(&str, &str)] = &[
    ("title", "The built-in title, for example \"nas — Disk full\""),
    ("monitor", "Name of the monitored device"),
    ("severity", "Severity as the interface says it: advisory, warning or critical"),
    ("state", "firing or resolved"),
    ("summary", "The built-in body: one line per alert, without the link"),
    ("value", "Measured value with its unit; empty if the alert carries none"),
    ("threshold", "Crossed threshold with its unit"),
    ("rule", "Name of the alert rule"),
    ("url", "Link to the device in DumbMonit; empty without a public URL"),
    ("started_at", "When the alert started firing, for example 2026-09-01 14:32 UTC"),
    ("duration", "How long it has been firing when the message leaves, for example 12 min"),
    ("site", "The device's \"site\" tag, or its \"location\" tag"),
    ("count", "Number of alerts in this message"),
    ("date", "When the message was sent, in UTC"),
    ("tags.<key>", "Any tag of the device, for example {{tags.room}}"),
];

/// Format du texte qu'un service lit, et donc de l'échappement des valeurs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    /// Texte brut : les valeurs passent telles quelles.
    Plain,
    /// `mrkdwn` de Slack : `&`, `<` et `>` échappés en entités.
    SlackMrkdwn,
    /// HTML (corps du courriel) : les cinq caractères réservés échappés.
    Html,
    /// MarkdownV2 de Telegram : chaque caractère réservé précédé de `\`.
    TelegramMarkdownV2,
}

impl TextFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::SlackMrkdwn => "slack_mrkdwn",
            Self::Html => "html",
            Self::TelegramMarkdownV2 => "telegram_markdown_v2",
        }
    }
}

/// Format des gabarits d'un type de canal, `None` s'il n'en prend pas : les
/// tickets et l'astreinte intégrée ne reçoivent pas un message, et le webhook
/// personnalisé a déjà son propre gabarit, plus complet.
pub fn format_for(kind: &str) -> Option<TextFormat> {
    match kind {
        "slack" => Some(TextFormat::SlackMrkdwn),
        "telegram" => Some(TextFormat::TelegramMarkdownV2),
        "smtp" => Some(TextFormat::Html),
        "webhook" | crate::oncall::CHANNEL_KIND => None,
        kind if crate::notify::tickets::is_ticket_kind(kind) => None,
        _ => Some(TextFormat::Plain),
    }
}

/// Ce que les gabarits et les boutons interactifs lisent d'un message en plus
/// de ses champs habituels.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageContext {
    /// Déclenchement de la première alerte du message.
    pub started_at: Option<DateTime<Utc>>,
    /// Étiquettes de l'équipement (vides pour un résumé de plusieurs).
    pub tags: BTreeMap<String, String>,
    /// Rendu du gabarit du canal, dans le format du service. `None` : le
    /// message n'a pas de gabarit, le canal le met en forme lui-même.
    pub rendered: Option<Rendered>,
    /// Références courtes des boutons Telegram (`dm:<aléa>`), par geste,
    /// posées une fois les jetons enregistrés (voir
    /// [`crate::notify::interactive::telegram`]).
    pub callbacks: Vec<(Action, String)>,
}

/// Titre et corps rendus par le gabarit du canal.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub format: TextFormat,
    pub title: String,
    pub body: String,
}

/// Les gabarits d'un canal, analysés. L'un ou l'autre peut manquer : le défaut
/// le remplace.
#[derive(Debug)]
pub struct MessageTemplates {
    title: Option<Template>,
    body: Option<Template>,
}

fn is_known(name: &str) -> bool {
    if let Some(key) = name.strip_prefix(TAG_PREFIX) {
        return !key.is_empty()
            && key.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'));
    }
    MESSAGE_VARIABLES.iter().any(|(known, _)| *known == name && !known.starts_with(TAG_PREFIX))
}

fn available() -> String {
    MESSAGE_VARIABLES.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ")
}

fn parse_one(source: Option<&str>, field: &str) -> Result<Option<Template>, NotifyError> {
    let Some(source) = source.filter(|source| !source.trim().is_empty()) else {
        return Ok(None);
    };
    if source.chars().count() > MAX_TEMPLATE_LEN {
        return Err(NotifyError::Config(format!(
            "\"{field}\": a template is limited to {MAX_TEMPLATE_LEN} characters"
        )));
    }
    Template::parse_with(source, field, is_known, &available()).map(Some)
}

fn read_text<'a>(settings: &'a Value, key: &str) -> Result<Option<&'a str>, NotifyError> {
    match settings.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.as_str())),
        Some(_) => Err(NotifyError::Config(format!("\"{key}\" must be text"))),
    }
}

/// Un gabarit intégré, analysé à chaque usage (quelques octets).
fn builtin(source: &str) -> Template {
    Template::parse_with(source, "default", is_known, "").expect("built-in templates are valid")
}

impl MessageTemplates {
    /// Analyse les gabarits saisis ; `None` quand les deux sont vides (le
    /// message habituel, sans détour par le moteur).
    pub fn parse(title: Option<&str>, body: Option<&str>) -> Result<Option<Self>, NotifyError> {
        let title = parse_one(title, TITLE_KEY)?;
        let body = parse_one(body, BODY_KEY)?;
        if title.is_none() && body.is_none() {
            return Ok(None);
        }
        Ok(Some(Self { title, body }))
    }

    /// Les gabarits enregistrés dans les réglages d'un canal.
    pub fn from_settings(settings: &Value) -> Result<Option<Self>, NotifyError> {
        Self::parse(read_text(settings, TITLE_KEY)?, read_text(settings, BODY_KEY)?)
    }

    /// Titre et corps rendus pour `message`, dans `format`.
    pub fn render(&self, message: &Message, format: TextFormat) -> Rendered {
        let variables = Variables::new(message);
        let title = match &self.title {
            Some(template) => render(template, &variables, format),
            None => render(&builtin(DEFAULT_TITLE), &variables, format),
        };
        let body = match &self.body {
            Some(template) => render(template, &variables, format),
            None => render(&builtin(DEFAULT_BODY), &variables, format),
        };
        // Un titre tient sur une ligne : objet de courriel, titre de carte.
        let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
        Rendered { format, title, body }
    }
}

/// Le message tel que ce canal doit le recevoir, son gabarit appliqué.
///
/// `title`, `text` et `markdown` prennent le rendu, si bien que les canaux qui
/// ne connaissent pas les gabarits les envoient sans rien changer ; ceux dont
/// le format a des règles propres (Telegram, courriel) lisent
/// `context.rendered`. Un gabarit devenu illisible (restauré d'ailleurs) laisse
/// le message habituel : mieux vaut l'ancien texte que pas d'alerte.
pub fn apply<'a>(config: &ChannelConfig, message: &'a Message) -> Cow<'a, Message> {
    let Some(format) = format_for(&config.kind) else { return Cow::Borrowed(message) };
    let templates = match MessageTemplates::from_settings(&config.settings) {
        Ok(Some(templates)) => templates,
        Ok(None) => return Cow::Borrowed(message),
        Err(error) => {
            tracing::warn!(channel = %config.name, %error, "message template ignored");
            return Cow::Borrowed(message);
        }
    };
    Cow::Owned(templated(&templates, message, format))
}

/// Applique des gabarits déjà analysés (aussi l'aperçu du formulaire).
pub fn templated(templates: &MessageTemplates, message: &Message, format: TextFormat) -> Message {
    let rendered = templates.render(message, format);
    let mut out = message.clone();
    if format == TextFormat::Html {
        // Le courriel part en deux versions : le texte brut, et le HTML.
        let plain = templates.render(message, TextFormat::Plain);
        out.title = plain.title;
        out.text = plain.body.clone();
        out.markdown = plain.body;
    } else {
        out.title = rendered.title.clone();
        out.text = rendered.body.clone();
        out.markdown = rendered.body.clone();
    }
    out.context.rendered = Some(rendered);
    out
}

// --------------------------------------------------------------------------
// Variables
// --------------------------------------------------------------------------

struct Variables<'a> {
    message: &'a Message,
    summary: String,
}

impl<'a> Variables<'a> {
    fn new(message: &'a Message) -> Self {
        // Le corps habituel se termine par le lien quand il y en a un : il a sa
        // propre variable, `{{url}}`, et ne doit pas apparaître deux fois.
        let summary = match &message.link {
            Some(link) if message.text.ends_with(link.as_str()) => {
                message.text[..message.text.len() - link.len()].trim_end().to_string()
            }
            _ => message.text.clone(),
        };
        Self { message, summary }
    }

    fn get(&self, name: &str) -> String {
        let message = self.message;
        let tags = &message.context.tags;
        let started = message.context.started_at;
        let stamp = |at: DateTime<Utc>| at.format("%Y-%m-%d %H:%M UTC").to_string();
        match name {
            "title" => message.title.clone(),
            "monitor" => message.target_name.clone(),
            "severity" => message.severity_label().to_string(),
            "state" => message.status().to_string(),
            "summary" => self.summary.clone(),
            "value" => message.value.map(|v| format_value(v, &message.unit)).unwrap_or_default(),
            "threshold" => {
                message.threshold.map(|v| format_value(v, &message.unit)).unwrap_or_default()
            }
            "rule" => message.rule_name.clone(),
            "url" => message.link.clone().unwrap_or_default(),
            "started_at" => started.map(stamp).unwrap_or_default(),
            "duration" => started
                .filter(|_| !message.resolved)
                .map(|since| format_duration(message.at.signed_duration_since(since)))
                .unwrap_or_default(),
            "site" => {
                tags.get("site").or_else(|| tags.get("location")).cloned().unwrap_or_default()
            }
            "count" => message.count.to_string(),
            "date" => stamp(message.at),
            other => other
                .strip_prefix(TAG_PREFIX)
                .and_then(|key| tags.get(key))
                .cloned()
                .unwrap_or_default(),
        }
    }
}

// --------------------------------------------------------------------------
// Rendu et échappement
// --------------------------------------------------------------------------

/// Caractères réservés du MarkdownV2 de Telegram.
const MARKDOWN_V2_RESERVED: &str = "_*[]()~`>#+-=|{}.!\\";
/// Ceux qu'un texte écrit à la main n'emploie jamais comme mise en forme : ils
/// sont échappés d'office dans le texte du gabarit, pour qu'une phrase comme
/// « Disk full on nas. » ne fasse pas refuser le message. La mise en forme
/// (`*gras*`, `_italique_`, `[lien](…)`, `` `code` ``) reste à l'utilisateur.
const MARKDOWN_V2_ALWAYS_ESCAPED: &str = ".!-=+#{}";

fn escape_markdown_v2(value: &str, in_link_url: bool) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        // Dans l'adresse d'un lien, seuls `)` et `\` ont à l'être.
        let reserved =
            if in_link_url { matches!(c, ')' | '\\') } else { MARKDOWN_V2_RESERVED.contains(c) };
        if reserved {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Échappe une valeur substituée pour `format`.
pub fn escape(format: TextFormat, value: &str, in_link_url: bool) -> String {
    match format {
        TextFormat::Plain => value.to_string(),
        TextFormat::SlackMrkdwn => {
            value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        }
        TextFormat::Html => value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;"),
        TextFormat::TelegramMarkdownV2 => escape_markdown_v2(value, in_link_url),
    }
}

/// État du rendu MarkdownV2 d'un morceau à l'autre du gabarit.
#[derive(Default)]
struct MarkdownState {
    /// Entre `](` et `)` : l'adresse d'un lien.
    in_link_url: bool,
    /// Le caractère précédent était un `\` d'échappement.
    escaping: bool,
    last: Option<char>,
}

fn markdown_v2_literal(text: &str, state: &mut MarkdownState, out: &mut String) {
    for c in text.chars() {
        if state.escaping {
            state.escaping = false;
        } else if c == '\\' {
            state.escaping = true;
        } else if state.in_link_url {
            if c == ')' {
                state.in_link_url = false;
            }
        } else if c == '(' && state.last == Some(']') {
            state.in_link_url = true;
        } else if MARKDOWN_V2_ALWAYS_ESCAPED.contains(c) {
            out.push('\\');
        }
        out.push(c);
        state.last = Some(c);
    }
}

fn render(template: &Template, variables: &Variables<'_>, format: TextFormat) -> String {
    let mut out = String::new();
    let mut markdown = MarkdownState::default();
    for part in template.parts() {
        match part {
            Part::Literal(text) if format == TextFormat::TelegramMarkdownV2 => {
                markdown_v2_literal(text, &mut markdown, &mut out);
            }
            Part::Literal(text) => out.push_str(text),
            Part::Variable(name) => {
                let value = variables.get(name);
                out.push_str(&escape(format, &value, markdown.in_link_url));
                markdown.escaping = false;
                markdown.last = value.chars().last().or(markdown.last);
            }
        }
    }
    // Une variable vide en fin de ligne (`{{url}}` sans adresse publique) ne
    // laisse pas de ligne blanche derrière elle.
    out.trim_end().to_string()
}

// --------------------------------------------------------------------------
// Aperçu
// --------------------------------------------------------------------------

/// Aperçu d'un gabarit sur une alerte d'exemple, pour le formulaire.
#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub format: TextFormat,
    pub title: String,
    pub body: String,
    pub default_title: &'static str,
    pub default_body: &'static str,
}

/// Alerte d'exemple de l'aperçu : un disque presque plein sur un NAS, avec
/// des étiquettes et un lien, pour que chaque variable ait quelque chose à dire.
pub fn sample_message(public_url: Option<&str>) -> Message {
    use crate::alerting::model::Severity;
    let at = DateTime::from_timestamp(1_788_271_200, 0).unwrap_or_else(Utc::now);
    let base = public_url.map(|url| url.trim_end_matches('/')).unwrap_or("https://monit.example");
    let link = format!("{base}/targets/42");
    let mut message = crate::notify::message::test_message("preview");
    message.title = "nas — Disk full".to_string();
    message.text =
        format!("Warning · Disk full — /volume1 — 93 % (threshold > 90 %), for 12 min\n{link}");
    message.markdown = format!("**{}**\n• {}", message.title, message.text);
    message.severity = Severity::Warning;
    message.target_name = "nas".to_string();
    message.target_id = Some(42);
    message.at = at;
    message.rule_name = "Disk full".to_string();
    message.fingerprint = "disk_full@nas".to_string();
    message.value = Some(93.0);
    message.threshold = Some(90.0);
    message.unit = "%".to_string();
    message.operator = ">".to_string();
    message.link = Some(link);
    message.context.started_at = Some(at - chrono::TimeDelta::minutes(12));
    message.context.tags = BTreeMap::from([
        ("site".to_string(), "Home".to_string()),
        ("room".to_string(), "Cellar".to_string()),
    ]);
    message
}

/// Rend l'aperçu des gabarits saisis pour un type de canal.
pub fn preview(
    kind: &str,
    title: Option<&str>,
    body: Option<&str>,
    public_url: Option<&str>,
) -> Result<Preview, NotifyError> {
    let Some(format) = format_for(kind) else {
        return Err(NotifyError::Config(format!(
            "channels of type \"{kind}\" do not use message templates"
        )));
    };
    let templates = MessageTemplates::parse(title, body)?;
    let templates = templates.unwrap_or(MessageTemplates { title: None, body: None });
    let rendered = templates.render(&sample_message(public_url), format);
    Ok(Preview {
        format,
        title: rendered.title,
        body: rendered.body,
        default_title: DEFAULT_TITLE,
        default_body: DEFAULT_BODY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn message() -> Message {
        sample_message(Some("https://monit.lan/"))
    }

    fn rendered(title: Option<&str>, body: Option<&str>, format: TextFormat) -> Rendered {
        MessageTemplates::parse(title, body).unwrap().unwrap().render(&message(), format)
    }

    #[test]
    fn the_default_templates_give_back_the_usual_message() {
        let message = message();
        let out = rendered(Some(DEFAULT_TITLE), Some(DEFAULT_BODY), TextFormat::Plain);
        assert_eq!(out.title, message.title);
        assert_eq!(out.body, message.text, "summary then the link, as before");
        // Sans lien, pas de ligne vide en fin de corps.
        let mut bare = message.clone();
        bare.text = "Warning · Disk full".to_string();
        bare.link = None;
        let templates = MessageTemplates::parse(None, Some(DEFAULT_BODY)).unwrap().unwrap();
        assert_eq!(templates.render(&bare, TextFormat::Plain).body, "Warning · Disk full");
    }

    #[test]
    fn no_template_means_no_change() {
        assert!(MessageTemplates::parse(None, Some("  ")).unwrap().is_none());
        assert!(MessageTemplates::from_settings(&json!({"chat_id": "1"})).unwrap().is_none());
        let config = ChannelConfig {
            id: 1,
            name: "c".into(),
            kind: "ntfy".into(),
            enabled: true,
            settings: json!({}),
            secrets: json!({}),
        };
        let message = message();
        assert!(matches!(apply(&config, &message), Cow::Borrowed(_)));
    }

    #[test]
    fn every_variable_has_a_value_on_the_sample() {
        let body = "{{monitor}}|{{severity}}|{{state}}|{{value}}|{{threshold}}|{{rule}}|{{url}}|\
                    {{started_at}}|{{duration}}|{{site}}|{{count}}|{{date}}|{{tags.room}}";
        let out = rendered(None, Some(body), TextFormat::Plain);
        assert_eq!(
            out.body,
            "nas|warning|firing|93 %|90 %|Disk full|https://monit.lan/targets/42|\
             2026-09-01 13:48 UTC|12 min|Home|1|2026-09-01 14:00 UTC|Cellar"
        );
        assert_eq!(rendered(None, Some("{{tags.missing}}x"), TextFormat::Plain).body, "x");
    }

    #[test]
    fn an_unknown_variable_is_refused_with_the_list() {
        let error = MessageTemplates::parse(Some("{{monitr}}"), None).unwrap_err().to_string();
        assert!(error.contains("monitr"), "{error}");
        assert!(error.contains(TITLE_KEY), "{error}");
        assert!(error.contains("monitor") && error.contains("tags.<key>"), "{error}");
        assert!(MessageTemplates::parse(None, Some("{{tags.}}")).is_err());
        assert!(MessageTemplates::parse(None, Some("{{tags.<key>}}")).is_err());
        assert!(MessageTemplates::parse(None, Some("{{tags.rack-2}}")).is_ok());
        assert!(MessageTemplates::parse(None, Some("{{monitor")).is_err(), "unclosed");
        let long = "a".repeat(MAX_TEMPLATE_LEN + 1);
        assert!(MessageTemplates::parse(Some(&long), None).is_err());
        assert!(MessageTemplates::from_settings(&json!({ BODY_KEY: 3 })).is_err());
    }

    #[test]
    fn values_cannot_add_markup_in_slack() {
        let mut message = message();
        message.target_name = "<!channel> & co".to_string();
        let templates = MessageTemplates::parse(None, Some("*{{monitor}}* is down")).unwrap();
        let out = templates.unwrap().render(&message, TextFormat::SlackMrkdwn);
        assert_eq!(out.body, "*&lt;!channel&gt; &amp; co* is down", "the user's * stays bold");
    }

    #[test]
    fn values_are_escaped_for_html_email() {
        let mut message = message();
        message.target_name = r#"<script>"x"&'y'"#.to_string();
        let templates = MessageTemplates::parse(None, Some("<b>{{monitor}}</b>")).unwrap();
        let out = templates.unwrap().render(&message, TextFormat::Html);
        assert_eq!(out.body, "<b>&lt;script&gt;&quot;x&quot;&amp;&#39;y&#39;</b>");
    }

    #[test]
    fn telegram_markdown_v2_escapes_values_and_plain_punctuation() {
        let mut message = message();
        message.target_name = "DESKTOP_01 (v2.1)".to_string();
        let templates = MessageTemplates::parse(
            None,
            Some("*{{monitor}}* is down. Check it!\n[Open]({{url}}) \\(really\\)"),
        )
        .unwrap();
        let out = templates.unwrap().render(&message, TextFormat::TelegramMarkdownV2);
        assert_eq!(
            out.body,
            "*DESKTOP\\_01 \\(v2\\.1\\)* is down\\. Check it\\!\n\
             [Open](https://monit.lan/targets/42) \\(really\\)"
        );
        // Une adresse avec une parenthèse fermante reste dans le lien.
        let mut odd = message.clone();
        odd.link = Some("https://m.lan/a)b".to_string());
        let templates = MessageTemplates::parse(None, Some("[x]({{url}})")).unwrap().unwrap();
        assert_eq!(
            templates.render(&odd, TextFormat::TelegramMarkdownV2).body,
            "[x](https://m.lan/a\\)b)"
        );
    }

    #[test]
    fn plain_text_is_left_alone() {
        let mut message = message();
        message.target_name = "a<b>*c*".to_string();
        let out = MessageTemplates::parse(Some("{{monitor}}\n!"), Some("{{monitor}}"))
            .unwrap()
            .unwrap()
            .render(&message, TextFormat::Plain);
        assert_eq!(out.body, "a<b>*c*");
        assert_eq!(out.title, "a<b>*c* !", "a title stays on one line");
    }

    #[test]
    fn applying_fills_the_fields_every_channel_reads() {
        let config = ChannelConfig {
            id: 1,
            name: "mail".into(),
            kind: "smtp".into(),
            enabled: true,
            settings: json!({ TITLE_KEY: "{{monitor}}: {{rule}}", BODY_KEY: "<p>{{monitor}}</p>" }),
            secrets: json!({}),
        };
        let message = message();
        let out = apply(&config, &message);
        assert_eq!(out.title, "nas: Disk full");
        assert_eq!(out.text, "<p>nas</p>", "the plain part keeps the user's text");
        let rendered = out.context.rendered.as_ref().expect("rendered");
        assert_eq!(rendered.format, TextFormat::Html);
        assert_eq!(rendered.body, "<p>nas</p>");
        assert_eq!(out.actions, message.actions);
    }

    #[test]
    fn kinds_without_templates_are_left_alone_and_refused_in_preview() {
        assert_eq!(format_for("webhook"), None);
        assert_eq!(format_for("jira"), None);
        assert_eq!(format_for("discord"), Some(TextFormat::Plain));
        assert!(preview("jira", None, None, None).is_err());
        let shown = preview("telegram", None, Some("{{monitor}}."), None).unwrap();
        assert_eq!(shown.body, "nas\\.");
        assert_eq!(shown.title, "nas — Disk full");
        assert_eq!(shown.default_body, DEFAULT_BODY);
    }
}
