use anyhow::Result;

use crate::domain::{AutonomyMode, MountMode, Project};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobCreationSource {
    ExplicitUserAction,
    AutomaticSchedule,
}

pub fn ensure_agent_job_allowed(
    project: &Project,
    mount_mode: MountMode,
    source: JobCreationSource,
) -> Result<()> {
    if !matches!(mount_mode, MountMode::ReadWrite) {
        return Ok(());
    }

    match project.autonomy_mode {
        AutonomyMode::ProjectFull => Ok(()),
        AutonomyMode::ProjectGuarded if matches!(source, JobCreationSource::ExplicitUserAction) => {
            Ok(())
        }
        AutonomyMode::ProjectGuarded => {
            anyhow::bail!(
                "Project `{}` is guarded; automatic read-write agent jobs are blocked. Queue a read-only review or launch the write task explicitly.",
                project.name
            )
        }
        AutonomyMode::ReadOnlyReview => {
            anyhow::bail!(
                "Project `{}` is read-only review; read-write agent jobs are blocked by project policy.",
                project.name
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use uuid::Uuid;

    use super::*;
    use crate::domain::GitPolicy;

    fn project(mode: AutonomyMode) -> Project {
        Project {
            id: Uuid::new_v4(),
            name: "PolicySmoke".to_string(),
            library_path: None,
            path: ".".into(),
            autonomy_mode: mode,
            git_policy: GitPolicy::default(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn guarded_project_blocks_automatic_write_jobs() {
        let project = project(AutonomyMode::ProjectGuarded);
        assert!(ensure_agent_job_allowed(
            &project,
            MountMode::ReadWrite,
            JobCreationSource::AutomaticSchedule
        )
        .is_err());
        assert!(ensure_agent_job_allowed(
            &project,
            MountMode::ReadWrite,
            JobCreationSource::ExplicitUserAction
        )
        .is_ok());
    }

    #[test]
    fn read_only_review_blocks_all_write_jobs() {
        let project = project(AutonomyMode::ReadOnlyReview);
        assert!(ensure_agent_job_allowed(
            &project,
            MountMode::ReadWrite,
            JobCreationSource::ExplicitUserAction
        )
        .is_err());
        assert!(ensure_agent_job_allowed(
            &project,
            MountMode::ReadOnly,
            JobCreationSource::AutomaticSchedule
        )
        .is_ok());
    }
}
