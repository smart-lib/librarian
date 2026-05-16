use crate::domain::{ContextPack, Project};

pub fn build_agent_prompt(project: &Project, goal: &str, context_pack: &ContextPack) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "You are a Librarian-managed coding agent running inside a project-scoped container.\n",
    );
    prompt.push_str("Work autonomously within the mounted project boundary and respect the provided policy context.\n\n");

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
