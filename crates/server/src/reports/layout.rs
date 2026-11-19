//! Mise en page d'un rapport SLA, choisie à la génération : gabarit (détaillé
//! ou synthèse), format de papier, sections à inclure, titre, notes, logo, et
//! périmètre (moniteurs ou groupes).
//!
//! Les deux structures sont stockées en JSON dans `report_schedules` (colonnes
//! `layout` et `scope`) : un champ absent prend sa valeur par défaut, et le
//! défaut reproduit le rapport d'avant ces options (détaillé, A4, tout inclus,
//! toute l'instance).

use serde::{Deserialize, Serialize};

use super::collect::UNGROUPED;

/// Plafonds des entrées libres.
pub const MAX_TITLE_CHARS: usize = 120;
pub const MAX_NOTES_CHARS: usize = 2000;
pub const MAX_SCOPE_TARGETS: usize = 1000;
pub const MAX_SCOPE_GROUPS: usize = 200;

/// Gabarit du PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Template {
    /// Tout le détail : chiffres clés, graphe, groupes, chaque moniteur,
    /// chaque incident, maintenances, temps de réponse, méthode.
    #[default]
    Detailed,
    /// Une à deux pages : SLA global contre l'objectif, disponibilité par
    /// groupe (ou par moniteur), incidents majeurs, indisponibilité totale.
    Summary,
}

impl Template {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "detailed" => Some(Self::Detailed),
            "summary" => Some(Self::Summary),
            _ => None,
        }
    }
}

/// Format de papier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Paper {
    #[default]
    A4,
    Letter,
}

impl Paper {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "a4" => Some(Self::A4),
            "letter" => Some(Self::Letter),
            _ => None,
        }
    }

    /// Largeur et hauteur en points (1/72 de pouce).
    pub fn size(self) -> (f64, f64) {
        match self {
            Self::A4 => (595.0, 842.0),
            Self::Letter => (612.0, 792.0),
        }
    }
}

/// Origine du logo en tête du PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogoSource {
    /// Celui du client du rapport (mode MSP), sinon celui de la page de statut
    /// choisie : le comportement d'avant ce choix.
    #[default]
    Auto,
    /// Le logo du client du rapport seulement.
    Client,
    /// Le logo de la page de statut choisie seulement.
    Page,
    /// Le logo téléversé pour les rapports (`report_logo`, migration 0247).
    Custom,
    /// Pas de logo.
    #[serde(rename = "none")]
    Hidden,
}

impl LogoSource {
    /// Valeur du paramètre `logo` de l'API : `auto` (ou `true`), `client`,
    /// `page`, `custom`, `none` (ou `false`).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "auto" | "true" => Some(Self::Auto),
            "client" => Some(Self::Client),
            "page" => Some(Self::Page),
            "custom" => Some(Self::Custom),
            "none" | "false" => Some(Self::Hidden),
            _ => None,
        }
    }
}

/// Sections du rapport. Toutes incluses par défaut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sections {
    /// Chiffres clés et objectif.
    pub summary: bool,
    /// Graphe de disponibilité (barres par groupe ou par moniteur).
    pub chart: bool,
    /// Tableau par moniteur (par groupe dans la synthèse).
    pub monitors: bool,
    /// Incidents (les majeurs seulement dans la synthèse).
    pub incidents: bool,
    /// Fenêtres de maintenance de la période.
    pub maintenance: bool,
    /// Temps de réponse des sondes.
    pub response_times: bool,
    /// Notes libres de l'auteur du rapport.
    pub notes: bool,
}

impl Default for Sections {
    fn default() -> Self {
        Self {
            summary: true,
            chart: true,
            monitors: true,
            incidents: true,
            maintenance: true,
            response_times: true,
            notes: true,
        }
    }
}

/// Noms des sections, dans l'ordre du rapport (paramètre `sections` de l'API).
pub const SECTION_NAMES: [&str; 7] =
    ["summary", "chart", "monitors", "incidents", "maintenance", "response_times", "notes"];

impl Sections {
    /// Aucune section : rien d'autre que l'en-tête.
    pub fn none() -> Self {
        Self {
            summary: false,
            chart: false,
            monitors: false,
            incidents: false,
            maintenance: false,
            response_times: false,
            notes: false,
        }
    }

    /// Liste séparée par des virgules (`summary,incidents`). Une liste vide
    /// garde toutes les sections ; un nom inconnu est une erreur.
    pub fn parse_list(raw: &str) -> Result<Self, String> {
        let names: Vec<&str> = raw.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if names.is_empty() {
            return Ok(Self::default());
        }
        let mut sections = Self::none();
        for name in names {
            match name {
                "summary" => sections.summary = true,
                "chart" => sections.chart = true,
                "monitors" => sections.monitors = true,
                "incidents" => sections.incidents = true,
                "maintenance" => sections.maintenance = true,
                "response_times" => sections.response_times = true,
                "notes" => sections.notes = true,
                other => {
                    let shown: String =
                        other.chars().filter(|c| !c.is_control()).take(40).collect();
                    return Err(format!(
                        "Unknown section \"{shown}\" (expected: {}).",
                        SECTION_NAMES.join(", ")
                    ));
                }
            }
        }
        Ok(sections)
    }
}

/// Mise en page complète.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    pub template: Template,
    pub paper: Paper,
    pub sections: Sections,
    /// Titre affiché sous la marque, à la place de « Availability report ».
    pub title: Option<String>,
    /// Notes libres, imprimées dans la section « Notes ».
    pub notes: String,
    /// Faux : pas de logo, même si le client ou la page de statut en a un.
    /// Gardé pour les mises en page enregistrées avant [`LogoSource`] ;
    /// toujours égal à `logo != none` après [`Layout::validate`].
    pub show_logo: bool,
    /// D'où vient le logo.
    pub logo: LogoSource,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            template: Template::Detailed,
            paper: Paper::A4,
            sections: Sections::default(),
            title: None,
            notes: String::new(),
            show_logo: true,
            logo: LogoSource::Auto,
        }
    }
}

/// Retire les caractères de contrôle, sauf les sauts de ligne, et les espaces
/// de bord de chaque ligne.
fn clean_notes(raw: &str) -> String {
    let text: String = raw
        .replace("\r\n", "\n")
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect();
    text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string()
}

impl Layout {
    /// Le logo à mettre en tête : `show_logo` à faux (mise en page ancienne)
    /// l'emporte sur tout choix.
    pub fn logo_source(&self) -> LogoSource {
        if self.show_logo { self.logo } else { LogoSource::Hidden }
    }

    /// Valide et normalise : titre d'une ligne et borné, notes bornées.
    pub fn validate(mut self) -> Result<Self, String> {
        self.logo = self.logo_source();
        self.show_logo = self.logo != LogoSource::Hidden;
        self.title = match self.title.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(title) => {
                if title.chars().count() > MAX_TITLE_CHARS || title.chars().any(char::is_control) {
                    return Err(format!(
                        "The report title must be {MAX_TITLE_CHARS} characters at most, without \
                         control characters."
                    ));
                }
                Some(title.to_string())
            }
        };
        self.notes = clean_notes(&self.notes);
        if self.notes.chars().count() > MAX_NOTES_CHARS {
            return Err(format!("The notes must be {MAX_NOTES_CHARS} characters at most."));
        }
        Ok(self)
    }

    /// Lecture tolérante d'une colonne stockée : illisible, c'est le défaut.
    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }
}

/// Périmètre : les moniteurs nommés et ceux des groupes nommés (union). Vide :
/// tout ce que voit le rapport (l'instance, ou le client en mode MSP).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Scope {
    pub targets: Vec<i64>,
    /// Noms de groupes ; « Ungrouped » désigne les moniteurs sans groupe.
    pub groups: Vec<String>,
}

impl Scope {
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty() && self.groups.is_empty()
    }

    /// Vrai si le moniteur `id`, du groupe `group`, est dans le périmètre.
    pub fn keeps(&self, id: i64, group: &str) -> bool {
        if self.is_empty() || self.targets.contains(&id) {
            return true;
        }
        let group = group.trim();
        let group = if group.is_empty() { UNGROUPED } else { group };
        self.groups.iter().any(|wanted| wanted == group)
    }

    /// Dédoublonne, retire les groupes vides, borne les listes.
    pub fn validate(mut self) -> Result<Self, String> {
        self.targets.sort_unstable();
        self.targets.dedup();
        let mut groups: Vec<String> = Vec::new();
        for group in self.groups {
            let group = group.trim().to_string();
            if group.chars().any(char::is_control) || group.chars().count() > 200 {
                return Err("A group name in the scope is not valid.".to_string());
            }
            if !group.is_empty() && !groups.contains(&group) {
                groups.push(group);
            }
        }
        self.groups = groups;
        if self.targets.len() > MAX_SCOPE_TARGETS || self.groups.len() > MAX_SCOPE_GROUPS {
            return Err(format!(
                "A report scope lists at most {MAX_SCOPE_TARGETS} monitors and \
                 {MAX_SCOPE_GROUPS} groups."
            ));
        }
        Ok(self)
    }

    pub fn from_json(raw: &str) -> Self {
        serde_json::from_str(raw).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_column_gives_the_report_of_before() {
        let layout = Layout::from_json("{}");
        assert_eq!(layout, Layout::default());
        assert_eq!(layout.template, Template::Detailed);
        assert_eq!(layout.paper, Paper::A4);
        assert!(layout.sections.monitors && layout.sections.notes && layout.show_logo);
        assert_eq!(Layout::from_json("not json"), Layout::default());
        assert!(Scope::from_json("{}").is_empty());
    }

    #[test]
    fn a_partial_layout_keeps_the_other_defaults() {
        let layout = Layout::from_json(r#"{"template":"summary","sections":{"chart":false}}"#);
        assert_eq!(layout.template, Template::Summary);
        assert!(!layout.sections.chart);
        assert!(layout.sections.incidents);
        assert!(layout.show_logo);
    }

    #[test]
    fn sections_are_parsed_from_a_list() {
        let only = Sections::parse_list("summary, incidents").expect("valid");
        assert!(only.summary && only.incidents);
        assert!(!only.chart && !only.monitors && !only.notes);
        assert_eq!(Sections::parse_list("").expect("valid"), Sections::default());
        assert!(Sections::parse_list("summary,bogus").is_err());
    }

    #[test]
    fn titles_and_notes_are_bounded_and_cleaned() {
        let layout = Layout {
            title: Some("  Q3 review  ".into()),
            notes: "Line one  \r\nLine\u{0} two\n".into(),
            ..Layout::default()
        }
        .validate()
        .expect("valid");
        assert_eq!(layout.title.as_deref(), Some("Q3 review"));
        assert_eq!(layout.notes, "Line one\nLine  two");
        let blank = Layout { title: Some("   ".into()), ..Layout::default() }.validate();
        assert_eq!(blank.expect("valid").title, None);
        let long = Layout { title: Some("x".repeat(121)), ..Layout::default() };
        assert!(long.validate().is_err());
        let control = Layout { title: Some("a\nb".into()), ..Layout::default() };
        assert!(control.validate().is_err());
        let notes = Layout { notes: "n".repeat(MAX_NOTES_CHARS + 1), ..Layout::default() };
        assert!(notes.validate().is_err());
    }

    #[test]
    fn the_scope_is_a_union_of_monitors_and_groups() {
        let scope = Scope { targets: vec![7], groups: vec!["Edge".into(), UNGROUPED.into()] };
        assert!(scope.keeps(7, "Core"));
        assert!(scope.keeps(1, "Edge"));
        assert!(scope.keeps(2, "  "), "the Ungrouped name selects monitors without a group");
        assert!(!scope.keeps(3, "Core"));
        assert!(Scope::default().keeps(3, "Core"), "an empty scope keeps everything");
    }

    #[test]
    fn the_scope_is_deduplicated_and_bounded() {
        let groups = vec![" A ".to_string(), "A".to_string(), String::new()];
        let scope = Scope { targets: vec![3, 1, 3], groups }.validate().expect("valid");
        assert_eq!(scope.targets, vec![1, 3]);
        assert_eq!(scope.groups, vec!["A".to_string()]);
        let many = Scope { targets: (0..=MAX_SCOPE_TARGETS as i64).collect(), groups: Vec::new() };
        assert!(many.validate().is_err());
    }

    #[test]
    fn the_logo_source_reads_old_layouts_and_query_values() {
        // Mise en page d'avant le choix : `show_logo` seul.
        let old = Layout::from_json(r#"{"show_logo":false}"#);
        assert_eq!(old.logo_source(), LogoSource::Hidden);
        assert_eq!(Layout::from_json("{}").logo_source(), LogoSource::Auto);
        let custom = Layout::from_json(r#"{"logo":"custom"}"#);
        assert_eq!(custom.logo_source(), LogoSource::Custom);
        let hidden = Layout::from_json(r#"{"logo":"none"}"#).validate().expect("valid");
        assert!(!hidden.show_logo, "validate keeps both fields in step");
        let shown = Layout { show_logo: false, logo: LogoSource::Page, ..Layout::default() };
        assert_eq!(shown.validate().expect("valid").logo, LogoSource::Hidden);
        assert_eq!(LogoSource::parse("false"), Some(LogoSource::Hidden));
        assert_eq!(LogoSource::parse("true"), Some(LogoSource::Auto));
        assert_eq!(LogoSource::parse("Client"), Some(LogoSource::Client));
        assert_eq!(LogoSource::parse("svg"), None);
        let json = serde_json::to_string(&LogoSource::Hidden).expect("json");
        assert_eq!(json, "\"none\"");
    }

    #[test]
    fn paper_sizes_are_in_points() {
        assert_eq!(Paper::A4.size(), (595.0, 842.0));
        assert_eq!(Paper::Letter.size(), (612.0, 792.0));
        assert_eq!(Paper::parse("Letter"), Some(Paper::Letter));
        assert_eq!(Template::parse("summary"), Some(Template::Summary));
        assert_eq!(Template::parse("fancy"), None);
    }
}
