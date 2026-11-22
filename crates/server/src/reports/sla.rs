//! Rapport de disponibilité (SLA) : un PDF lisible par un client ou un
//! auditeur, et des CSV pour le tableur.
//!
//! Le contenu est celui de [`ReportData`] : disponibilité par moniteur et par
//! groupe, incidents de la période, temps de réponse moyen et au 95e
//! centile, et, si un objectif est donné, pour chaque moniteur s'il est tenu.
//! Aucun état n'est porté par la couleur seule : chaque pourcentage est suivi
//! d'un mot.
//!
//! Le PDF est écrit par [`super::pdf`] (texte et tableaux, polices standard,
//! sans dépendance), en deux gabarits ([`Template`]) : détaillé (tout) ou
//! synthèse (une à deux pages). Les sections, le papier (A4 ou Letter), le
//! titre et les notes suivent [`Layout`]. Le logo du client (mode MSP) ou de
//! la page de statut est incorporé en tête quand il est en JPEG ou en PNG
//! (voir [`super::logo`]) ; sinon le nom en tient lieu.

use chrono::{DateTime, Datelike, TimeDelta, Utc};
use chrono_tz::Tz;

use super::collect::{DeviceStat, Incident, ReportData, UNGROUPED};
use super::layout::{Layout, LogoSource, Paper, Template};
use super::pdf::{self, Document, Font, Page, Rgb};
use super::render::{count_delta, delta, duration, one_line, verdict, when};

/// Plafonds : un PDF de plusieurs centaines de pages ne se lit plus.
pub const MAX_PDF_MONITORS: usize = 400;
pub const MAX_PDF_INCIDENTS: usize = 300;

/// Habillage et réglages d'un rapport.
pub struct Options {
    /// Nom affiché en tête : l'organisation, ou la page de statut choisie.
    pub brand: String,
    /// Objectif de disponibilité, en pour cent.
    pub objective: Option<f64>,
    /// Instant de génération, écrit dans le pied de page et les métadonnées.
    pub generated: DateTime<Utc>,
    /// Logo en tête à droite, selon [`Layout::logo`] : celui du client (mode
    /// MSP), de la page de statut choisie ou le logo téléversé des rapports.
    pub logo: Option<pdf::Image>,
    /// Adresse (relative à `/api`) du même logo, pour l'aperçu à l'écran ;
    /// `None` quand le PDF n'en a pas.
    pub logo_url: Option<String>,
    /// Gabarit, papier, sections, titre et notes.
    pub layout: Layout,
}

// --------------------------------------------------------------------------
// Noms de fichiers et étiquettes
// --------------------------------------------------------------------------

/// Racine du nom de fichier : `availability-2026-10` pour un mois civil entier,
/// sinon `availability-2026-10-05_2026-10-12`.
pub fn file_stem(data: &ReportData, tz: Tz) -> String {
    let start = data.period.start.with_timezone(&tz).date_naive();
    let last = (data.period.end - TimeDelta::seconds(1)).with_timezone(&tz).date_naive();
    let whole_month = start.day() == 1
        && last.month() == start.month()
        && last.year() == start.year()
        && last.succ_opt().is_some_and(|next| next.day() == 1);
    if whole_month {
        format!("availability-{}", start.format("%Y-%m"))
    } else {
        format!("availability-{}_{}", start.format("%Y-%m-%d"), last.format("%Y-%m-%d"))
    }
}

/// « 2026-10-01 to 2026-10-31 (UTC) », la fin étant le dernier jour inclus.
pub fn period_label(data: &ReportData, tz: Tz) -> String {
    let start = data.period.start.with_timezone(&tz);
    let last = (data.period.end - TimeDelta::seconds(1)).with_timezone(&tz);
    format!("{} to {} ({})", start.format("%Y-%m-%d"), last.format("%Y-%m-%d"), last.format("%Z"))
}

/// Mois d'un rapport mensuel : « October 2026 ».
pub fn month_label(data: &ReportData, tz: Tz) -> String {
    data.period.start.with_timezone(&tz).format("%B %Y").to_string()
}

fn pct(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |v| format!("{:.3}%", v.clamp(0.0, 100.0)))
}

fn ms(value: Option<f64>) -> String {
    value.map_or_else(|| "-".to_string(), |v| format!("{} ms", v.round() as i64))
}

/// Le mot qui accompagne un pourcentage : tenu / manqué avec un objectif, sinon
/// le verdict habituel des rapports.
pub fn status_word(uptime: Option<f64>, objective: Option<f64>) -> &'static str {
    match (uptime, objective) {
        (None, _) => "No data",
        (Some(value), Some(goal)) => {
            // Une tolérance d'un millionième : 99.9 contre 99.900000001.
            if value + 1e-6 >= goal { "Met" } else { "Missed" }
        }
        (Some(_), None) => verdict(uptime).0,
    }
}

// --------------------------------------------------------------------------
// CSV
// --------------------------------------------------------------------------

/// Une cellule CSV (RFC 4180). Une valeur qui commence par `=`, `+`, `-`, `@`
/// ou une tabulation serait lue comme une formule par un tableur : elle est
/// préfixée d'une apostrophe. Les noms de moniteurs viennent de l'utilisateur,
/// parfois du réseau.
pub fn csv_cell(raw: &str) -> String {
    let mut value = one_line(raw);
    if value.starts_with(['=', '+', '-', '@', '\t']) {
        value.insert(0, '\'');
    }
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

fn csv_row(cells: &[String]) -> String {
    let mut line = cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",");
    line.push_str("\r\n");
    line
}

fn number(value: Option<f64>, decimals: usize) -> String {
    value.map_or_else(String::new, |v| format!("{v:.decimals$}"))
}

/// Les moniteurs : une ligne chacun. Commence par une marque UTF-8 (BOM) pour
/// qu'Excel lise les accents.
pub fn csv_monitors(data: &ReportData, objective: Option<f64>) -> String {
    let mut out = String::from("\u{feff}");
    let mut header = vec![
        "group",
        "monitor",
        "availability_percent",
        "previous_availability_percent",
        "incidents",
        "downtime_seconds",
        "avg_response_ms",
        "p95_response_ms",
    ];
    // Les colonnes d'objectif paraissent dès qu'il y en a un, du rapport ou d'une ligne.
    let with_objective = objective.is_some() || data.devices.iter().any(|d| d.objective.is_some());
    if with_objective {
        header.extend(["objective_percent", "objective"]);
    }
    out.push_str(&csv_row(&header.iter().map(|h| (*h).to_string()).collect::<Vec<_>>()));
    for device in &data.devices {
        let mut cells = vec![
            device.group.clone(),
            device.name.clone(),
            number(device.uptime, 3),
            number(device.previous_uptime, 3),
            device.incidents.to_string(),
            device.downtime_secs.to_string(),
            number(device.latency_avg_ms, 1),
            number(device.latency_p95_ms, 1),
        ];
        if with_objective {
            let goal = device.objective.or(objective);
            cells.push(number(goal, 3));
            cells.push(status_word(device.uptime, goal).to_lowercase().replace(' ', "_"));
        }
        out.push_str(&csv_row(&cells));
    }
    out
}

/// Les incidents de la période, horodatés en UTC (lisibles par une machine).
pub fn csv_incidents(data: &ReportData) -> String {
    let iso = |at: DateTime<Utc>| at.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut out = String::from("\u{feff}");
    let header = [
        "monitor",
        "rule",
        "severity",
        "started_utc",
        "ended_utc",
        "duration_seconds",
        "ongoing",
        "began_before_period",
    ];
    out.push_str(&csv_row(&header.iter().map(|h| (*h).to_string()).collect::<Vec<_>>()));
    for incident in &data.incidents {
        out.push_str(&csv_row(&[
            incident.device.clone(),
            incident.rule.clone(),
            incident.severity.clone(),
            iso(incident.started),
            incident.ended.map(iso).unwrap_or_default(),
            incident.duration(data.period.end).num_seconds().max(0).to_string(),
            incident.ended.is_none().to_string(),
            incident.began_before_period.to_string(),
        ]));
    }
    out
}

// --------------------------------------------------------------------------
// PDF
// --------------------------------------------------------------------------
//
// Mise en page pensée pour l'impression : marges de 17 mm, aucun fond coloré
// (contours et filets seulement, qu'une imprimante n'omet jamais), en-têtes de
// tableaux répétés à chaque page, rappel de la marque et de la période en tête
// des pages suivantes, pagination en pied. Lisible en noir et blanc : chaque
// statut est un mot, et les mauvais (« Missed », « Poor ») sont en gras.

const MARGIN: f64 = 48.0;
/// Hauteur réservée au rappel d'en-tête des pages suivantes.
const RUNNING_HEADER: f64 = 26.0;
/// Bas de la zone de contenu ; le pied de page est en dessous.
const BOTTOM: f64 = 62.0;
const FOOTER_RULE: f64 = 46.0;
const FOOTER_TEXT: f64 = 32.0;
const ROW: f64 = 15.0;
const GAP: f64 = 6.0;
/// Cadre du logo dans l'en-tête, en points.
const LOGO_W: f64 = 140.0;
const LOGO_H: f64 = 44.0;
/// Barres du graphe au plus.
const MAX_CHART_BARS: usize = 15;
/// Moniteurs de la synthèse, quand il n'y a pas de groupes.
const MAX_SUMMARY_MONITORS: usize = 25;
/// Incidents majeurs de la synthèse.
const MAX_MAJOR_INCIDENTS: usize = 5;

const INK: Rgb = Rgb(17, 24, 39);
const MUTED: Rgb = Rgb(75, 85, 99);
/// Filets de structure : assez foncés pour sortir à l'impression.
const RULE: Rgb = Rgb(156, 163, 175);
const HAIR: Rgb = Rgb(209, 213, 219);
const BAR: Rgb = Rgb(107, 114, 128);

fn word_color(word: &str) -> Rgb {
    match word {
        "Healthy" | "Met" => Rgb(21, 128, 61),
        "Degraded" => Rgb(146, 64, 14),
        "Poor" | "Missed" => Rgb(153, 27, 27),
        _ => MUTED,
    }
}

/// Les mauvais statuts en gras : la différence survit au noir et blanc.
fn word_font(word: &str) -> Font {
    if matches!(word, "Poor" | "Missed" | "Degraded") { Font::Bold } else { Font::Regular }
}

/// Le titre du rapport : celui choisi, sinon « Availability report ».
pub fn heading_title(layout: &Layout) -> String {
    layout.title.clone().unwrap_or_else(|| "Availability report".to_string())
}

/// Coupe un texte en lignes de `max` points au plus ; les sauts de ligne du
/// texte sont gardés, un mot trop long est tronqué.
pub fn wrap(text: &str, font: Font, size: f64, max: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate =
                if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
            if pdf::text_width(&candidate, font, size) <= max {
                line = candidate;
            } else {
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                }
                line = pdf::fit(word, font, size, max);
            }
        }
        lines.push(line);
    }
    lines
}

/// Les pages en construction, avec le curseur vertical.
struct Sheet {
    pages: Vec<Page>,
    page: Page,
    /// Ordonnée de la prochaine ligne (l'origine est en bas de la page).
    y: f64,
    left: f64,
    right: f64,
    top: f64,
}

impl Sheet {
    fn new(paper: Paper) -> Self {
        let (width, height) = paper.size();
        Self {
            pages: Vec::new(),
            page: Page::default(),
            y: height - MARGIN,
            left: MARGIN,
            right: width - MARGIN,
            top: height - MARGIN,
        }
    }

    fn width(&self) -> f64 {
        self.right - self.left
    }

    fn break_page(&mut self) {
        self.pages.push(std::mem::take(&mut self.page));
        self.y = self.top - RUNNING_HEADER;
    }

    /// Passe à la page suivante s'il ne reste pas `height` points.
    fn need(&mut self, height: f64) -> bool {
        if self.y - height < BOTTOM {
            self.break_page();
            true
        } else {
            false
        }
    }

    fn heading(&mut self, text: &str) {
        // Un titre n'est jamais seul en bas de page : il emmène quelques lignes.
        self.need(70.0);
        self.y -= 26.0;
        self.page.text(self.left, self.y, text, Font::Bold, 12.0, INK);
        self.y -= 4.0;
    }

    fn note(&mut self, text: &str) {
        let width = self.width();
        for line in wrap(text, Font::Regular, 9.0, width) {
            self.need(ROW);
            self.y -= 13.0;
            self.page.text(self.left, self.y, &line, Font::Regular, 9.0, MUTED);
        }
    }

    fn paragraph(&mut self, text: &str) {
        let width = self.width();
        for line in wrap(text, Font::Regular, 10.0, width) {
            self.need(ROW);
            self.y -= 14.0;
            self.page.text(self.left, self.y, &line, Font::Regular, 10.0, INK);
        }
    }
}

/// Une colonne de tableau, placée.
struct Column {
    title: &'static str,
    x: f64,
    width: f64,
    /// Alignée à droite sur `x + width`.
    right: bool,
}

/// Répartit la largeur de la page entre les colonnes, au prorata des poids.
fn columns(sheet: &Sheet, specs: &[(&'static str, f64, bool)]) -> Vec<Column> {
    let total: f64 = specs.iter().map(|(_, weight, _)| weight).sum();
    let usable = sheet.width() - GAP * (specs.len().saturating_sub(1)) as f64;
    let mut x = sheet.left;
    specs
        .iter()
        .map(|&(title, weight, right)| {
            let width = usable * weight / total;
            let column = Column { title, x, width, right };
            x += width + GAP;
            column
        })
        .collect()
}

fn table_header(sheet: &mut Sheet, columns: &[Column]) {
    sheet.y -= ROW;
    for column in columns {
        if column.right {
            sheet.page.text_right(
                column.x + column.width,
                sheet.y,
                column.title,
                Font::Bold,
                8.0,
                INK,
            );
        } else {
            sheet.page.text(column.x, sheet.y, column.title, Font::Bold, 8.0, INK);
        }
    }
    sheet.page.line(sheet.left, sheet.y - 5.0, sheet.right, sheet.y - 5.0, 0.8, INK);
}

/// Une ligne de tableau : texte, couleur et police par colonne. L'en-tête est
/// répété en haut de chaque nouvelle page.
fn table_row(sheet: &mut Sheet, columns: &[Column], cells: &[(String, Rgb, Font)]) {
    if sheet.need(ROW * 2.0) {
        table_header(sheet, columns);
    }
    sheet.y -= ROW;
    for (column, (text, color, font)) in columns.iter().zip(cells) {
        let shown = pdf::fit(text, *font, 8.5, column.width);
        if column.right {
            sheet.page.text_right(column.x + column.width, sheet.y, &shown, *font, 8.5, *color);
        } else {
            sheet.page.text(column.x, sheet.y, &shown, *font, 8.5, *color);
        }
    }
    sheet.page.line(sheet.left, sheet.y - 5.0, sheet.right, sheet.y - 5.0, 0.3, HAIR);
}

/// Ouvre un tableau : l'en-tête et assez de place pour quelques lignes.
fn table_open(sheet: &mut Sheet, specs: &[(&'static str, f64, bool)]) -> Vec<Column> {
    sheet.need(ROW * 4.0);
    let placed = columns(sheet, specs);
    table_header(sheet, &placed);
    placed
}

fn plain(text: impl Into<String>) -> (String, Rgb, Font) {
    (text.into(), INK, Font::Regular)
}

fn muted(text: impl Into<String>) -> (String, Rgb, Font) {
    (text.into(), MUTED, Font::Regular)
}

fn status_cell(word: &str) -> (String, Rgb, Font) {
    (word.to_string(), word_color(word), word_font(word))
}

/// Un chiffre clé : étiquette, valeur, précision.
struct Stat {
    label: &'static str,
    value: String,
    sub: String,
    color: Rgb,
}

fn stat_box(page: &mut Page, x: f64, y: f64, width: f64, stat: &Stat) {
    page.rect_stroke(x, y - 54.0, width, 58.0, 0.6, RULE);
    page.text(x + 9.0, y - 10.0, stat.label, Font::Bold, 8.0, MUTED);
    page.text(x + 9.0, y - 32.0, &stat.value, Font::Bold, 17.0, stat.color);
    let sub = pdf::fit(&stat.sub, Font::Regular, 7.5, width - 18.0);
    page.text(x + 9.0, y - 46.0, &sub, Font::Regular, 7.5, MUTED);
}

fn has_groups(data: &ReportData) -> bool {
    data.groups.len() > 1 || data.groups.first().is_some_and(|g| g.name != UNGROUPED)
}

/// Les incidents majeurs : critiques d'abord, puis les plus longs.
pub fn major_incidents(data: &ReportData) -> Vec<&Incident> {
    let mut ranked: Vec<&Incident> = data.incidents.iter().collect();
    ranked.sort_by(|a, b| {
        (b.severity == "critical")
            .cmp(&(a.severity == "critical"))
            .then(b.duration(data.period.end).cmp(&a.duration(data.period.end)))
            .then(a.started.cmp(&b.started))
    });
    ranked.truncate(MAX_MAJOR_INCIDENTS);
    ranked
}

/// En-tête de la première page : logo à droite (image n° 0 du document), la
/// marque, le titre, la période.
fn first_header(sheet: &mut Sheet, data: &ReportData, tz: Tz, options: &Options, logo: bool) {
    let mut brand_width = sheet.width();
    if logo && let Some(image) = &options.logo {
        let (w, h) = (f64::from(image.width), f64::from(image.height));
        let scale = (LOGO_W / w).min(LOGO_H / h);
        let (dw, dh) = (w * scale, h * scale);
        sheet.page.image(0, sheet.right - dw, sheet.top - dh, dw, dh);
        brand_width -= LOGO_W + 12.0;
    }
    let left = sheet.left;
    sheet.y -= 18.0;
    let brand = pdf::fit(&one_line(&options.brand), Font::Bold, 20.0, brand_width);
    sheet.page.text(left, sheet.y, &brand, Font::Bold, 20.0, INK);
    sheet.y -= 19.0;
    let title = heading_title(&options.layout);
    let shown = pdf::fit(&one_line(&title), Font::Bold, 13.0, brand_width);
    sheet.page.text(left, sheet.y, &shown, Font::Bold, 13.0, MUTED);
    sheet.y -= 15.0;
    sheet.page.text(left, sheet.y, &period_label(data, tz), Font::Regular, 10.0, INK);
    let report_name = one_line(&data.name);
    if !report_name.is_empty() && report_name != title && report_name != "Availability report" {
        sheet.y -= 13.0;
        let shown = pdf::fit(&report_name, Font::Regular, 9.0, brand_width);
        sheet.page.text(left, sheet.y, &shown, Font::Regular, 9.0, MUTED);
    }
    sheet.y = sheet.y.min(sheet.top - LOGO_H) - 10.0;
    sheet.page.line(sheet.left, sheet.y, sheet.right, sheet.y, 1.0, INK);
}

/// Chiffres clés et, avec un objectif, combien de moniteurs le tiennent.
fn summary_section(sheet: &mut Sheet, data: &ReportData, options: &Options) {
    let incidents = data.incidents.len();
    let (word, _) = verdict(data.fleet_uptime);
    let availability_sub = match options.objective {
        Some(goal) => format!(
            "Objective {goal:.3}%: {}",
            status_word(data.fleet_uptime, Some(goal)).to_lowercase()
        ),
        None => {
            format!("{word}; {} vs previous", delta(data.fleet_uptime, data.fleet_previous_uptime))
        }
    };
    let fleet_word = status_word(data.fleet_uptime, options.objective);
    let stats = [
        Stat {
            label: "Availability",
            value: pct(data.fleet_uptime),
            sub: availability_sub,
            color: word_color(fleet_word),
        },
        Stat {
            label: "Downtime",
            value: duration(TimeDelta::seconds(data.downtime_secs)),
            sub: "total across monitors".to_string(),
            color: INK,
        },
        Stat {
            label: "Incidents",
            value: incidents.to_string(),
            sub: count_delta(incidents, data.previous_incident_count),
            color: INK,
        },
    ];
    sheet.need(80.0);
    sheet.y -= 16.0;
    let gap = 10.0;
    let width = (sheet.width() - 2.0 * gap) / 3.0;
    let top = sheet.y;
    let left = sheet.left;
    for (index, stat) in stats.iter().enumerate() {
        stat_box(&mut sheet.page, left + index as f64 * (width + gap), top, width, stat);
    }
    sheet.y -= 60.0;

    if let Some(goal) = options.objective {
        let measured: Vec<&DeviceStat> =
            data.devices.iter().filter(|d| d.uptime.is_some()).collect();
        let met = measured
            .iter()
            .filter(|d| status_word(d.uptime, Some(d.objective.unwrap_or(goal))) == "Met")
            .count();
        sheet.note(&format!(
            "SLA objective {goal:.3}%: met by {met} of {} monitor(s) with data. Fleet \
             availability: {}.",
            measured.len(),
            fleet_word.to_lowercase()
        ));
    }
    if !data.availability_known {
        sheet.note("Availability could not be read from the time-series database for this report.");
    }
}

/// Barres horizontales : par groupe s'il y en a, sinon les moniteurs les moins
/// disponibles. L'échelle ne part pas de zéro (tout serait collé à 100 %) :
/// le point de départ est écrit sous le graphe, la valeur au bout de chaque
/// barre, et l'objectif est un trait pointillé nommé.
fn chart_section(sheet: &mut Sheet, data: &ReportData, objective: Option<f64>) {
    let grouped = has_groups(data);
    let bars: Vec<(String, f64)> = if grouped {
        data.groups.iter().filter_map(|g| g.uptime.map(|v| (g.name.clone(), v))).collect()
    } else {
        data.devices.iter().filter_map(|d| d.uptime.map(|v| (d.name.clone(), v))).collect()
    };
    let total = bars.len();
    let bars: Vec<(String, f64)> = bars.into_iter().take(MAX_CHART_BARS).collect();
    sheet.heading(if grouped {
        "Availability by group, lowest first"
    } else {
        "Least available monitors"
    });
    if bars.is_empty() {
        sheet.note("No availability data to chart for this period.");
        return;
    }
    let lowest = bars.iter().map(|(_, v)| *v).fold(100.0_f64, f64::min);
    let lowest = objective.map_or(lowest, |goal| lowest.min(goal));
    let floor = [99.9, 99.5, 99.0, 98.0, 95.0, 90.0, 80.0, 50.0, 0.0]
        .into_iter()
        .find(|step| *step < lowest)
        .unwrap_or(0.0);
    // Tout le graphe sur une page.
    sheet.need(ROW * bars.len() as f64 + 46.0);
    let label_width = (sheet.width() * 0.3).min(160.0);
    let x0 = sheet.left + label_width + 8.0;
    let x1 = sheet.right - 52.0;
    let scale =
        |value: f64| x0 + (x1 - x0) * ((value.clamp(floor, 100.0) - floor) / (100.0 - floor));
    sheet.y -= 6.0;
    let top = sheet.y;
    for (name, value) in &bars {
        sheet.y -= ROW;
        let label = pdf::fit(&one_line(name), Font::Regular, 8.5, label_width);
        sheet.page.text(sheet.left, sheet.y, &label, Font::Regular, 8.5, INK);
        let end = scale(*value);
        if end > x0 {
            sheet.page.rect_fill(x0, sheet.y - 2.0, end - x0, 9.0, BAR);
            sheet.page.rect_stroke(x0, sheet.y - 2.0, end - x0, 9.0, 0.4, INK);
        }
        let shown = pct(Some(*value));
        sheet.page.text_right(sheet.right, sheet.y, &shown, Font::Bold, 8.5, INK);
    }
    let bottom = sheet.y - 6.0;
    sheet.page.line(x0, top, x0, bottom, 0.6, INK);
    sheet.page.line(x0, bottom, x1, bottom, 0.6, INK);
    sheet.page.text(x0, bottom - 10.0, &format!("{floor}%"), Font::Regular, 7.5, MUTED);
    sheet.page.text_right(x1, bottom - 10.0, "100%", Font::Regular, 7.5, MUTED);
    let mut legend = format!("Scale starts at {floor}%.");
    if let Some(goal) = objective
        && goal > floor
    {
        let x = scale(goal);
        sheet.page.dashed_line(x, top + 2.0, x, bottom, 0.9, 2.0, INK);
        legend.push_str(&format!(" The dashed line is the objective, {goal:.3}%."));
    }
    if total > bars.len() {
        legend.push_str(&format!(" {} more not shown.", total - bars.len()));
    }
    sheet.y = bottom - 12.0;
    sheet.note(&legend);
}

fn groups_table(sheet: &mut Sheet, data: &ReportData, objective: Option<f64>) {
    sheet.heading("Availability by group");
    // Une colonne « Objective » seulement quand un groupe a le sien.
    let own = data.groups.iter().any(|g| g.objective.is_some());
    let mut specs = vec![
        ("Group", 3.0, false),
        ("Monitors", 0.9, true),
        ("Availability", 1.2, true),
        ("Status", 0.9, false),
        ("Previous", 1.2, true),
    ];
    if own {
        specs.insert(3, ("Objective", 1.1, true));
    }
    let placed = table_open(sheet, &specs);
    for group in &data.groups {
        let goal = group.objective.or(objective);
        let word = status_word(group.uptime, goal);
        let mut cells = vec![
            plain(group.name.clone()),
            plain(group.devices.to_string()),
            (pct(group.uptime), INK, Font::Bold),
        ];
        if own {
            cells.push(muted(goal.map_or_else(|| "-".to_string(), |g| pct(Some(g)))));
        }
        cells.push(status_cell(word));
        cells.push(muted(pct(group.previous_uptime)));
        table_row(sheet, &placed, &cells);
    }
}

fn monitors_table(sheet: &mut Sheet, data: &ReportData, objective: Option<f64>, cap: usize) {
    sheet.heading("Availability by monitor");
    if data.devices.is_empty() {
        sheet.note("No monitor is being monitored.");
        return;
    }
    // Une colonne « Objective » seulement quand un moniteur (ou son groupe) a le sien.
    let own = data.devices.iter().any(|d| d.objective.is_some());
    let mut specs = vec![
        ("Monitor", 3.0, false),
        ("Group", 1.7, false),
        ("Availability", 1.25, true),
        ("Status", 0.95, false),
        ("Incidents", 0.9, true),
        ("Downtime", 1.0, true),
    ];
    if own {
        specs.insert(3, ("Objective", 1.1, true));
    }
    let placed = table_open(sheet, &specs);
    for device in data.devices.iter().take(cap) {
        let goal = device.objective.or(objective);
        let word = status_word(device.uptime, goal);
        let mut cells = vec![
            plain(device.name.clone()),
            muted(device.group.clone()),
            (pct(device.uptime), INK, Font::Bold),
        ];
        if own {
            cells.push(muted(goal.map_or_else(|| "-".to_string(), |g| pct(Some(g)))));
        }
        cells.push(status_cell(word));
        cells.push(plain(device.incidents.to_string()));
        cells.push(plain(duration(TimeDelta::seconds(device.downtime_secs))));
        table_row(sheet, &placed, &cells);
    }
    if data.devices.len() > cap {
        sheet.note(&format!(
            "{} more monitor(s) are in the detailed report and the CSV export.",
            data.devices.len() - cap
        ));
    }
}

fn incident_started(incident: &Incident, tz: Tz) -> String {
    if incident.began_before_period {
        "before period".to_string()
    } else {
        when(incident.started, tz)
    }
}

fn incident_length(incident: &Incident, end: DateTime<Utc>) -> String {
    let mut length = duration(incident.duration(end));
    if incident.ended.is_none() {
        length.push_str(" (ongoing)");
    }
    length
}

fn incidents_table(sheet: &mut Sheet, data: &ReportData, tz: Tz, list: &[&Incident], title: &str) {
    sheet.heading(title);
    if list.is_empty() {
        sheet.note("No incident in this period.");
        return;
    }
    let placed = table_open(
        sheet,
        &[
            ("Monitor", 2.6, false),
            ("Started", 1.8, false),
            ("Duration", 1.4, false),
            ("Severity", 1.0, false),
            ("Rule", 3.0, false),
        ],
    );
    for incident in list {
        let critical = incident.severity == "critical";
        table_row(
            sheet,
            &placed,
            &[
                plain(incident.device.clone()),
                plain(incident_started(incident, tz)),
                plain(incident_length(incident, data.period.end)),
                (incident.severity.clone(), INK, if critical { Font::Bold } else { Font::Regular }),
                plain(incident.rule.clone()),
            ],
        );
    }
}

fn maintenance_table(sheet: &mut Sheet, data: &ReportData, tz: Tz) {
    sheet.heading("Maintenance windows");
    if data.maintenance.is_empty() {
        sheet.note("No maintenance window in this period.");
        return;
    }
    let placed = table_open(
        sheet,
        &[
            ("Window", 2.6, false),
            ("Monitor", 2.2, false),
            ("Starts", 1.7, false),
            ("Ends", 1.7, false),
            ("Repeats", 0.9, false),
        ],
    );
    for window in &data.maintenance {
        table_row(
            sheet,
            &placed,
            &[
                plain(window.name.clone()),
                muted(window.monitor.clone().unwrap_or_else(|| "Several monitors".to_string())),
                plain(when(window.starts, tz)),
                plain(when(window.ends, tz)),
                plain(if window.recurring { "Yes" } else { "No" }),
            ],
        );
    }
    if data.maintenance.len() >= super::collect::MAX_MAINTENANCE_WINDOWS {
        sheet.note("Only the first windows of the period are listed.");
    }
}

/// Les sondes qui mesurent un temps de réponse, les plus lentes d'abord.
fn timed(data: &ReportData) -> Vec<&DeviceStat> {
    let mut list: Vec<&DeviceStat> =
        data.devices.iter().filter(|d| d.latency_avg_ms.is_some()).collect();
    list.sort_by(|a, b| {
        b.latency_p95_ms
            .unwrap_or(0.0)
            .total_cmp(&a.latency_p95_ms.unwrap_or(0.0))
            .then(a.name.cmp(&b.name))
    });
    list
}

fn response_table(sheet: &mut Sheet, data: &ReportData) {
    sheet.heading("Response times");
    let list = timed(data);
    if list.is_empty() {
        sheet.note("No probe measured a response time in this period.");
        return;
    }
    let placed = table_open(
        sheet,
        &[("Monitor", 3.2, false), ("Group", 2.0, false), ("Mean", 1.0, true), ("P95", 1.0, true)],
    );
    for device in list.iter().take(MAX_PDF_MONITORS) {
        table_row(
            sheet,
            &placed,
            &[
                plain(device.name.clone()),
                muted(device.group.clone()),
                plain(ms(device.latency_avg_ms)),
                (ms(device.latency_p95_ms), INK, Font::Bold),
            ],
        );
    }
    if list.len() > MAX_PDF_MONITORS {
        sheet.note(&format!(
            "{} more probe(s) are in the CSV export.",
            list.len() - MAX_PDF_MONITORS
        ));
    }
}

fn notes_section(sheet: &mut Sheet, notes: &str) {
    sheet.heading("Notes");
    sheet.paragraph(notes);
}

/// Le PDF du rapport, selon le gabarit et les sections de `options.layout`.
pub fn pdf(data: &ReportData, tz: Tz, options: &Options) -> Vec<u8> {
    let layout = &options.layout;
    let sections = layout.sections;
    let logo = layout.logo_source() != LogoSource::Hidden && options.logo.is_some();
    let mut sheet = Sheet::new(layout.paper);
    first_header(&mut sheet, data, tz, options, logo);

    if sections.summary {
        summary_section(&mut sheet, data, options);
    }
    if sections.chart {
        chart_section(&mut sheet, data, options.objective);
    }
    match layout.template {
        Template::Detailed => {
            if sections.monitors {
                if has_groups(data) {
                    groups_table(&mut sheet, data, options.objective);
                }
                monitors_table(&mut sheet, data, options.objective, MAX_PDF_MONITORS);
            }
            if sections.incidents {
                let list: Vec<&Incident> = data.incidents.iter().take(MAX_PDF_INCIDENTS).collect();
                incidents_table(&mut sheet, data, tz, &list, "Incidents");
                if data.incidents.len() > MAX_PDF_INCIDENTS {
                    sheet.note(&format!(
                        "{} more incident(s) are in the CSV export.",
                        data.incidents.len() - MAX_PDF_INCIDENTS
                    ));
                }
            }
            if sections.maintenance {
                maintenance_table(&mut sheet, data, tz);
            }
            if sections.response_times {
                response_table(&mut sheet, data);
            }
        }
        Template::Summary => {
            if sections.monitors {
                if has_groups(data) {
                    groups_table(&mut sheet, data, options.objective);
                } else {
                    monitors_table(&mut sheet, data, options.objective, MAX_SUMMARY_MONITORS);
                }
            }
            if sections.incidents {
                let major = major_incidents(data);
                incidents_table(&mut sheet, data, tz, &major, "Major incidents");
                if data.incidents.len() > major.len() {
                    sheet.note(&format!(
                        "{} incident(s) in total; the detailed report lists them all.",
                        data.incidents.len()
                    ));
                }
            }
            if sections.maintenance {
                sheet.heading("Maintenance");
                let total: i64 =
                    data.maintenance.iter().map(|w| (w.ends - w.starts).num_seconds()).sum();
                sheet.note(&match data.maintenance.len() {
                    0 => "No maintenance window in this period.".to_string(),
                    n => format!(
                        "{n} maintenance window(s), {} in total.",
                        duration(TimeDelta::seconds(total))
                    ),
                });
            }
            if sections.response_times {
                sheet.heading("Response times");
                let list = timed(data);
                let slowest: Vec<String> = list
                    .iter()
                    .take(3)
                    .map(|d| format!("{} (P95 {})", one_line(&d.name), ms(d.latency_p95_ms)))
                    .collect();
                sheet.note(&if slowest.is_empty() {
                    "No probe measured a response time in this period.".to_string()
                } else {
                    format!("{} probe(s) measured. Slowest: {}.", list.len(), slowest.join(", "))
                });
            }
        }
    }
    if sections.notes && !layout.notes.trim().is_empty() {
        notes_section(&mut sheet, &layout.notes);
    }

    // Méthode.
    sheet.heading("How this is measured");
    sheet.note(
        "Availability is the share of the period during which each monitor was up: probes \
         count the checks that succeeded, devices count the 5-minute slots in which they \
         reported. The comparison is with the period of the same length just before it. \
         Response times are the mean and 95th percentile of probe durations over the period.",
    );

    // Rappel d'en-tête et pieds de page, une fois le nombre de pages connu.
    sheet.break_page();
    let (left, right, top) = (sheet.left, sheet.right, sheet.top);
    let pages = std::mem::take(&mut sheet.pages);
    let total = pages.len();
    let stamp = options.generated.format("%Y-%m-%d %H:%M UTC").to_string();
    let title = heading_title(layout);
    let (width, height) = layout.paper.size();
    let mut doc = Document::new(
        &format!("{} - {}", one_line(&options.brand), one_line(&title)),
        Some(options.generated.format("D:%Y%m%d%H%M%SZ").to_string()),
    )
    .with_size(width, height);
    if logo && let Some(image) = &options.logo {
        doc.add_image(image.clone());
    }
    let period = period_label(data, tz);
    let running = pdf::fit(
        &format!("{} - {}", one_line(&options.brand), one_line(&title)),
        Font::Bold,
        8.0,
        (right - left) * 0.6,
    );
    for (index, mut page) in pages.into_iter().enumerate() {
        if index > 0 {
            page.text(left, top - 8.0, &running, Font::Bold, 8.0, MUTED);
            page.text_right(right, top - 8.0, &period, Font::Regular, 8.0, MUTED);
            page.line(left, top - 14.0, right, top - 14.0, 0.4, RULE);
        }
        page.line(left, FOOTER_RULE, right, FOOTER_RULE, 0.4, RULE);
        page.text(
            left,
            FOOTER_TEXT,
            &format!("DumbMonit - generated {stamp}"),
            Font::Regular,
            8.0,
            MUTED,
        );
        page.text_right(
            right,
            FOOTER_TEXT,
            &format!("Page {} of {total}", index + 1),
            Font::Regular,
            8.0,
            MUTED,
        );
        doc.add_page(page);
    }
    doc.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reports::collect::{DeviceInfo, Inputs, assemble};
    use crate::reports::schedule::{Frequency, Period};
    use std::collections::HashMap;

    fn at(iso: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(iso).expect("ISO date").with_timezone(&Utc)
    }

    fn data_for(devices: Vec<DeviceInfo>, uptime: HashMap<i64, f64>) -> ReportData {
        let period = Period::custom(at("2026-10-01T00:00:00Z"), at("2026-11-01T00:00:00Z"));
        assemble(Inputs {
            name: "Monthly SLA",
            frequency: Frequency::Monthly,
            period,
            link: None,
            devices: &devices,
            rule_names: &HashMap::new(),
            uptime: &uptime,
            previous_uptime: &HashMap::new(),
            availability_known: true,
            incidents: &[],
            latency: &HashMap::from([(1, (120.4, 340.0))]),
        })
    }

    fn sample() -> ReportData {
        let devices = vec![
            DeviceInfo { id: 1, name: "Web, \"front\"".into(), group: "Edge".into() },
            DeviceInfo { id: 2, name: "=HYPERLINK(\"http://x\")".into(), group: String::new() },
            DeviceInfo { id: 3, name: "Idle".into(), group: "Edge".into() },
        ];
        data_for(devices, HashMap::from([(1, 99.95), (2, 98.5)]))
    }

    #[test]
    fn file_names_follow_the_period() {
        let data = sample();
        assert_eq!(file_stem(&data, Tz::UTC), "availability-2026-10");
        assert_eq!(period_label(&data, Tz::UTC), "2026-10-01 to 2026-10-31 (UTC)");
        assert_eq!(month_label(&data, Tz::UTC), "October 2026");
        let mut partial = sample();
        partial.period = Period::custom(at("2026-10-05T00:00:00Z"), at("2026-10-12T00:00:00Z"));
        assert_eq!(file_stem(&partial, Tz::UTC), "availability-2026-10-05_2026-10-11");
    }

    #[test]
    fn csv_cells_are_quoted_and_cannot_become_formulas() {
        assert_eq!(csv_cell("plain"), "plain");
        assert_eq!(csv_cell("a,b"), "\"a,b\"");
        assert_eq!(csv_cell("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_cell("=1+1"), "'=1+1");
        assert_eq!(csv_cell("@SUM(A1)"), "'@SUM(A1)");
        assert_eq!(csv_cell("-2"), "'-2");
        assert_eq!(csv_cell("two\nlines"), "two lines");
    }

    #[test]
    fn the_monitor_csv_has_one_row_per_monitor() {
        let csv = csv_monitors(&sample(), None);
        assert!(csv.starts_with("\u{feff}group,monitor,availability_percent,"));
        assert!(csv.lines().count() == 4, "{csv}");
        assert!(csv.contains("Edge,\"Web, \"\"front\"\"\",99.950,,0,0,120.4,340.0\r\n"), "{csv}");
        assert!(csv.contains("'=HYPERLINK("), "formulas are neutralised");
        assert!(!csv.contains("objective"));
        let with = csv_monitors(&sample(), Some(99.9));
        assert!(with.contains("objective_percent,objective"));
        assert!(with.contains(",99.900,met\r\n"), "{with}");
        assert!(with.contains(",99.900,missed\r\n"));
        assert!(with.contains(",99.900,no_data\r\n"));
    }

    #[test]
    fn the_objective_is_compared_with_a_tiny_tolerance() {
        assert_eq!(status_word(Some(99.9), Some(99.9)), "Met");
        assert_eq!(status_word(Some(99.899), Some(99.9)), "Missed");
        assert_eq!(status_word(None, Some(99.9)), "No data");
        assert_eq!(status_word(Some(99.95), None), "Healthy");
    }

    #[test]
    fn a_group_or_monitor_objective_replaces_the_one_of_the_report() {
        let mut data = sample();
        let mut objectives = crate::reports::layout::Objectives::default();
        objectives.groups.insert("Edge".into(), 99.99);
        objectives.monitors.insert("Idle".into(), 90.0);
        data.apply_objectives(&objectives);
        let web = data.devices.iter().find(|d| d.name.starts_with("Web")).expect("web");
        assert_eq!(web.objective, Some(99.99), "the group's objective");
        let idle = data.devices.iter().find(|d| d.name == "Idle").expect("idle");
        assert_eq!(idle.objective, Some(90.0), "the monitor beats its group");
        let ungrouped = data.devices.iter().find(|d| d.name.starts_with('=')).expect("other");
        assert_eq!(ungrouped.objective, None);
        // 99.95 meets the report's 99.9 but not the group's 99.99.
        let csv = csv_monitors(&data, Some(99.9));
        assert!(csv.contains(",99.990,missed\r\n"), "{csv}");
        assert!(csv.contains(",90.000,no_data\r\n"), "{csv}");
        assert!(csv.contains(",99.900,missed\r\n"), "the report's own objective elsewhere: {csv}");
        // The tables gain an "Objective" column only when a row has its own.
        let text = text_of(&pdf(&data, Tz::UTC, &options()));
        assert!(text.contains("(Objective)"), "own objectives are shown");
        let plain = text_of(&pdf(&sample(), Tz::UTC, &options()));
        assert!(!plain.contains("(Objective)"), "no column without overrides");
    }

    #[test]
    fn the_incident_csv_lists_incidents_in_utc() {
        let mut data = sample();
        data.incidents.push(crate::reports::collect::Incident {
            device: "NAS".into(),
            rule: "Monitor unreachable".into(),
            severity: "critical".into(),
            started: at("2026-10-06T10:05:00Z"),
            ended: Some(at("2026-10-06T11:05:00Z")),
            began_before_period: false,
        });
        let csv = csv_incidents(&data);
        assert!(csv.contains("NAS,Monitor unreachable,critical,2026-10-06T10:05:00Z,2026-10-06T11:05:00Z,3600,false,false\r\n"), "{csv}");
    }

    fn options() -> Options {
        Options {
            brand: "Acme (Ops)".into(),
            objective: Some(99.9),
            generated: at("2026-11-03T08:00:00Z"),
            logo: None,
            logo_url: None,
            layout: Layout::default(),
        }
    }

    fn with_layout(layout: Layout) -> Options {
        Options { layout, ..options() }
    }

    fn text_of(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).into_owned()
    }

    fn page_count(text: &str) -> usize {
        text.matches("/Type /Page ").count()
    }

    fn with_incidents() -> ReportData {
        let mut data = sample();
        for (i, severity) in
            ["warning", "critical", "warning", "critical", "warning", "warning"].iter().enumerate()
        {
            data.incidents.push(crate::reports::collect::Incident {
                device: format!("dev-{i}"),
                rule: format!("rule-{i}"),
                severity: (*severity).into(),
                started: at("2026-10-06T10:00:00Z"),
                ended: Some(at("2026-10-06T10:00:00Z") + TimeDelta::minutes(10 * (i as i64 + 1))),
                began_before_period: false,
            });
        }
        data
    }

    #[test]
    fn the_pdf_names_the_organisation_and_the_period() {
        let bytes = pdf(&sample(), Tz::UTC, &options());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("Acme \\(Ops\\)"));
        assert!(text.contains("Availability report"));
        assert!(text.contains("2026-10-01 to 2026-10-31 \\(UTC\\)"));
        assert!(text.contains("SLA objective 99.900%"));
        let pages = page_count(&text);
        assert!(text.contains(&format!("Page {pages} of {pages}")));
        assert!(text.contains("/CreationDate (D:20261103080000Z)"));
        // Deux appels donnent les mêmes octets : rien d'aléatoire.
        assert_eq!(bytes, pdf(&sample(), Tz::UTC, &options()));
    }

    #[test]
    fn a_long_table_runs_over_several_pages() {
        let devices: Vec<DeviceInfo> = (0..120)
            .map(|i| DeviceInfo { id: i, name: format!("monitor-{i}"), group: "G".into() })
            .collect();
        let uptime = (0..120).map(|i| (i, 99.0 + (i % 10) as f64 / 10.0)).collect();
        let bytes = pdf(&data_for(devices, uptime), Tz::UTC, &options());
        let text = String::from_utf8_lossy(&bytes);
        let pages = text.matches("/Type /Page ").count();
        assert!(pages >= 3, "{pages} pages");
        assert!(text.contains(&format!("Page {pages} of {pages}")));
    }

    #[test]
    fn an_empty_estate_still_gives_a_document() {
        let bytes = pdf(&data_for(Vec::new(), HashMap::new()), Tz::UTC, &options());
        assert!(String::from_utf8_lossy(&bytes).contains("No monitor is being monitored."));
    }

    #[test]
    fn the_detailed_template_has_every_section() {
        let mut data = with_incidents();
        data.maintenance.push(crate::reports::collect::MaintenanceWindow {
            name: "Firmware upgrade".into(),
            monitor: Some("Web".into()),
            starts: at("2026-10-10T02:00:00Z"),
            ends: at("2026-10-10T04:00:00Z"),
            recurring: false,
        });
        let options = with_layout(Layout { notes: "Migration done.".into(), ..Layout::default() });
        let text = text_of(&pdf(&data, Tz::UTC, &options));
        for heading in [
            "Availability by group, lowest first",
            "Availability by group",
            "Availability by monitor",
            "Incidents",
            "Maintenance windows",
            "Firmware upgrade",
            "Response times",
            "Notes",
            "Migration done.",
            "How this is measured",
            "Scale starts at",
            "dashed line is the objective",
        ] {
            assert!(text.contains(heading), "{heading} is missing");
        }
        assert!(text.contains("(Missed)"), "a missed objective is written out");
        assert!(text.contains("/MediaBox [0 0 595 842]"), "A4 by default");
    }

    #[test]
    fn the_summary_template_is_short_and_keeps_the_major_incidents() {
        let devices: Vec<DeviceInfo> = (0..120)
            .map(|i| DeviceInfo { id: i, name: format!("monitor-{i}"), group: String::new() })
            .collect();
        let uptime = (0..120).map(|i| (i, 99.0 + (i % 10) as f64 / 10.0)).collect();
        let mut data = data_for(devices, uptime);
        data.incidents = with_incidents().incidents;
        let detailed = text_of(&pdf(&data, Tz::UTC, &options()));
        let summary_options =
            with_layout(Layout { template: Template::Summary, ..Layout::default() });
        let summary = text_of(&pdf(&data, Tz::UTC, &summary_options));
        assert!(page_count(&summary) <= 2, "{} pages", page_count(&summary));
        assert!(page_count(&detailed) > page_count(&summary));
        assert!(summary.contains("Major incidents"));
        assert!(summary.contains("6 incident\\(s\\) in total"), "the total is named");
        // Les deux critiques d'abord, la plus longue en tête.
        let first = summary.find("(dev-3)").expect("dev-3 listed");
        let second = summary.find("(dev-1)").expect("dev-1 listed");
        assert!(first < second);
        assert!(!summary.contains("(dev-0)"), "only five major incidents");
        assert!(summary.contains("more monitor\\(s\\) are in the detailed report"));
    }

    #[test]
    fn unchecked_sections_are_left_out() {
        let mut sections = crate::reports::layout::Sections::none();
        sections.summary = true;
        let layout =
            Layout { sections, notes: "Should not be printed.".into(), ..Layout::default() };
        let text = text_of(&pdf(&with_incidents(), Tz::UTC, &with_layout(layout)));
        assert!(text.contains("(Availability)"), "the summary stays");
        for absent in [
            "Availability by monitor",
            "Availability by group",
            "(Severity)",
            "(dev-1)",
            "Maintenance windows",
            "(Response times)",
            "(Notes)",
            "Should not be printed.",
            "Scale starts at",
        ] {
            assert!(!text.contains(absent), "{absent} should be left out");
        }
    }

    #[test]
    fn paper_title_and_logo_follow_the_layout() {
        let logo = pdf::Image {
            width: 1,
            height: 1,
            color_space: "DeviceGray",
            filter: "FlateDecode",
            data: vec![0x78, 0x9c, 0xff],
            alpha: None,
        };
        let layout = Layout {
            paper: Paper::Letter,
            title: Some("Quarterly service review".into()),
            ..Layout::default()
        };
        let mut options = with_layout(layout.clone());
        options.logo = Some(logo.clone());
        let text = text_of(&pdf(&sample(), Tz::UTC, &options));
        assert!(text.contains("/MediaBox [0 0 612 792]"));
        assert!(text.contains("(Quarterly service review)"));
        assert!(text.contains("/Title (Acme \\(Ops\\) - Quarterly service review)"));
        assert!(text.contains("/Im0 Do"));

        let mut hidden = with_layout(Layout { show_logo: false, ..layout });
        hidden.logo = Some(logo);
        let text = text_of(&pdf(&sample(), Tz::UTC, &hidden));
        assert!(!text.contains("/XObject"), "no logo when it is turned off");
    }

    #[test]
    fn a_long_report_repeats_the_header_on_every_page() {
        let devices: Vec<DeviceInfo> = (0..120)
            .map(|i| DeviceInfo { id: i, name: format!("monitor-{i}"), group: "G".into() })
            .collect();
        let uptime = (0..120).map(|i| (i, 99.0 + (i % 10) as f64 / 10.0)).collect();
        let text = text_of(&pdf(&data_for(devices, uptime), Tz::UTC, &options()));
        let pages = page_count(&text);
        assert!(pages >= 3);
        // Le rappel de la marque en tête de chaque page après la première, plus
        // le titre des métadonnées, qui porte le même texte.
        let running = text.matches("(Acme \\(Ops\\) - Availability report)").count();
        assert_eq!(running, pages, "running header on pages 2..{pages}");
        // La ligne d'en-tête du tableau des moniteurs, répétée.
        assert!(text.matches("(Monitor) Tj").count() >= pages - 1);
        // Aucun fond plein sous les tableaux : seulement les barres du graphe.
        assert!(text.matches(" re f").count() <= 15, "no background fills");
    }

    #[test]
    fn notes_are_wrapped_to_the_page_width() {
        let long = "word ".repeat(200);
        let lines = wrap(&long, Font::Regular, 10.0, 300.0);
        assert!(lines.len() > 5);
        assert!(lines.iter().all(|l| pdf::text_width(l, Font::Regular, 10.0) <= 300.0));
        assert_eq!(wrap("a\n\nb", Font::Regular, 10.0, 300.0), vec!["a", "", "b"]);
    }
}
