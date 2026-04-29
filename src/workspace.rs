use std::path::{Path, PathBuf};

use tokio::{
    process::Command,
    time::{Duration, timeout},
};
use tracing::{info, warn};

use crate::{
    config::HooksConfig,
    error::{LunaError, Result},
    model::WorkspaceAssignment,
};

const HOOK_OUTPUT_LIMIT: usize = 4 * 1024;

#[derive(Clone, Debug)]
pub struct WorkspaceManager {
    root: PathBuf,
    hooks: HooksConfig,
    source_repo_hint: Option<PathBuf>,
}

impl WorkspaceManager {
    pub fn new(root: PathBuf, hooks: HooksConfig, source_repo_hint: Option<PathBuf>) -> Self {
        Self {
            root,
            hooks,
            source_repo_hint,
        }
    }

    pub async fn prepare(&self, issue_identifier: &str) -> Result<WorkspaceAssignment> {
        tokio::fs::create_dir_all(&self.root).await?;
        let workspace_key = sanitize_workspace_key(issue_identifier);
        let path = self.root.join(&workspace_key);
        ensure_within_root(&self.root, &path)?;

        let created_now = self.ensure_workspace_checkout(&path).await?;

        let workspace = WorkspaceAssignment {
            path,
            workspace_key,
            created_now,
        };

        if workspace.created_now {
            self.run_optional_hook(
                "after_create",
                self.hooks.after_create.as_deref(),
                &workspace.path,
                true,
            )
            .await?;
        }

        Ok(workspace)
    }

    async fn ensure_workspace_checkout(&self, path: &Path) -> Result<bool> {
        if path.exists() {
            if !path.is_dir() {
                return Err(LunaError::Workspace(format!(
                    "workspace path exists but is not a directory: {}",
                    path.display()
                )));
            }
            if path.join(".git").exists() {
                return Ok(false);
            }
            if !is_directory_empty(path).await? {
                return Err(LunaError::Workspace(format!(
                    "workspace exists but is not a git worktree and is not empty: {}",
                    path.display()
                )));
            }
            tokio::fs::remove_dir(path).await?;
        }

        let Some(source_repo_root) = self.resolve_source_repo_root().await? else {
            tokio::fs::create_dir_all(path).await?;
            return Ok(true);
        };

        info!(
            workspace = %path.display(),
            source_repo = %source_repo_root.display(),
            "creating git worktree for workspace"
        );

        let output = Command::new("git")
            .arg("-C")
            .arg(&source_repo_root)
            .args(["worktree", "add", "--detach"])
            .arg(path)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
            let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
            return Err(LunaError::Workspace(format!(
                "failed to create git worktree for workspace: status={}, stdout={stdout:?}, stderr={stderr:?}",
                output.status
            )));
        }

        Ok(true)
    }

    async fn resolve_source_repo_root(&self) -> Result<Option<PathBuf>> {
        let Some(source_repo_hint) = &self.source_repo_hint else {
            return Ok(None);
        };

        let output = Command::new("git")
            .arg("-C")
            .arg(source_repo_hint)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            Ok(None)
        } else {
            Ok(Some(PathBuf::from(path)))
        }
    }

    pub async fn before_run(&self, workspace: &WorkspaceAssignment) -> Result<()> {
        self.run_optional_hook(
            "before_run",
            self.hooks.before_run.as_deref(),
            &workspace.path,
            true,
        )
        .await
    }

    pub async fn after_run_best_effort(&self, workspace: &WorkspaceAssignment) {
        if let Err(err) = self
            .run_optional_hook(
                "after_run",
                self.hooks.after_run.as_deref(),
                &workspace.path,
                false,
            )
            .await
        {
            warn!(workspace = %workspace.path.display(), error = %err, "after_run hook failed");
        }
    }

    pub async fn cleanup(&self, issue_identifier: &str) -> Result<()> {
        let workspace_key = sanitize_workspace_key(issue_identifier);
        let path = self.root.join(workspace_key);
        ensure_within_root(&self.root, &path)?;

        if !path.exists() {
            return Ok(());
        }

        if let Err(err) = self
            .run_optional_hook(
                "before_remove",
                self.hooks.before_remove.as_deref(),
                &path,
                false,
            )
            .await
        {
            warn!(workspace = %path.display(), error = %err, "before_remove hook failed");
        }

        if path.join(".git").exists() {
            if let Some(source_repo_root) = self.resolve_source_repo_root().await? {
                let output = Command::new("git")
                    .arg("-C")
                    .arg(&source_repo_root)
                    .args(["worktree", "remove", "--force"])
                    .arg(&path)
                    .output()
                    .await?;
                if output.status.success() {
                    return Ok(());
                }

                let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
                let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
                warn!(
                    workspace = %path.display(),
                    source_repo = %source_repo_root.display(),
                    stdout,
                    stderr,
                    "git worktree remove failed; falling back to recursive delete"
                );
            }
        }

        tokio::fs::remove_dir_all(path).await?;
        Ok(())
    }

    async fn run_optional_hook(
        &self,
        name: &str,
        script: Option<&str>,
        cwd: &Path,
        fatal: bool,
    ) -> Result<()> {
        let Some(script) = script else {
            return Ok(());
        };

        info!(hook = name, workspace = %cwd.display(), "running workspace hook");
        let output = timeout(
            Duration::from_millis(self.hooks.timeout_ms),
            Command::new("bash")
                .arg("-lc")
                .arg(script)
                .current_dir(cwd)
                .output(),
        )
        .await
        .map_err(|_| LunaError::Workspace(format!("hook timed out: {name}")))??;

        if !output.status.success() {
            let stderr = truncate_output(&String::from_utf8_lossy(&output.stderr));
            let stdout = truncate_output(&String::from_utf8_lossy(&output.stdout));
            let message = format!(
                "hook failed: {name}, status={}, stdout={stdout:?}, stderr={stderr:?}",
                output.status
            );
            if fatal {
                return Err(LunaError::Workspace(message));
            }
            warn!(hook = name, workspace = %cwd.display(), message, "hook failure ignored");
        }

        Ok(())
    }
}

pub fn sanitize_workspace_key(issue_identifier: &str) -> String {
    issue_identifier
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn ensure_within_root(root: &Path, path: &Path) -> Result<()> {
    let root = normalize(root);
    let path = normalize(path);
    if !path.starts_with(&root) {
        return Err(LunaError::Workspace(format!(
            "workspace path escaped root: {} not under {}",
            path.display(),
            root.display()
        )));
    }
    Ok(())
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        normalized.push(component);
    }
    normalized
}

fn truncate_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.len() <= HOOK_OUTPUT_LIMIT {
        trimmed.to_string()
    } else {
        format!("{}...", &trimmed[..HOOK_OUTPUT_LIMIT])
    }
}

async fn is_directory_empty(path: &Path) -> Result<bool> {
    let mut entries = tokio::fs::read_dir(path).await?;
    Ok(entries.next_entry().await?.is_none())
}

#[cfg(test)]
mod tests {
    use super::sanitize_workspace_key;

    #[test]
    fn sanitizes_workspace_keys() {
        assert_eq!(sanitize_workspace_key("ABC-123"), "ABC-123");
        assert_eq!(sanitize_workspace_key("ABC/123 hello"), "ABC_123_hello");
    }
}
