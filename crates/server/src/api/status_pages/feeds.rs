//! Badge SVG et flux RSS d'une page.

use super::*;

// --------------------------------------------------------------------------
// Badge et flux
// --------------------------------------------------------------------------

pub(super) fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Badge d'état global de la page : « status | operational ».
pub(super) fn render_badge(overall: &str, style: badges::Style) -> String {
    let (word, colour) = badges::overall_word(overall);
    badges::render_styled("status", word, colour, style)
}

pub async fn public_badge(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    Query(query): Query<badges::StyleQuery>,
) -> ApiResult<Response> {
    let style = badges::Style::parse(query.style.as_deref())?;
    let (body, private) = load_public_checked(&state, &slug, &headers).await?;
    let overall = body.get("overall").and_then(Value::as_str).unwrap_or("unknown");
    Ok(access::seal(
        (
            [
                (header::CONTENT_TYPE, "image/svg+xml; charset=utf-8"),
                (header::CACHE_CONTROL, "public, max-age=60"),
            ],
            render_badge(overall, style),
        )
            .into_response(),
        private,
    ))
}

// L'origine publique vient de `oidc::Origin` : les en-têtes `X-Forwarded-*` n'y
// comptent que s'ils viennent d'un mandataire de confiance
// (`DUMBMONIT_TRUSTED_PROXIES`). Sinon un client choisirait le lien d'un flux RSS
// mis en cache, ou celui des e-mails d'abonnement.

pub(super) fn rfc2822(stored: &str) -> String {
    parse_stored(stored).map(|at| at.to_rfc2822()).unwrap_or_default()
}

/// Réglages du flux RSS.
#[derive(Debug, Deserialize)]
pub struct RssQuery {
    /// Clé publique d'un service : le flux ne garde que les annonces qui le
    /// touchent (et celles qui valent pour toute la page).
    #[serde(default)]
    pub(super) component: Option<String>,
}

pub async fn public_rss(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    origin: crate::api::oidc::Origin,
    Query(query): Query<RssQuery>,
) -> ApiResult<Response> {
    let (body, private) = load_public_checked(&state, &slug, &headers).await?;
    // Le domaine public de la page s'il y en a un ; sinon l'origine de la requête.
    let domain =
        db::status_pages::get_page_by_slug(&state.pool, &slug).await?.and_then(|page| page.domain);
    let link = match domain {
        Some(domain) => format!("https://{domain}/"),
        None => format!("{}/s/{slug}", origin.as_str()),
    };
    let title = body.pointer("/page/title").and_then(Value::as_str).unwrap_or("Status");
    let description = body.pointer("/page/description").and_then(Value::as_str).unwrap_or("");

    let mut items = String::new();
    let entries = ["incidents", "maintenance"]
        .into_iter()
        .filter_map(|key| body.get(key).and_then(Value::as_array))
        .flatten();
    let component = query.component.as_deref().map(str::trim).filter(|c| !c.is_empty());
    for entry in entries {
        if let Some(wanted) = component {
            let touched = entry.get("components").and_then(Value::as_array);
            if touched.is_some_and(|keys| !keys.iter().any(|key| key.as_str() == Some(wanted))) {
                continue;
            }
        }
        let field = |name: &str| entry.get(name).and_then(Value::as_str).unwrap_or("");
        let permalink = match entry.get("anchor").and_then(Value::as_str) {
            Some(anchor) => format!("{link}#incident-{anchor}"),
            None => link.clone(),
        };
        let latest = entry
            .get("updates")
            .and_then(Value::as_array)
            .and_then(|updates| updates.last())
            .and_then(|update| update.get("body"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let kind = if field("kind") == "maintenance" { "Maintenance" } else { "Incident" };
        items.push_str(&format!(
            "<item><title>{kind}: {title} [{status}]</title><link>{link}</link>\
             <guid isPermaLink=\"false\">{guid}</guid><pubDate>{date}</pubDate>\
             <description>{description}</description></item>",
            title = xml_escape(field("title")),
            status = xml_escape(field("status")),
            link = xml_escape(&permalink),
            guid = xml_escape(&format!("{slug}:{}:{}", field("starts_at"), field("title"))),
            date = rfc2822(field("starts_at")),
            description = xml_escape(latest),
        ));
    }

    let feed = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <rss version=\"2.0\"><channel><title>{title} — incidents</title><link>{link}</link>\
         <description>{description}</description>{items}</channel></rss>",
        title = xml_escape(title),
        link = xml_escape(&link),
        description = xml_escape(description),
    );
    Ok(access::seal(
        (
            [
                (header::CONTENT_TYPE, "application/rss+xml; charset=utf-8"),
                (header::CACHE_CONTROL, "public, max-age=30"),
            ],
            feed,
        )
            .into_response(),
        private,
    ))
}
