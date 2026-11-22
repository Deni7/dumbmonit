//! Badges SVG des pages de statut : état, disponibilité et temps de réponse,
//! pour la page entière ou pour un service.
//!
//! Chaque badge est tiré du document public déjà calculé (et mis en cache) :
//! il ne peut donc rien dire que la page ne montre pas. Les couleurs sont
//! pleines, avec du texte blanc à au moins 4,5:1 : le badge se lit aussi bien
//! sur le fond clair que sur le fond sombre d'un README GitHub.

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::Value;

use super::{access, load_public_checked, round};
use crate::api::{ApiError, ApiResult};
use crate::state::AppState;

/// Vert, ambre, rouge, bleu et gris, tous lisibles en blanc (≥ 4,5:1).
const GREEN: &str = "#1a7f37";
const AMBER: &str = "#8a5a00";
const RED: &str = "#c62828";
const BLUE: &str = "#0b5cad";
const GREY: &str = "#5c636e";
/// Fond de l'étiquette (partie gauche).
const LABEL_BG: &str = "#454b54";

/// Un badge peut rester une minute dans un cache — celui de GitHub compris.
const CACHE: &str = "public, max-age=60";

/// Fenêtres de disponibilité qu'un badge sait dire, et le champ qui les porte.
const WINDOWS: [(u32, &str); 4] =
    [(1, "uptime_24h"), (7, "uptime_7d"), (30, "uptime_30d"), (90, "uptime_90d")];

#[derive(Debug, Deserialize)]
pub struct UptimeQuery {
    #[serde(default)]
    days: Option<u32>,
    /// `flat` (défaut), `flat-square` ou `for-the-badge`.
    #[serde(default)]
    style: Option<String>,
}

/// Paramètre `?style=` seul, pour les badges qui n'ont pas d'autre réglage.
#[derive(Debug, Deserialize)]
pub struct StyleQuery {
    #[serde(default)]
    pub style: Option<String>,
}

fn svg_response(svg: String) -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"), (header::CACHE_CONTROL, CACHE)], svg)
        .into_response()
}

fn xml_escape(text: &str) -> String {
    super::xml_escape(text)
}

/// Largeur approximative d'un texte en Verdana 11 px : les majuscules et les
/// chiffres sont plus larges que les minuscules étroites.
fn text_width(text: &str) -> u32 {
    let width: f64 = text
        .chars()
        .map(|c| match c {
            'i' | 'l' | 'j' | '.' | ',' | ':' | '·' | '|' | '!' | '\'' | ' ' => 3.6,
            'f' | 't' | 'r' | '(' | ')' => 4.8,
            'm' | 'w' | 'M' | 'W' | '%' => 10.0,
            c if c.is_ascii_uppercase() || c.is_ascii_digit() => 7.5,
            _ => 6.8,
        })
        .sum();
    width.ceil() as u32
}

/// Style d'un badge, au vocabulaire de shields.io.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Style {
    /// Coins légèrement arrondis (défaut).
    #[default]
    Flat,
    /// Coins droits.
    FlatSquare,
    /// Grand format, majuscules espacées.
    ForTheBadge,
}

impl Style {
    /// Lit le paramètre `?style=` ; absent ou vide : le style par défaut.
    pub(super) fn parse(raw: Option<&str>) -> ApiResult<Self> {
        match raw.map(str::trim).filter(|s| !s.is_empty()) {
            None | Some("flat") => Ok(Self::Flat),
            Some("flat-square") => Ok(Self::FlatSquare),
            Some("for-the-badge") => Ok(Self::ForTheBadge),
            Some(_) => Err(ApiError::BadRequest(
                "style must be flat, flat-square or for-the-badge.".into(),
            )),
        }
    }
}

/// Badge à la manière de shields.io : « étiquette | valeur ».
#[cfg(test)]
pub(super) fn render(label: &str, value: &str, colour: &str) -> String {
    render_styled(label, value, colour, Style::Flat)
}

/// Comme [`render`], dans le style demandé.
pub(super) fn render_styled(label: &str, value: &str, colour: &str, style: Style) -> String {
    let label: String = label.chars().take(40).collect();
    let (label, value, height, radius, pad, font_size, y, extra) = match style {
        Style::Flat => (label, value.to_string(), 20, 3, 12, 11, 14, ""),
        Style::FlatSquare => (label, value.to_string(), 20, 0, 12, 11, 14, ""),
        Style::ForTheBadge => (
            label.to_uppercase(),
            value.to_uppercase(),
            28,
            0,
            24,
            10,
            18,
            " font-weight=\"700\" letter-spacing=\"1\"",
        ),
    };
    // Les majuscules espacées de « for-the-badge » prennent une unité par lettre.
    let spacing = |text: &str| if extra.is_empty() { 0 } else { text.chars().count() as u32 };
    let label_w = text_width(&label) + spacing(&label) + pad;
    let value_w = text_width(&value) + spacing(&value) + pad;
    let total = label_w + value_w;
    let label = xml_escape(&label);
    let value = xml_escape(&value);
    format!(
        concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{total}\" height=\"{height}\" ",
            "role=\"img\" aria-label=\"{label}: {value}\">",
            "<title>{label}: {value}</title>",
            "<clipPath id=\"r\"><rect width=\"{total}\" height=\"{height}\" rx=\"{radius}\" fill=\"#fff\"/></clipPath>",
            "<g clip-path=\"url(#r)\">",
            "<rect width=\"{label_w}\" height=\"{height}\" fill=\"{label_bg}\"/>",
            "<rect x=\"{label_w}\" width=\"{value_w}\" height=\"{height}\" fill=\"{colour}\"/></g>",
            "<g fill=\"#fff\" text-anchor=\"middle\" ",
            "font-family=\"Inter,system-ui,-apple-system,Segoe UI,sans-serif\" font-size=\"{font_size}\"{extra}>",
            "<text x=\"{label_x}\" y=\"{y}\">{label}</text>",
            "<text x=\"{value_x}\" y=\"{y}\">{value}</text></g></svg>"
        ),
        total = total,
        height = height,
        radius = radius,
        font_size = font_size,
        extra = extra,
        y = y,
        label = label,
        value = value,
        label_w = label_w,
        value_w = value_w,
        label_bg = LABEL_BG,
        colour = colour,
        label_x = label_w / 2,
        value_x = label_w + value_w / 2,
    )
}

/// Mot et couleur d'un état global de page.
pub(super) fn overall_word(overall: &str) -> (&'static str, &'static str) {
    match overall {
        "operational" => ("operational", GREEN),
        "degraded" => ("degraded", AMBER),
        "major" => ("major outage", RED),
        "maintenance" => ("maintenance", BLUE),
        _ => ("unknown", GREY),
    }
}

/// Mot et couleur d'un état de service.
fn item_word(state: &str) -> (&'static str, &'static str) {
    match state {
        "up" => ("operational", GREEN),
        "degraded" => ("degraded", AMBER),
        "down" => ("down", RED),
        "maintenance" => ("maintenance", BLUE),
        _ => ("no data", GREY),
    }
}

fn uptime_value(value: Option<f64>) -> (String, &'static str) {
    match value {
        None => ("no data".to_string(), GREY),
        Some(pct) => {
            let colour = if pct >= 99.5 {
                GREEN
            } else if pct >= 95.0 {
                AMBER
            } else {
                RED
            };
            let text = if pct >= 100.0 { "100%".to_string() } else { format!("{pct:.2}%") };
            (text, colour)
        }
    }
}

fn latency_value(value: Option<f64>) -> (String, &'static str) {
    match value {
        None => ("no data".to_string(), GREY),
        Some(ms) if ms < 1_000.0 => (format!("{} ms", ms.round() as i64), BLUE),
        Some(ms) => (format!("{:.1} s", ms / 1_000.0), BLUE),
    }
}

/// Champ de disponibilité d'une fenêtre, refusé si la page ne la montre pas.
fn window_field(doc: &Value, days: Option<u32>) -> ApiResult<(u32, &'static str)> {
    let days = days.unwrap_or(30);
    let Some((_, field)) = WINDOWS.iter().find(|(d, _)| *d == days) else {
        return Err(ApiError::BadRequest("days must be 1, 7, 30 or 90.".into()));
    };
    let shown = doc.pointer("/page/show_uptime_days").and_then(Value::as_u64).unwrap_or(90);
    if u64::from(days) > shown {
        return Err(ApiError::BadRequest(format!(
            "This page shows {shown} days of history; a {days}-day badge would say more."
        )));
    }
    Ok((days, field))
}

fn items(doc: &Value) -> impl Iterator<Item = &Value> {
    doc.get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|group| group.get("items").and_then(Value::as_array))
        .flatten()
}

fn find_item<'a>(doc: &'a Value, key: &str) -> ApiResult<&'a Value> {
    items(doc)
        .find(|item| item.get("key").and_then(Value::as_str) == Some(key))
        .ok_or_else(|| ApiError::NotFound("This service is not on the page.".into()))
}

/// Moyenne des valeurs présentes d'un champ, sur tous les services.
fn mean(doc: &Value, field: &str) -> Option<f64> {
    let values: Vec<f64> = items(doc).filter_map(|item| item.get(field)?.as_f64()).collect();
    (!values.is_empty()).then(|| round(values.iter().sum::<f64>() / values.len() as f64))
}

fn window_label(days: u32) -> String {
    if days == 1 { "uptime 24h".to_string() } else { format!("uptime {days}d") }
}

pub async fn page_uptime(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Query(query): Query<UptimeQuery>,
) -> ApiResult<Response> {
    let (doc, private) = load_public_checked(&state, &slug, &headers).await?;
    let style = Style::parse(query.style.as_deref())?;
    let (days, field) = window_field(&doc, query.days)?;
    let (value, colour) = uptime_value(mean(&doc, field));
    let svg = render_styled(&window_label(days), &value, colour, style);
    Ok(access::seal(svg_response(svg), private))
}

pub async fn page_response(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Query(query): Query<StyleQuery>,
) -> ApiResult<Response> {
    let (doc, private) = load_public_checked(&state, &slug, &headers).await?;
    let style = Style::parse(query.style.as_deref())?;
    let (value, colour) = latency_value(mean(&doc, "latency_ms"));
    Ok(access::seal(svg_response(render_styled("response", &value, colour, style)), private))
}

pub async fn item_status(
    State(state): State<AppState>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<StyleQuery>,
) -> ApiResult<Response> {
    let (doc, private) = load_public_checked(&state, &slug, &headers).await?;
    let style = Style::parse(query.style.as_deref())?;
    let item = find_item(&doc, &key)?;
    let label = item.get("label").and_then(Value::as_str).unwrap_or("service");
    let (word, colour) = item_word(item.get("state").and_then(Value::as_str).unwrap_or(""));
    Ok(access::seal(svg_response(render_styled(label, word, colour, style)), private))
}

pub async fn item_uptime(
    State(state): State<AppState>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<UptimeQuery>,
) -> ApiResult<Response> {
    let (doc, private) = load_public_checked(&state, &slug, &headers).await?;
    let style = Style::parse(query.style.as_deref())?;
    let (days, field) = window_field(&doc, query.days)?;
    let item = find_item(&doc, &key)?;
    let label = item.get("label").and_then(Value::as_str).unwrap_or("service");
    let (value, colour) = uptime_value(item.get(field).and_then(Value::as_f64));
    let svg = svg_response(render_styled(
        &format!("{label} · {}", window_label(days)),
        &value,
        colour,
        style,
    ));
    Ok(access::seal(svg, private))
}

pub async fn item_response(
    State(state): State<AppState>,
    Path((slug, key)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<StyleQuery>,
) -> ApiResult<Response> {
    let (doc, private) = load_public_checked(&state, &slug, &headers).await?;
    let style = Style::parse(query.style.as_deref())?;
    let item = find_item(&doc, &key)?;
    let label = item.get("label").and_then(Value::as_str).unwrap_or("service");
    let (value, colour) = latency_value(item.get("latency_ms").and_then(Value::as_f64));
    let svg = svg_response(render_styled(&format!("{label} · response"), &value, colour, style));
    Ok(access::seal(svg, private))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_is_escaped() {
        let svg = render("<script>", "99%", GREEN);
        assert!(!svg.contains("<script>"));
        assert!(svg.contains("&lt;script&gt;"));
    }

    #[test]
    fn uptime_colours_follow_the_thresholds() {
        assert_eq!(uptime_value(Some(99.9)).1, GREEN);
        assert_eq!(uptime_value(Some(97.0)).1, AMBER);
        assert_eq!(uptime_value(Some(80.0)).1, RED);
        assert_eq!(uptime_value(None), ("no data".to_string(), GREY));
        assert_eq!(uptime_value(Some(100.0)).0, "100%");
    }

    #[test]
    fn a_badge_never_says_more_than_the_page() {
        let doc = serde_json::json!({ "page": { "show_uptime_days": 30 } });
        assert!(window_field(&doc, Some(30)).is_ok());
        assert!(window_field(&doc, Some(90)).is_err());
        assert!(window_field(&doc, Some(12)).is_err());
        assert!(matches!(window_field(&doc, None), Ok((30, "uptime_30d"))));
    }

    #[test]
    fn the_style_changes_the_shape() {
        let flat = render_styled("status", "up", GREEN, Style::Flat);
        let square = render_styled("status", "up", GREEN, Style::FlatSquare);
        let big = render_styled("status", "up", GREEN, Style::ForTheBadge);
        assert!(flat.contains("rx=\"3\""));
        assert!(square.contains("rx=\"0\""));
        assert!(big.contains("height=\"28\"") && big.contains("STATUS"));
        assert_eq!(flat, render("status", "up", GREEN));
    }

    #[test]
    fn the_style_parameter_is_checked() {
        assert!(matches!(Style::parse(None), Ok(Style::Flat)));
        assert!(matches!(Style::parse(Some("for-the-badge")), Ok(Style::ForTheBadge)));
        assert!(Style::parse(Some("plastic")).is_err());
    }

    #[test]
    fn latency_reads_in_ms_then_seconds() {
        assert_eq!(latency_value(Some(123.4)).0, "123 ms");
        assert_eq!(latency_value(Some(2_345.0)).0, "2.3 s");
    }
}
