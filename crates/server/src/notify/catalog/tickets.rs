//! Entrées du catalogue : Création automatique de tickets.

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Tickets automatiques ---------------------------------------------
        "jira" => info(
            kind,
            "Jira",
            "Ticket opened by the alert, commented on escalation, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#jira",
            vec![
                url(
                    "server_url",
                    "Jira address",
                    "Jira Cloud site or Data Center server, without /rest.",
                    "https://example.atlassian.net",
                )
                .required(),
                text(
                    "email",
                    "Account e-mail",
                    "Jira Cloud: the account that owns the API token. Leave empty on Data Center, where the token is a personal access token.",
                    "ops@example.org",
                ),
                text("project", "Project key", "The key shown before the issue number.", "OPS")
                    .required()
                    .destination(true),
                text("issue_type", "Issue type", "Defaults to Task.", "Task").with_default("Task"),
                textarea("labels", "Labels", "One label per line.", "dumbmonit").list(),
                select(
                    "priority",
                    "Priority field",
                    "\"map\" sets Highest, Medium or Low from the severity; \"none\" leaves it \
                     to the project (some projects have no priority field).",
                    &["map", "none"],
                ),
                text(
                    "close_transition",
                    "Transition on resolution",
                    "Name of the workflow transition (or of its target status) that closes the \
                     issue. Defaults to Done.",
                    "Done",
                )
                .with_default("Done"),
                text(
                    "reopen_transition",
                    "Transition to reopen",
                    "Used when the alert comes back within the reopen delay. Defaults to Reopen.",
                    "Reopen",
                )
                .with_default("Reopen"),
                number(
                    "reopen_hours",
                    "Reopen if the alert returns within (hours)",
                    "0 or empty: a returning alert always opens a new ticket.",
                    "24",
                ),
            ],
            vec![
                password(
                    "api_token",
                    "API token",
                    "Cloud: id.atlassian.com → Security → API tokens. Data Center: Profile → \
                     Personal Access Tokens.",
                    "ATATT3xFfGF0…",
                )
                .required(),
            ],
        ),
        "glpi" => info(
            kind,
            "GLPI",
            "Incident opened by the alert, followed up on escalation, solved by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#glpi",
            vec![
                url(
                    "server_url",
                    "GLPI address",
                    "The address of GLPI; /apirest.php is added. The REST API must be enabled \
                     (Setup → General → API).",
                    "https://glpi.example.org",
                )
                .required(),
                number("entity_id", "Entity id", "Empty: the profile's default entity.", "0"),
                number("category_id", "Category id", "ITIL category of the ticket.", "12"),
                select(
                    "close_status",
                    "Status on resolution",
                    "Solved lets the requester validate; Closed ends the ticket.",
                    &["solved", "closed"],
                ),
                number(
                    "reopen_hours",
                    "Reopen if the alert returns within (hours)",
                    "0 or empty: a returning alert always opens a new ticket.",
                    "24",
                ),
            ],
            vec![
                password(
                    "user_token",
                    "User API token",
                    "My settings → Remote access keys → API token.",
                    "f7g3csp8mgatg5ebc5elnazakw20i9fyev1qopya7",
                )
                .required(),
                password(
                    "app_token",
                    "Application token",
                    "Setup → General → API → API clients. Empty if the client does not require one.",
                    "nHqcAHhq0XQYPrCzGXZNfMn1NGgUVUMdnPqZ9o3m",
                ),
            ],
        ),
        "zammad" => info(
            kind,
            "Zammad",
            "Ticket opened by the alert, noted on escalation, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#zammad",
            vec![
                url(
                    "server_url",
                    "Zammad address",
                    "The address of your Zammad, without /api.",
                    "https://support.example.org",
                )
                .required(),
                text("group", "Group", "The group that receives the tickets.", "Users").required(),
                text(
                    "customer",
                    "Customer",
                    "E-mail or login of the customer the tickets are filed for.",
                    "ops@example.org",
                )
                .required(),
                text("close_state", "State on resolution", "Defaults to closed.", "closed")
                    .with_default("closed"),
                text(
                    "reopen_state",
                    "State to reopen",
                    "Used when the alert comes back within the reopen delay. Defaults to open.",
                    "open",
                )
                .with_default("open"),
                number(
                    "reopen_hours",
                    "Reopen if the alert returns within (hours)",
                    "0 or empty: a returning alert always opens a new ticket.",
                    "24",
                ),
            ],
            vec![
                password(
                    "token",
                    "Access token",
                    "Profile → Token Access. It needs the ticket.agent permission.",
                    "s3cr3t-zammad-t0ken",
                )
                .required(),
            ],
        ),
        "github_issues" => info(
            kind,
            "GitHub Issues",
            "Issue opened by the alert, commented on escalation, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#github-issues",
            vec![
                text("repository", "Repository", "As owner/name.", "my-org/homelab")
                    .required()
                    .destination(true),
                url(
                    "api_url",
                    "API address",
                    "Only for GitHub Enterprise Server, for example https://ghe.example.org/api/v3.",
                    "https://api.github.com",
                ),
                textarea(
                    "labels",
                    "Labels",
                    "One label per line. They must already exist.",
                    "monitoring",
                )
                .list(),
                number(
                    "reopen_hours",
                    "Reopen if the alert returns within (hours)",
                    "0 or empty: a returning alert always opens a new issue.",
                    "24",
                ),
            ],
            vec![
                password(
                    "token",
                    "Fine-grained token",
                    "Settings → Developer settings → Fine-grained tokens, with Issues: read and \
                     write on the repository.",
                    "github_pat_11AAAA…",
                )
                .required(),
            ],
        ),
        "gitlab_issues" => info(
            kind,
            "GitLab Issues",
            "Issue opened by the alert, commented on escalation, closed by its resolution.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#gitlab-issues",
            vec![
                url(
                    "server_url",
                    "GitLab address",
                    "gitlab.com or your instance, without /api.",
                    "https://gitlab.example.org",
                )
                .required(),
                text(
                    "project",
                    "Project",
                    "Numeric id or path such as group/project.",
                    "ops/homelab",
                )
                .required()
                .destination(true),
                textarea("labels", "Labels", "One label per line.", "monitoring").list(),
                number(
                    "reopen_hours",
                    "Reopen if the alert returns within (hours)",
                    "0 or empty: a returning alert always opens a new issue.",
                    "24",
                ),
            ],
            vec![
                password(
                    "token",
                    "Access token",
                    "A project or personal access token with the api scope.",
                    "glpat-xxxxxxxxxxxxxxxxxxxx",
                )
                .required(),
            ],
        ),
        _ => return None,
    })
}
