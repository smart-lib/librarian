use serde::Serialize;

use crate::domain::{ContextPack, Project, PromptBlock, ProviderKind};

pub const TARGET_LIBRARIAN: &str = "librarian";
pub const TARGET_AGENTS: &str = "agents";
pub const TARGET_AGENTS_FILE: &str = "AGENTS.md";
pub const TARGET_CLAUDE_FILE: &str = "CLAUDE.md";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptProfileKind {
    Chat,
    Agent,
    ProviderInstruction,
}

pub const PROMPT_PROFILE_KINDS: &[PromptProfileKind] = &[
    PromptProfileKind::Chat,
    PromptProfileKind::Agent,
    PromptProfileKind::ProviderInstruction,
];

pub fn provider_instruction_target(provider: &ProviderKind) -> Option<&'static str> {
    match provider {
        ProviderKind::ClaudeCode => Some(TARGET_CLAUDE_FILE),
        ProviderKind::Codex | ProviderKind::OpenRouter => None,
    }
}

pub fn default_profile_target(kind: PromptProfileKind) -> &'static str {
    match kind {
        PromptProfileKind::Chat => TARGET_LIBRARIAN,
        PromptProfileKind::Agent => TARGET_AGENTS,
        PromptProfileKind::ProviderInstruction => TARGET_AGENTS_FILE,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PromptBlockVersion {
    pub target: Option<String>,
    pub version: String,
    pub enabled_blocks: usize,
    pub rendered_chars: usize,
}

pub fn render_prompt_blocks(blocks: &[PromptBlock]) -> String {
    blocks
        .iter()
        .filter(|block| block.enabled)
        .map(|block| block.content.trim())
        .filter(|content| !content.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn prompt_block_version(target: Option<&str>, blocks: &[PromptBlock]) -> PromptBlockVersion {
    let mut enabled = blocks
        .iter()
        .filter(|block| block.enabled)
        .collect::<Vec<_>>();
    enabled.sort_by(|left, right| {
        left.target
            .cmp(&right.target)
            .then(left.position.cmp(&right.position))
            .then(left.name.cmp(&right.name))
            .then(left.id.cmp(&right.id))
    });

    let mut hash = FNV_OFFSET_BASIS;
    hash = fnv_update(hash, b"prompt-blocks-v1");
    hash = fnv_update(hash, target.unwrap_or("*").as_bytes());
    for block in &enabled {
        hash = fnv_update(hash, block.target.as_bytes());
        hash = fnv_update(hash, block.name.as_bytes());
        hash = fnv_update(hash, block.position.to_string().as_bytes());
        hash = fnv_update(
            hash,
            if block.markdown {
                b"markdown"
            } else {
                b"plain"
            },
        );
        hash = fnv_update(hash, block.content.trim().as_bytes());
    }

    PromptBlockVersion {
        target: target.map(ToOwned::to_owned),
        version: format!("pbv1-{hash:016x}"),
        enabled_blocks: enabled.len(),
        rendered_chars: render_prompt_blocks(blocks).chars().count(),
    }
}

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv_update(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash ^= 0xff;
    hash.wrapping_mul(FNV_PRIME)
}

pub fn build_agent_prompt(
    project: &Project,
    goal: &str,
    context_pack: &ContextPack,
    instruction_blocks: &str,
) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a Librarian-managed coding agent running inside a project-scoped container.\n",
    );
    prompt.push_str("Work autonomously within the mounted project boundary and respect the provided policy context.\n\n");

    if !instruction_blocks.trim().is_empty() {
        prompt.push_str("## Instruction Blocks\n\n");
        prompt.push_str(instruction_blocks.trim());
        prompt.push_str("\n\n");
    }

    prompt.push_str("## Project\n\n");
    prompt.push_str(&format!("Name: {}\n", project.name));
    prompt.push_str(&format!("Path in container: `/workspace/project`\n"));
    prompt.push_str(&format!("Autonomy: {:?}\n", project.autonomy_mode));
    prompt.push_str(&format!(
        "Git policy: commits={}, pushes={}, protected_branches={:?}, branch_pattern={:?}\n\n",
        project.git_policy.allow_commit,
        project.git_policy.allow_push,
        project.git_policy.protected_branches,
        project.git_policy.require_branch_pattern
    ));

    prompt.push_str("## Goal\n\n");
    prompt.push_str(goal);
    prompt.push_str("\n\n");

    prompt.push_str("## Retrieved Memory Context\n\n");
    if context_pack.hits.is_empty() {
        prompt.push_str("No relevant prior memory was found.\n\n");
    } else {
        for (index, hit) in context_pack.hits.iter().enumerate() {
            prompt.push_str(&format!(
                "{}. kind={:?}; score={:.3}; observed={}; id={}\n",
                index + 1,
                hit.item.kind,
                hit.score,
                hit.item.observed_at.to_rfc3339(),
                hit.item.id
            ));
            prompt.push_str(hit.item.content.trim());
            prompt.push_str("\n\n");
        }
    }

    prompt.push_str("## Operating Notes\n\n");
    prompt.push_str("- Prefer existing project conventions over introducing new patterns.\n");
    prompt.push_str("- Keep changes scoped to the stated goal.\n");
    prompt.push_str("- Summarize what changed and what remains uncertain in the final response.\n");
    prompt.push_str("- If the context contains conflicting memories, prefer newer higher-confidence memories.\n");

    prompt
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;

    fn block(name: &str, content: &str, enabled: bool, position: i64) -> PromptBlock {
        PromptBlock {
            id: Uuid::new_v4(),
            target: "librarian".to_string(),
            name: name.to_string(),
            content: content.to_string(),
            enabled,
            position,
            markdown: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn prompt_block_version_ignores_disabled_blocks() {
        let blocks = vec![
            block("identity", "Be useful.", true, 1),
            block("disabled", "Ignore me.", false, 2),
        ];
        let without_disabled = vec![blocks[0].clone()];

        let version = prompt_block_version(Some("librarian"), &blocks);
        let other = prompt_block_version(Some("librarian"), &without_disabled);

        assert_eq!(version.version, other.version);
        assert_eq!(version.enabled_blocks, 1);
        assert_eq!(version.rendered_chars, "Be useful.".chars().count());
    }

    #[test]
    fn prompt_profile_targets_are_canonical() {
        assert_eq!(PROMPT_PROFILE_KINDS.len(), 3);
        assert_eq!(
            default_profile_target(PromptProfileKind::Chat),
            TARGET_LIBRARIAN
        );
        assert_eq!(
            default_profile_target(PromptProfileKind::Agent),
            TARGET_AGENTS
        );
        assert_eq!(
            provider_instruction_target(&ProviderKind::ClaudeCode),
            Some(TARGET_CLAUDE_FILE)
        );
        assert_eq!(provider_instruction_target(&ProviderKind::Codex), None);
    }
}
