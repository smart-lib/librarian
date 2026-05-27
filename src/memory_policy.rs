use crate::domain::{MemoryItem, MemoryKind};

pub fn is_raw_transcript_memory_item(item: &MemoryItem) -> bool {
    item.metadata
        .get("durability")
        .and_then(serde_json::Value::as_str)
        == Some("transcript")
        || item
            .metadata
            .get("memory_role")
            .and_then(serde_json::Value::as_str)
            == Some("raw_chat_turn")
}

pub fn is_visible_durable_memory_item(item: &MemoryItem) -> bool {
    if is_raw_transcript_memory_item(item) {
        return false;
    }
    if matches!(
        item.kind,
        MemoryKind::UserMessage | MemoryKind::AssistantMessage
    ) {
        if item
            .metadata
            .get("memory_role")
            .and_then(serde_json::Value::as_str)
            == Some("durable_memory")
        {
            return true;
        }
        if item.source.as_deref() == Some("admin:librarian-chat")
            || item.topic.as_deref() == Some("librarian-chat")
        {
            return false;
        }
    }
    true
}

pub fn durable_memory_type(kind: &MemoryKind) -> &'static str {
    match kind {
        MemoryKind::Instruction => "instruction",
        MemoryKind::Decision => "decision",
        MemoryKind::Status => "status",
        MemoryKind::Summary => "summary",
        MemoryKind::RunObservation => "run_observation",
        MemoryKind::Fact => "fact",
        MemoryKind::UserMessage => "user_message",
        MemoryKind::AssistantMessage => "assistant_message",
    }
}

pub fn durable_memory_priority(kind: &MemoryKind) -> f64 {
    match kind {
        MemoryKind::Instruction => 1.0,
        MemoryKind::Decision => 0.92,
        MemoryKind::Status => 0.78,
        MemoryKind::Summary => 0.72,
        MemoryKind::Fact => 0.64,
        MemoryKind::RunObservation => 0.58,
        MemoryKind::UserMessage | MemoryKind::AssistantMessage => 0.35,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_memory_metadata_classifies_kinds() {
        assert_eq!(durable_memory_type(&MemoryKind::Instruction), "instruction");
        assert!(
            durable_memory_priority(&MemoryKind::Instruction)
                > durable_memory_priority(&MemoryKind::Fact)
        );
    }
}
