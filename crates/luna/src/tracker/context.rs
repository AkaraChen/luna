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
