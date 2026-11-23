//! Entrées du catalogue : Notifications directes (push, SMS, courriel).

use super::*;

pub(super) fn describe(kind: &'static str) -> Option<KindInfo> {
    Some(match kind {
        // --- Notifications directes ----------------------------------------
        "ntfy" => info(
            kind,
            "ntfy",
            "Notification on an ntfy topic, on the public instance or your own.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#ntfy",
            vec![
                text(
                    "topic",
                    "Topic",
                    "On ntfy.sh, anyone who knows this name can read the topic: pick a long, \
                     random one.",
                    "dumbmonit-k7x2p9qv",
                )
                .required(),
                url("server_url", "Server address", "Your own instance, if you host one.", "")
                    .with_default("https://ntfy.sh"),
            ],
            vec![password(
                "token",
                "Access token",
                "Only if the server requires authentication.",
                "tk_abcdefghijklmnopqrstuvwxyz",
            )],
        ),
        "gotify" => info(
            kind,
            "Gotify",
            "Notification on your self-hosted Gotify server.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#gotify",
            vec![server_url("Address of your Gotify server.", "https://gotify.home")],
            vec![
                password(
                    "token",
                    "Application token",
                    "Apps → Create Application, then copy the token shown.",
                    "AbCdEfGhIjKlMnO",
                )
                .required(),
            ],
        ),
        "pushover" => info(
            kind,
            "Pushover",
            "Pushover notification on your devices, with priority following severity.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#pushover",
            vec![
                number(
                    "priority",
                    "Forced priority",
                    "From -2 to 2. Empty, the priority follows severity: -1, 0 or 1. Priority 2 \
                     repeats until acknowledged.",
                    "0",
                ),
                text("sound", "Sound", "Name of a Pushover sound, for example \"siren\".", ""),
                number(
                    "retry",
                    "Retry (seconds)",
                    "Interval between two repeats of a priority 2; from 30 to 10,800.",
                    "60",
                )
                .with_default("60"),
                number(
                    "expire",
                    "Expiration (seconds)",
                    "How long a priority 2 keeps being repeated; from 30 to 10,800.",
                    "3600",
                )
                .with_default("3600"),
            ],
            vec![
                password(
                    "token",
                    "Application token",
                    "Create an Application/API Token, on pushover.net.",
                    "azGDORePK8gMaC0QOYAMyEEuzJnyUi",
                )
                .required(),
                password(
                    "user_key",
                    "User key",
                    "Shown on the pushover.net home page.",
                    "uQiRzpo4DXghDmr9QzzfQu27cmVRsG",
                )
                .required(),
            ],
        ),
        "pushbullet" => info(
            kind,
            "Pushbullet",
            "Pushbullet notification on all your devices, or on a single one.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#pushbullet",
            vec![text(
                "device_iden",
                "Device identifier",
                "Empty, all your devices are notified.",
                "ujpah72o0sjAoRtnM0jc",
            )],
            vec![
                password(
                    "token",
                    "Access token",
                    "Settings → Account → Create Access Token.",
                    "o.AbCdEfGhIjKlMnOpQrStUvWxYz",
                )
                .required(),
            ],
        ),
        "bark" => info(
            kind,
            "Bark (iOS)",
            "Notification on iPhone through Bark, with interruption level following severity.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#bark-ios",
            vec![
                url("server_url", "Server address", "Your own Bark instance, if you host one.", "")
                    .with_default("https://api.day.app"),
                text("group", "Group", "Group under which iOS files the notifications.", "")
                    .with_default("DumbMonit"),
                select(
                    "level",
                    "Forced interruption level",
                    "Empty, the level follows severity: passive, active or timeSensitive. \
                     \"critical\" breaks through Do Not Disturb.",
                    &["", "passive", "active", "timeSensitive", "critical"],
                ),
                text("sound", "Sound", "Name of a Bark sound, for example \"alarm\".", ""),
            ],
            vec![
                password(
                    "token",
                    "Device key",
                    "The string of characters after the domain name in the URL shown by the \
                     app.",
                    "AbCdEfGhIjKlMnOpQrStUv",
                )
                .required(),
            ],
        ),
        // Pas de secret : les destinataires sont les appareils abonnés depuis
        // Réglages → Push notifications, et la clé VAPID est celle de l'instance.
        "webpush" => info(
            kind,
            "Web Push (browser, PWA)",
            "Native notifications on the phones and computers where DumbMonit was enabled, \
             even with the tab closed. No app, no third-party account.",
            "https://dumbmonit.selfhostr.io/docs/notifications/#web-push",
            vec![
                text(
                    "users",
                    "Accounts",
                    "Accounts whose devices get this channel, one per line. Empty: every \
                     subscribed device of every account.",
                    "admin",
                )
                .list(),
                select(
                    "urgency",
                    "Urgency",
                    "Delivery priority asked of the push service. \"low\" may wait until the \
                     phone is charging or on Wi-Fi.",
                    crate::notify::webpush::URGENCIES,
                ),
                number(
                    "ttl",
                    "Keep for (seconds)",
                    "How long the push service keeps the message while the device is off.",
                    "86400",
                )
                .with_default("86400"),
                text(
                    "contact",
                    "Contact",
                    "mailto: or https: address given to push services in case of abuse. Empty: \
                     DUMBMONIT_PUBLIC_URL when it is HTTPS, otherwise the project page.",
                    "mailto:ops@example.com",
                ),
            ],
            vec![],
        ),
        _ => return None,
    })
}
