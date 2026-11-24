//! Règles d'alerte : lecture et activation.

use super::*;

pub(super) async fn list_rules(state: &AppState) -> ToolResult {
    let Json(rules) = alerts::list_rules(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let text = if rules.is_empty() {
        "No alert rule.".to_string()
    } else {
        let mut lines = vec![format!("{}:", plural(rules.len(), "rule", "rules"))];
        for r in &rules {
            let mut line = format!(
                "- `{}` (#{}) {} — {}, {}",
                r.uid,
                r.id,
                r.name,
                r.kind.as_str(),
                severity_word(r.severity)
            );
            if r.kind == crate::alerting::model::RuleKind::Threshold {
                line.push_str(&format!(
                    ", {} {}{}",
                    r.operator.as_str(),
                    trim_float(r.threshold),
                    r.unit
                ));
            }
            if r.for_secs > 0 {
                line.push_str(&format!(" for {} s", r.for_secs));
            }
            line.push_str(if r.enabled { "; enabled" } else { "; disabled" });
            if r.builtin {
                line.push_str("; built in");
            }
            lines.push(line);
        }
        lines.join("\n")
    };
    let structured: Vec<Value> = rules
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "uid": r.uid,
                "name": r.name,
                "description": r.description,
                "kind": r.kind.as_str(),
                "severity": severity_word(r.severity),
                "query": r.query,
                "operator": r.operator.as_str(),
                "threshold": r.threshold,
                "unit": r.unit,
                "for_secs": r.for_secs,
                "enabled": r.enabled,
                "builtin": r.builtin,
            })
        })
        .collect();
    Ok(ToolOutput::new(text, json!({ "rules": structured })))
}

pub(super) async fn set_rule_enabled(state: &AppState, args: &Args) -> ToolResult {
    let enabled = args.required_bool("enabled")?;
    let Json(rules) = alerts::list_rules(
        crate::auth::teams::TeamAccess(crate::auth::teams::Access::All),
        State(state.clone()),
    )
    .await?;
    let rule = if let Some(id) = args.i64("id") {
        rules.iter().find(|r| r.id == id)
    } else if let Some(uid) = args.str("uid") {
        rules.iter().find(|r| r.uid.eq_ignore_ascii_case(uid))
    } else {
        return Err(failed("Name the rule: \"uid\" or \"id\" (see list_rules)."));
    };
    let Some(rule) = rule else {
        return Err(failed(format!(
            "No such rule. Known rules: {}.",
            rules.iter().map(|r| r.uid.as_str()).collect::<Vec<_>>().join(", ")
        )));
    };
    let Json(updated) = alerts::set_rule_enabled(
        State(state.clone()),
        Path(rule.id),
        Json(EnablePayload { enabled }),
    )
    .await?;
    Ok(ToolOutput::new(
        format!(
            "Rule `{}` ({}) is now {}.",
            updated.uid,
            updated.name,
            if updated.enabled { "enabled" } else { "disabled" }
        ),
        json!({ "id": updated.id, "uid": updated.uid, "name": updated.name, "enabled": updated.enabled }),
    ))
}
