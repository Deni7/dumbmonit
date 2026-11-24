//! Canaux de notification : lecture et envoi d'un essai.

use super::*;

pub(super) async fn list_channels(state: &AppState) -> ToolResult {
    let Json(channels) = channels::list(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let text = if channels.is_empty() {
        "No notification channel: alerts are not sent anywhere yet.".to_string()
    } else {
        let mut lines = vec![format!(
            "{}:",
            plural(channels.len(), "notification channel", "notification channels")
        )];
        for c in &channels {
            let mut line = format!(
                "- **{}** (#{}, {}) — {}",
                c.name,
                c.id,
                c.kind,
                if c.enabled { "enabled" } else { "disabled" }
            );
            if let Some(at) = &c.last_sent_at {
                line.push_str(&format!("; last sent {at}"));
            }
            if let Some(error) = &c.last_error {
                line.push_str(&format!("; last error: {error}"));
            }
            lines.push(line);
        }
        lines.join("\n")
    };
    // Les réglages restent au serveur : certains types y rangent des adresses
    // ou des identifiants que l'assistant n'a pas à connaître.
    let structured: Vec<Value> = channels
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "kind": c.kind,
                "enabled": c.enabled,
                "has_secret": c.has_secret,
                "last_error": c.last_error,
                "last_sent_at": c.last_sent_at,
            })
        })
        .collect();
    Ok(ToolOutput::new(text, json!({ "channels": structured })))
}

pub(super) async fn test_channel(state: &AppState, args: &Args) -> ToolResult {
    let reference = args
        .str("channel")
        .map(str::to_string)
        .or_else(|| args.i64("channel").map(|id| id.to_string()))
        .or_else(|| args.i64("id").map(|id| id.to_string()))
        .ok_or_else(|| failed("Name the channel: \"channel\" (id or name, see list_channels)."))?;
    let Json(channels) = channels::list(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let wanted = reference.to_lowercase();
    let by_id = reference.parse::<i64>().ok().and_then(|id| channels.iter().find(|c| c.id == id));
    let exact = channels.iter().find(|c| c.name.to_lowercase() == wanted);
    let partial: Vec<_> =
        channels.iter().filter(|c| c.name.to_lowercase().contains(&wanted)).collect();
    let channel = match (by_id.or(exact), partial.as_slice()) {
        (Some(c), _) => c,
        (None, [c]) => *c,
        (None, []) => {
            return Err(failed(format!(
                "No channel matches \"{reference}\". Channels: {}.",
                if channels.is_empty() {
                    "none yet".to_string()
                } else {
                    channels
                        .iter()
                        .map(|c| format!("{} (#{})", c.name, c.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )));
        }
        (None, many) => {
            return Err(failed(format!(
                "\"{reference}\" is ambiguous: {}. Use the id.",
                many.iter()
                    .map(|c| format!("{} (#{})", c.name, c.id))
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    };
    let Json(report) = channels::test(State(state.clone()), Path(channel.id)).await?;
    let text = if report.ok {
        format!("Test message sent through {} ({}).", channel.name, channel.kind)
    } else {
        format!("The test through {} ({}) failed: {}", channel.name, channel.kind, report.message)
    };
    Ok(ToolOutput::new(
        text,
        json!({ "channel_id": channel.id, "channel": channel.name, "ok": report.ok, "message": report.message }),
    ))
}
