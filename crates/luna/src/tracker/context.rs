use std::{
    env,
    path::{Path, PathBuf},
};

use tokio::process::Command;

use crate::{
    error::{LunaError, Result},
    model::Issue,
    workflow::WorkflowStore,
};

use super::{Tracker, build_tracker};

#[derive(Debug, Clone)]
pub struct TrackerTargetOptions {
    pub workflow_path: PathBuf,
    pub issue_locator: Option<String>,
    pub cwd: PathBuf,
}

pub async fn resolve_tracker_issue(
    options: &TrackerTargetOptions,
) -> Result<(Box<dyn Tracker>, Issue)> {
    let store = WorkflowStore::load(options.workflow_path.clone())?;
    let workflow = store.current().clone();
    let tracker = build_tracker(&workflow.config.tracker)?;
    let issue = resolve_issue(tracker.as_ref(), options).await?;
    Ok((tracker, issue))
}

pub async fn resolve_issue(tracker: &dyn Tracker, options: &TrackerTargetOptions) -> Result<Issue> {
    let locators = collect_issue_locators(options).await?;
    if locators.is_empty() {
        return Err(LunaError::Tracker(
            "could not determine current issue; pass --issue explicitly".to_string(),
        ));
    }

    for locator in &locators {
        if let Some(issue) = tracker.find_issue_by_locator(locator).await? {
            return Ok(issue);
        }
    }

    Err(LunaError::Tracker(format!(
        "could not resolve issue from current context ({}); pass --issue explicitly",
        locators.join(", ")
    )))
}

async fn collect_issue_locators(options: &TrackerTargetOptions) -> Result<Vec<String>> {
    let mut locators = Vec::new();

    if let Some(locator) = options.issue_locator.as_deref() {
        push_locator(&mut locators, locator);
        return Ok(locators);
    }

    if let Ok(value) = env::var("LUNA_ISSUE_ID") {
        push_locator(&mut locators, &value);
    }
    if let Ok(value) = env::var("LUNA_ISSUE_IDENTIFIER") {
        push_locator(&mut locators, &value);
    }
    if !locators.is_empty() {
        return Ok(locators);
    }
    if let Some(locator) = detect_workspace_locator(&options.cwd).await? {
        push_locator(&mut locators, &locator);
    }

    Ok(locators)
}

fn push_locator(locators: &mut Vec<String>, value: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    if locators
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(trimmed))
    {
        return;
    }
    locators.push(trimmed.to_string());
}

async fn detect_workspace_locator(cwd: &Path) -> Result<Option<String>> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(cwd)
        .output()
        .await?;

    if !output.status.success() {
        return Ok(cwd
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.to_string()));
    }

    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if root.is_empty() {
        return Ok(None);
    }

    Ok(Path::new(&root)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string()))
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        process::Command as StdCommand,
        sync::{Arc, Mutex},
    };

    use async_trait::async_trait;
    use tempfile::tempdir;

    use crate::{
        error::Result,
        model::{Comment, Issue},
        tracker::Tracker,
    };

    use super::{TrackerTargetOptions, detect_workspace_locator, push_locator, resolve_issue};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvRestore {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            // Tests that mutate process env hold ENV_LOCK and restore it before releasing.
            unsafe {
                if let Some(previous) = self.previous.take() {
                    std::env::set_var(self.key, previous);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    fn set_env_for_test(key: &'static str, value: &str) -> EnvRestore {
        let previous = std::env::var_os(key);
        // Tests that mutate process env hold ENV_LOCK and restore it before releasing.
        unsafe {
            std::env::set_var(key, value);
        }
        EnvRestore { key, previous }
    }

    #[derive(Clone)]
    struct MemoryTracker {
        issues: Vec<Issue>,
        locators: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Tracker for MemoryTracker {
        async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
            unreachable!("not used by resolve_issue")
        }

        async fn fetch_issues_by_states(&self, _states: &[String]) -> Result<Vec<Issue>> {
            unreachable!("not used by resolve_issue")
        }

        async fn fetch_issue_states_by_ids(&self, _issue_ids: &[String]) -> Result<Vec<Issue>> {
            unreachable!("not used by resolve_issue")
        }

        async fn find_issue_by_locator(&self, locator: &str) -> Result<Option<Issue>> {
            self.locators.lock().unwrap().push(locator.to_string());
            Ok(self
                .issues
                .iter()
                .find(|issue| issue.id == locator || issue.identifier.eq_ignore_ascii_case(locator))
                .cloned())
        }

        async fn fetch_comments(&self, _issue: &Issue) -> Result<Vec<Comment>> {
            unreachable!("not used by resolve_issue")
        }

        async fn create_comment(&self, _issue: &Issue, _body: &str) -> Result<()> {
            unreachable!("not used by resolve_issue")
        }

        async fn update_issue_state(&self, _issue_id: &str, _state_name: &str) -> Result<()> {
            unreachable!("not used by resolve_issue")
        }
    }

    fn issue(id: &str, identifier: &str) -> Issue {
        Issue {
            id: id.to_string(),
            identifier: identifier.to_string(),
            title: format!("Issue {identifier}"),
            description: None,
            priority: None,
            state: "Todo".to_string(),
            branch_name: None,
            url: None,
            labels: Vec::new(),
            blocked_by: Vec::new(),
            created_at: None,
            updated_at: None,
            project: None,
            source_data: None,
        }
    }

    #[tokio::test]
    async fn resolve_issue_uses_explicit_locator_first_and_trims_it() {
        let locators = Arc::new(Mutex::new(Vec::new()));
        let tracker = MemoryTracker {
            issues: vec![issue("id-2", "COD-2")],
            locators: Arc::clone(&locators),
        };
        let temp = tempdir().unwrap();
        let options = TrackerTargetOptions {
            workflow_path: temp.path().join("WORKFLOW.md"),
            issue_locator: Some("  COD-2  ".to_string()),
            cwd: temp.path().to_path_buf(),
        };

        let resolved = resolve_issue(&tracker, &options).await.unwrap();

        assert_eq!(resolved.identifier, "COD-2");
        assert_eq!(*locators.lock().unwrap(), vec!["COD-2"]);
    }

    #[tokio::test]
    async fn detect_workspace_locator_falls_back_to_directory_name_outside_git() {
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("acme_repo_42");
        tokio::fs::create_dir(&workspace).await.unwrap();

        let locator = detect_workspace_locator(&workspace).await.unwrap();

        assert_eq!(locator.as_deref(), Some("acme_repo_42"));
    }

    #[tokio::test]
    async fn detect_workspace_locator_uses_git_root_directory_name() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo-root");
        let nested = repo.join("nested");
        tokio::fs::create_dir_all(&nested).await.unwrap();
        let status = StdCommand::new("git")
            .arg("init")
            .arg("-q")
            .arg(&repo)
            .status()
            .expect("git init");
        assert!(status.success());

        let locator = detect_workspace_locator(&nested).await.unwrap();

        assert_eq!(locator.as_deref(), Some("repo-root"));
    }

    #[tokio::test]
    async fn resolve_issue_uses_luna_env_locators_before_workspace_locator() {
        let _guard = ENV_LOCK.lock().unwrap();
        let _id_restore = set_env_for_test("LUNA_ISSUE_ID", "missing-id");
        let _identifier_restore = set_env_for_test("LUNA_ISSUE_IDENTIFIER", " COD-2 ");
        let locators = Arc::new(Mutex::new(Vec::new()));
        let tracker = MemoryTracker {
            issues: vec![issue("id-2", "COD-2")],
            locators: Arc::clone(&locators),
        };
        let temp = tempdir().unwrap();
        let workspace = temp.path().join("workspace-name");
        tokio::fs::create_dir(&workspace).await.unwrap();
        let options = TrackerTargetOptions {
            workflow_path: temp.path().join("WORKFLOW.md"),
            issue_locator: None,
            cwd: workspace,
        };

        let resolved = resolve_issue(&tracker, &options).await.unwrap();

        assert_eq!(resolved.identifier, "COD-2");
        assert_eq!(*locators.lock().unwrap(), vec!["missing-id", "COD-2"]);
    }

    #[tokio::test]
    async fn resolve_issue_reports_attempted_locators_when_missing() {
        let locators = Arc::new(Mutex::new(Vec::new()));
        let tracker = MemoryTracker {
            issues: vec![issue("id-1", "COD-1")],
            locators,
        };
        let temp = tempdir().unwrap();
        let options = TrackerTargetOptions {
            workflow_path: temp.path().join("WORKFLOW.md"),
            issue_locator: Some("COD-404".to_string()),
            cwd: temp.path().to_path_buf(),
        };

        let err = resolve_issue(&tracker, &options).await.unwrap_err();

        assert!(err.to_string().contains("COD-404"));
        assert!(err.to_string().contains("pass --issue explicitly"));
    }

    #[test]
    fn push_locator_drops_blank_and_case_insensitive_duplicates() {
        let mut locators = Vec::new();

        push_locator(&mut locators, "  ");
        push_locator(&mut locators, "COD-1");
        push_locator(&mut locators, "cod-1");
        push_locator(&mut locators, "COD-2");

        assert_eq!(locators, vec!["COD-1", "COD-2"]);
    }
}
