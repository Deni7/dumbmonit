//! Entrées du catalogue : Services d'astreinte.

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Astreinte -----------------------------------------------------
        "pagerduty" => info(
            kind,
            "PagerDuty",
            "PagerDuty incident opened by the alert, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#pagerduty",
            vec![
                select(
                    "region",
                    "Account region",
                    "\"eu\" if your account is hosted in Europe.",
                    &["us", "eu"],
                )
                .destination(true),
                text("source", "Source", "Source name shown in the incident.", "")
                    .with_default("dumbmonit"),
            ],
            vec![
                password(
                    "token",
                    "Integration Key",
                    "Services → Integrations → Add integration, of type Events API v2.",
                    "0123456789abcdef0123456789abcdef",
                )
                .required(),
            ],
        ),
        "opsgenie" => info(
            kind,
            "Opsgenie",
            "Opsgenie alert opened by the alert, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#opsgenie",
            vec![
                select("region", "Account region", "\"eu\" for a European account.", &["us", "eu"])
                    .destination(true),
                textarea("responders", "Teams to notify", "One team name per line.", "on-call")
                    .list(),
                textarea("tags", "Tags", "One tag per line.", "homelab").list(),
            ],
            vec![
                password(
                    "api_key",
                    "API key",
                    "Teams → Integrations → Add integration → API.",
                    "01234567-89ab-cdef-0123-456789abcdef",
                )
                .required(),
            ],
        ),

        // Astreintes intégrées : rien ne part vers un tiers, le canal ouvre une
        // escalade (`crate::oncall`).
        "oncall" => info(
            kind,
            "DumbMonit on-call",
            "Runs an escalation policy: pages whoever is on call, then the next step until \
             someone acknowledges.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#dumbmonit-on-call",
            vec![
                number(
                    "policy_id",
                    "Escalation policy",
                    "The policy to run, set up in Alerts → On-call.",
                    "1",
                )
                .required()
                .lookup("oncall_policies"),
            ],
            vec![],
        ),
        _ => return None,
    })
}
