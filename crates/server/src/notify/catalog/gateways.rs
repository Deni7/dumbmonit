//! Entrées du catalogue : Passerelles, domotique et messagerie.

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Passerelles, domotique et messagerie ---------------------------
        "apprise" => info(
            kind,
            "Apprise",
            "Gateway to dozens of services, through an Apprise instance.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#apprise",
            vec![
                server_url("Address of your Apprise instance.", "http://apprise.home:8000"),
                text(
                    "config_key",
                    "Configuration key",
                    "Recommended mode: key of a configuration stored in Apprise. Required if \
                     no destination URL is provided.",
                    "home",
                ),
                text("tag", "Tag", "To target only part of the configuration's destinations.", ""),
                select(
                    "format",
                    "Message format",
                    "Plain text suits every destination.",
                    &["text", "markdown"],
                ),
            ],
            vec![textarea(
                "urls",
                "Destination URLs",
                "Direct mode: one or more Apprise URLs separated by commas. Secret, because \
                 they often contain a password. Required if no configuration key is \
                 provided.",
                "mailto://user:password@example.org",
            )],
        ),
        "homeassistant" => info(
            kind,
            "Home Assistant",
            "Call to a Home Assistant service: a notification, or any other action.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#home-assistant",
            vec![
                server_url("Address of Home Assistant.", "http://homeassistant.local:8123"),
                text(
                    "service",
                    "Service to call",
                    "In \"domain.service\" form, for example notify.mobile_app_phone.",
                    "",
                )
                .with_default("persistent_notification.create"),
                text(
                    "target",
                    "Target",
                    "Passed as-is in \"target\", for services that expect one.",
                    "",
                ),
                textarea(
                    "data",
                    "Extra data",
                    "JSON object merged last into the call, for example an entity and a colour \
                     for light.turn_on.",
                    "{\"entity_id\": \"light.living_room\"}",
                )
                .object(),
            ],
            vec![
                password(
                    "token",
                    "Long-lived access token",
                    "Profile → Security → Long-lived access tokens → Create token.",
                    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9…",
                )
                .required(),
            ],
        ),
        "smtp" => info(
            kind,
            "Email (SMTP)",
            "Email sent by your own SMTP server, with no third-party service.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#email-smtp",
            vec![
                text("host", "SMTP server", "Host name or IP address.", "smtp.example.org")
                    .required()
                    .destination(true),
                select(
                    "security",
                    "Encryption",
                    "STARTTLS on 587, TLS from the start of the connection on 465, none \
                     (local relay only) on 25.",
                    &["starttls", "tls", "none"],
                )
                .destination(true),
                number("port", "Port", "Empty, derived from encryption: 587, 465 or 25.", "587")
                    .destination(true),
                text("from", "Sender", "Sending address.", "dumbmonit@example.org").required(),
                textarea("to", "Recipients", "One address per line.", "admin@example.org")
                    .required()
                    .list(),
                text(
                    "username",
                    "Username",
                    "For a server that requires authentication. Requires the password.",
                    "",
                ),
            ],
            vec![password(
                "password",
                "Password",
                "With Gmail, an app password, not the account password.",
                "",
            )],
        ),
        "signal" => info(
            kind,
            "Signal",
            "Signal message sent by your signal-cli-rest-api instance.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#signal",
            vec![
                server_url("Address of your signal-cli-rest-api.", "http://signal.home:8080"),
                text(
                    "number",
                    "Sender number",
                    "The number registered in signal-cli, in international format.",
                    "+33612345678",
                )
                .required(),
                textarea(
                    "recipients",
                    "Recipients",
                    "One number in international format or one group identifier per line.",
                    "+33698765432",
                )
                .required()
                .list(),
                text(
                    "username",
                    "Username",
                    "If the instance is protected by HTTP authentication. Requires the \
                     password.",
                    "",
                ),
            ],
            vec![password(
                "password",
                "Password",
                "Password of the instance's HTTP authentication.",
                "",
            )],
        ),
        "twilio" => info(
            kind,
            "SMS via Twilio",
            "SMS sent by Twilio, trimmed to the essentials to fit in one segment.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#sms-via-twilio",
            vec![
                text(
                    "account_sid",
                    "Account SID",
                    "Shown on the Twilio console home page.",
                    "ACxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                )
                .required(),
                text(
                    "from",
                    "Sender number",
                    "Your Twilio number. Required if no messaging service is provided.",
                    "+15005550006",
                ),
                text(
                    "messaging_service_sid",
                    "Messaging service",
                    "Required if no sender number is provided; Twilio then picks the number.",
                    "MGxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
                ),
                textarea(
                    "to",
                    "Recipients",
                    "One number in international format per line.",
                    "+33612345678",
                )
                .required()
                .list(),
            ],
            vec![
                password(
                    "token",
                    "Auth Token",
                    "Shown next to the Account SID in the console.",
                    "0123456789abcdef0123456789abcdef",
                )
                .required(),
            ],
        ),

        "whatsapp" => info(
            kind,
            "WhatsApp (Cloud API)",
            "WhatsApp message through the Meta Cloud API.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#whatsapp-cloud-api",
            vec![
                text(
                    "phone_number_id",
                    "Phone number ID",
                    "The numeric ID of the sending number, shown under WhatsApp → API Setup \
                     in the Meta developer console. Not the phone number itself.",
                    "109876543210987",
                )
                .required(),
                textarea(
                    "to",
                    "Recipients",
                    "One number in international format per line. Meta only allows free text \
                     within 24 h of the recipient's last message to your number.",
                    "+33612345678",
                )
                .required()
                .list(),
            ],
            vec![
                password(
                    "token",
                    "Access token",
                    "A permanent system-user token with the whatsapp_business_messaging \
                     permission.",
                    "EAAGm0PX4ZCpsBA…",
                )
                .required(),
            ],
        ),
        "callmebot" => info(
            kind,
            "WhatsApp (CallMeBot)",
            "WhatsApp message to yourself through the free CallMeBot gateway.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#whatsapp-callmebot",
            vec![
                text(
                    "phone",
                    "Phone number",
                    "Your number in international format, the one that activated CallMeBot.",
                    "+34123456789",
                )
                .required(),
            ],
            vec![
                password(
                    "api_key",
                    "API key",
                    "Sent back by the CallMeBot contact once you have written to it.",
                    "123456",
                )
                .required(),
            ],
        ),
        "threema" => info(
            kind,
            "Threema Gateway",
            "Threema message from your Gateway ID, in basic mode.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#threema-gateway",
            vec![
                text(
                    "from",
                    "Gateway ID",
                    "Eight characters starting with \"*\", from gateway.threema.ch.",
                    "*MYGWAY1",
                )
                .required(),
                textarea(
                    "to",
                    "Recipients",
                    "One Threema ID (eight characters) per line.",
                    "ECHOECHO",
                )
                .required()
                .list(),
            ],
            vec![
                password(
                    "secret",
                    "API secret",
                    "Shown next to the Gateway ID in the Threema Gateway portal.",
                    "AbCdEfGhIjKlMnOp",
                )
                .required(),
            ],
        ),
        "webex" => info(
            kind,
            "Webex",
            "Message from a Webex bot in a space.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#webex",
            vec![
                text(
                    "room_id",
                    "Room ID",
                    "The space identifier (not its name). The bot must be a member of the \
                     space.",
                    "Y2lzY29zcGFyazovL3VzL1JPT00vYWJj",
                )
                .required(),
            ],
            vec![
                password(
                    "token",
                    "Bot token",
                    "Created on developer.webex.com → My Apps → Create a Bot.",
                    "NzE2ZjQ5Y2ItZXhhbXBsZQ",
                )
                .required(),
            ],
        ),
        "vonage" => info(
            kind,
            "SMS via Vonage",
            "SMS sent by Vonage, trimmed to the essentials.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#sms-via-vonage",
            vec![
                text(
                    "from",
                    "Sender",
                    "A number, or an alphanumeric sender name where your country allows it.",
                    "DumbMonit",
                )
                .required(),
                textarea(
                    "to",
                    "Recipients",
                    "One number in international format per line.",
                    "+33612345678",
                )
                .required()
                .list(),
            ],
            vec![
                password("api_key", "API key", "Shown on the Vonage API dashboard.", "ab12cd34")
                    .required(),
                password(
                    "api_secret",
                    "API secret",
                    "Next to the API key on the Vonage dashboard.",
                    "AbCdEfGhIjKlMnOp",
                )
                .required(),
            ],
        ),
        "telnyx" => info(
            kind,
            "SMS via Telnyx",
            "SMS sent by Telnyx, trimmed to the essentials.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#sms-via-telnyx",
            vec![
                text(
                    "from",
                    "Sender number",
                    "A Telnyx number with SMS enabled, in E.164 format.",
                    "+15005550006",
                )
                .required(),
                textarea(
                    "to",
                    "Recipients",
                    "One number in E.164 format (with the +) per line.",
                    "+33612345678",
                )
                .required()
                .list(),
            ],
            vec![
                password(
                    "api_key",
                    "API key (v2)",
                    "Mission Control Portal → Auth → API Keys.",
                    "KEY0123456789ABCDEF",
                )
                .required(),
            ],
        ),
        "aspsms" => info(
            kind,
            "SMS via ASPSMS",
            "SMS sent by ASPSMS, trimmed to the essentials.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#sms-via-aspsms",
            vec![
                text("userkey", "Userkey", "Your ASPSMS account userkey.", "ABCDE12345").required(),
                text(
                    "originator",
                    "Originator",
                    "Sender name (11 characters at most) or number, as allowed by your account.",
                    "DumbMonit",
                )
                .required(),
                textarea(
                    "to",
                    "Recipients",
                    "One number in international format per line.",
                    "+41791234567",
                )
                .required()
                .list(),
            ],
            vec![
                password(
                    "password",
                    "API password",
                    "The API password set in the ASPSMS interface (not the login password).",
                    "AbCdEfGhIjKl",
                )
                .required(),
            ],
        ),
        "grafana_oncall" => info(
            kind,
            "Grafana OnCall",
            "Alert group in Grafana OnCall, resolved when the alert is.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#grafana-oncall",
            vec![],
            vec![webhook_url(
                "Integrations → New integration → Webhook, then copy the HTTP endpoint.",
                "https://oncall.example.org/integrations/v1/webhook/AbCdEf123456/",
            )],
        ),
        "signl4" => info(
            kind,
            "SIGNL4",
            "Mobile alert to your SIGNL4 team, closed when the alert resolves.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#signl4",
            vec![],
            vec![webhook_url(
                "The team webhook: https://connect.signl4.com/webhook/ followed by your team \
                 secret, shown under Teams → Team settings.",
                "https://connect.signl4.com/webhook/abc123def4",
            )],
        ),
        _ => return None,
    })
}
