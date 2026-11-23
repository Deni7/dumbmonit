//! Entrées du catalogue : Messageries grand public et autohébergées.

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Messageries grand public et autohébergées ------------------------
        "discord" => info(
            kind,
            "Discord",
            "Rich message in a Discord channel, through a webhook.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#discord",
            vec![],
            vec![webhook_url(
                "Channel settings → Integrations → Webhooks → Copy webhook URL.",
                "https://discord.com/api/webhooks/123456789/AbCdEf…",
            )],
        ),
        "slack" => info(
            kind,
            "Slack",
            "Message in a Slack channel, through an incoming webhook.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#slack",
            vec![interactive_field(
                "Acknowledge and Snooze become buttons that act right in Slack. Needs the app's \
                 signing secret below, and its Interactivity Request URL set to the address \
                 shown once the channel is saved (requires the instance's public URL).",
            )],
            vec![
                webhook_url(
                    "URL provided by \"Incoming Webhooks\" in your Slack app.",
                    "https://hooks.slack.com/services/T000/B000/XXXX",
                ),
                password(
                    crate::notify::interactive::slack::SIGNING_SECRET_KEY,
                    "Signing secret",
                    "Only for interactive buttons: Basic Information → App Credentials → \
                     Signing Secret. DumbMonit checks it on every button press.",
                    "8f742231b10e8888abcd99yyyzzz85a5",
                ),
            ],
        ),
        "telegram" => info(
            kind,
            "Telegram",
            "Message from a Telegram bot to you, in a group or in a channel.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#telegram",
            vec![
                text(
                    "chat_id",
                    "Chat ID",
                    "A number. Positive for a private chat with the bot; negative and \
                     starting with -100 for a group or a channel. The documentation explains \
                     how to find it in three cases.",
                    "-1001234567890",
                )
                .required(),
                text(
                    "message_thread_id",
                    "Group topic (optional)",
                    "Only for a group in \"Topics\" mode: the topic number, visible in the \
                     link of one of its messages (https://t.me/c/…/TOPIC/…).",
                    "11",
                ),
                url(
                    "api_base",
                    "API address",
                    "Change it only to go through a relay of the Telegram API.",
                    "https://api.telegram.org",
                )
                .with_default("https://api.telegram.org"),
                interactive_field(
                    "Acknowledge and Snooze act right in Telegram, without opening a page. \
                     DumbMonit registers itself as the bot's webhook when you save (the \
                     instance's public URL must be https): the bot then cannot be read with \
                     getUpdates by another tool.",
                ),
            ],
            vec![
                password(
                    "bot_token",
                    "Bot token",
                    "Given by @BotFather after /newbot, of the form 123456789:AAE…",
                    "123456789:AAExampleTokenFromBotFather",
                )
                .required(),
            ],
        ),
        "teams" => info(
            kind,
            "Microsoft Teams",
            "Adaptive card in a Teams channel, through a Workflows flow.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#microsoft-teams",
            vec![interactive_field(
                "Acknowledge, Snooze and Open become buttons on the card. Workflows webhooks \
                 cannot send a press back, so the buttons open DumbMonit's confirmation page.",
            )],
            vec![webhook_url(
                "HTTPS POST URL of the \"Post to a channel when a webhook request is \
                 received\" flow. The old Office 365 connectors no longer work.",
                "https://prod-00.westeurope.logic.azure.com:443/workflows/…",
            )],
        ),
        "matrix" => info(
            kind,
            "Matrix",
            "Message in a Matrix room, from an account dedicated to DumbMonit.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#matrix",
            vec![
                server_url("Homeserver of the DumbMonit account.", "https://matrix.org"),
                text(
                    "room_id",
                    "Room ID",
                    "The internal identifier, which starts with \"!\" — not the display name. \
                     The account must be a member of the room.",
                    "!aBcDeF:matrix.org",
                )
                .required(),
            ],
            vec![
                password(
                    "token",
                    "Access token",
                    "In Element: Settings → Help & About → Advanced → Access token.",
                    "syt_ZXp5bW9uaXQ_…",
                )
                .required(),
            ],
        ),
        "mattermost" => info(
            kind,
            "Mattermost",
            "Message in a Mattermost channel, through an incoming webhook.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#mattermost",
            vec![
                text(
                    "channel",
                    "Channel",
                    "To write somewhere other than the webhook's default channel.",
                    "alerts",
                ),
                text("username", "Display name", "Name under which the message appears.", "")
                    .with_default("DumbMonit"),
            ],
            vec![webhook_url(
                "Integrations → Incoming Webhooks → Add, then copy the URL.",
                "https://mattermost.example.org/hooks/abcdefghijklmnop",
            )],
        ),
        "rocketchat" => info(
            kind,
            "Rocket.Chat",
            "Message in a Rocket.Chat room, through an incoming webhook.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#rocketchat",
            vec![
                text(
                    "channel",
                    "Room",
                    "To write somewhere other than the room chosen when the webhook was created.",
                    "#alerts",
                ),
                text("alias", "Display name", "Name under which the message appears.", "")
                    .with_default("DumbMonit"),
            ],
            vec![webhook_url(
                "Administration → Integrations → Incoming webhook, then copy the webhook URL.",
                "https://rocket.example.org/hooks/abcdef/ghijkl",
            )],
        ),
        "googlechat" => info(
            kind,
            "Google Chat",
            "Card in a Google Chat space, through a webhook.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#google-chat",
            vec![],
            vec![webhook_url(
                "Space → Apps & integrations → Webhooks. The URL already contains the key \
                 and the token.",
                "https://chat.googleapis.com/v1/spaces/AAAA/messages?key=…&token=…",
            )],
        ),
        "zulip" => info(
            kind,
            "Zulip",
            "Message from a Zulip bot in a stream, under a dedicated topic.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#zulip",
            vec![
                server_url("Address of your Zulip organization.", "https://your-org.zulipchat.com"),
                text(
                    "email",
                    "Bot address",
                    "The email address assigned to the bot when it was created.",
                    "dumbmonit-bot@your-org.zulipchat.com",
                )
                .required(),
                text(
                    "stream",
                    "Stream",
                    "Name of the target stream; the bot must be subscribed to it.",
                    "monitoring",
                )
                .required(),
                text("topic", "Topic", "Topic under which messages are grouped.", "")
                    .with_default("DumbMonit"),
            ],
            vec![
                password(
                    "api_key",
                    "Bot API key",
                    "Personal settings → Bots, next to the bot.",
                    "aBcDeFgHiJkLmNoPqRsTuVwXyZ012345",
                )
                .required(),
            ],
        ),
        _ => return None,
    })
}
