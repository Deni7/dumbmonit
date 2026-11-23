//! Entrées du catalogue : Webhook sur mesure.

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Sur mesure ----------------------------------------------------
        "webhook" => info(
            kind,
            "Custom webhook",
            "HTTP request fully described by you: method, headers, templated body.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#custom-webhook",
            vec![
                select("method", "Method", "A GET request has no body.", &["POST", "PUT", "GET"]),
                select(
                    "content_type",
                    "Content type",
                    "Also determines how template variables are escaped.",
                    &["json", "form", "text"],
                ),
                textarea(
                    "body_template",
                    "Body template",
                    "Variables between double braces, for example {{title}}. Empty, the \
                     standard JSON payload is sent.",
                    "{\"title\": \"{{title}}\", \"detail\": \"{{message}}\"}",
                ),
                url(
                    "base_url",
                    "Public address of DumbMonit",
                    "So that {{link}} gives a clickable link to the device.",
                    "https://dumbmonit.home",
                )
                // Un lien inséré dans le message, pas l'endroit où il part.
                .destination(false),
                text(
                    "username",
                    "Username",
                    "For HTTP \"Basic\" authentication. Requires the password.",
                    "",
                ),
            ],
            vec![
                url(
                    "url",
                    "Address to call",
                    "May contain variables. Secret, because it often carries a token.",
                    "https://api.example.org/incidents",
                )
                .required(),
                password(
                    "token",
                    "Token",
                    "Sent as an \"Authorization: Bearer\" header, unless the template uses \
                     {{token}}.",
                    "",
                ),
                password("password", "Password", "Password for HTTP \"Basic\" authentication.", ""),
                // Secret : c'est là qu'on met naturellement un `X-Api-Key` ou un
                // `Authorization`, et les réglages sont lisibles par tout compte.
                textarea(
                    "headers",
                    "Extra headers",
                    "JSON object, one value per header. Stored encrypted like the token, so API \
                     keys belong here.",
                    "{\"X-Api-Key\": \"…\"}",
                ),
            ],
        ),
        _ => return None,
    })
}
