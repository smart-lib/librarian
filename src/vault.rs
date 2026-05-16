use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;

use crate::{
    config::Config,
    domain::{Job, Project},
};

#[derive(Clone)]
pub struct Vault {
    root: PathBuf,
}

impl Vault {
    pub fn new(config: &Config) -> Self {
        Self {
            root: config.vault_path.clone(),
        }
    }

    pub fn write_project_note(&self, project: &Project) -> Result<PathBuf> {
        let path = self
            .root
            .join("projects")
            .join(format!("{}.md", project_slug(project)));
        if path.exists() {
            return Ok(path);
        }

        let content = format!(
            r#"---
kind: project
id: {}
name: {}
created_at: {}
---

# {}

Path: `{}`

## Notes

"#,
            project.id,
            yaml_string(&project.name)?,
            project.created_at.to_rfc3339(),
            project.name,
            project.path.display()
        );
        write_text(path, content)
    }

    pub fn write_run_summary(
        &self,
        project: &Project,
        job: &Job,
        status: &str,
        outcome: &str,
    ) -> Result<PathBuf> {
        let stamp = Utc::now().format("%Y%m%d-%H%M%S");
        let path = self
            .root
            .join("runs")
            .join(format!("{}-{}.md", stamp, job.id));
        let content = format!(
            r#"---
kind: run
job_id: {}
project_id: {}
project: {}
provider: {:?}
status: {}
created_at: {}
---

# Run {}

Project: [[projects/{}|{}]]

## Goal

{}

## Outcome

{}
"#,
            job.id,
            project.id,
            yaml_string(&project.name)?,
            job.provider,
            status,
            Utc::now().to_rfc3339(),
            job.id,
            project_slug(project),
            project.name,
            job.goal,
            outcome
        );
        write_text(path, content)
    }
}

fn write_text(path: PathBuf, content: String) -> Result<PathBuf> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create vault directory {}", parent.display()))?;
    }
    fs::write(&path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(path)
}

fn slug(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.' | ' ') {
            out.push('-');
        }
    }
    while out.contains("--") {
        out = out.replace("--", "-");
    }
    out.trim_matches('-').to_string()
}

fn project_slug(project: &Project) -> String {
    let slug = slug(&project.name);
    if slug.is_empty() {
        project.id.to_string()
    } else {
        slug
    }
}

fn yaml_string(value: &str) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}
