use anyhow::Result;
use uuid::Uuid;

use crate::{config::Config, db::Database, secrets::SecretVault};

#[derive(Clone, Debug, serde::Serialize)]
pub struct GateResult {
    pub content: String,
    pub events: Vec<GateEvent>,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct GateEvent {
    pub kind: String,
    pub metadata: serde_json::Value,
}

pub async fn process_user_prompt(
    db: &Database,
    config: &Config,
    content: &str,
    source: &str,
) -> Result<GateResult> {
    let vault = SecretVault::new(config.clone());
    let mut output = content.to_string();
    let mut events = Vec::new();

    for token in detect_secret_tokens(content) {
        let name = format!("auto.detected.{}", Uuid::new_v4());
        let record = vault
            .store(db, &name, "unknown", "detected-token", &token)
            .await?;
        output = output.replace(&token, &format!("secret://{}", record.name));
        events.push(GateEvent {
            kind: "secret_captured".to_string(),
            metadata: serde_json::json!({
                "source": source,
                "secret_id": record.id,
                "secret_name": record.name,
                "replacement": format!("secret://{}", record.name),
            }),
        });
    }

    for event in &events {
        db.add_system_event(&event.kind, event.metadata.clone())
            .await?;
    }

    Ok(GateResult {
        content: output,
        events,
    })
}

pub async fn process_output(
    db: &Database,
    config: &Config,
    content: &str,
    source: &str,
) -> Result<GateResult> {
    let vault = SecretVault::new(config.clone());
    let mut output = content.to_string();
    let mut events = Vec::new();

    for record in db.list_secret_records().await? {
        let Ok(secret) = vault.decrypt_record(&record) else {
            continue;
        };
        if secret.len() < 8 {
            continue;
        }
        if output.contains(&secret) {
            output = output.replace(&secret, "[REDACTED_SECRET]");
            events.push(GateEvent {
                kind: "secret_redacted".to_string(),
                metadata: serde_json::json!({
                    "source": source,
                    "secret_id": record.id,
                    "secret_name": record.name,
                }),
            });
        }
    }

    for token in detect_secret_tokens(&output) {
        output = output.replace(&token, "[REDACTED_TOKEN]");
        events.push(GateEvent {
            kind: "secret_shaped_token_redacted".to_string(),
            metadata: serde_json::json!({
                "source": source,
                "token_prefix": token.chars().take(6).collect::<String>(),
            }),
        });
    }

    for event in &events {
        db.add_system_event(&event.kind, event.metadata.clone())
            .await?;
    }

    Ok(GateResult {
        content: output,
        events,
    })
}

fn detect_secret_tokens(content: &str) -> Vec<String> {
    content
        .split(|ch: char| {
            ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | '<' | '>' | ',' | ';')
        })
        .map(|part| {
            part.trim_matches(|ch: char| {
                matches!(ch, ')' | '(' | ']' | '[' | '}' | '{' | ':' | '.')
            })
        })
        .filter(|part| looks_like_secret(part))
        .map(ToOwned::to_owned)
        .collect()
}

fn looks_like_secret(value: &str) -> bool {
    if value.len() < 20 {
        return false;
    }
    if value.parse::<Uuid>().is_ok() {
        return false;
    }
    let lower = value.to_lowercase();
    lower.starts_with("sk-")
        || lower.starts_with("sk_")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("xoxb-")
        || lower.starts_with("xoxp-")
        || lower.starts_with("or-")
        || high_entropy(value)
}

fn high_entropy(value: &str) -> bool {
    if value.len() < 32 || value.len() > 256 {
        return false;
    }
    let alpha_num = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .count();
    let symbols = value
        .chars()
        .filter(|ch| matches!(ch, '_' | '-' | '.' | '='))
        .count();
    alpha_num + symbols == value.len() && symbols > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approval_uuid_is_not_treated_as_secret() {
        assert!(!looks_like_secret("7b8d0ee0-225e-438a-adbb-5a0a33999f08"));
        assert!(looks_like_secret(
            "sk-test_abcdefghijklmnopqrstuvwxyz1234567890"
        ));
    }
}
