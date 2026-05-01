use bashkit::ExecResult;

use crate::{
    config::TrackerConfig,
    error::{LunaError, Result},
    tracker::{
        AsahiTracker,
        context::{TrackerTargetOptions, resolve_issue},
    },
    workflow::WorkflowStore,
};

use super::{fs::build_wiki_fs, shell::WikiShell};

#[derive(Debug, Clone)]
pub struct WikiCommandOptions {
    pub target: TrackerTargetOptions,
    pub args: Vec<String>,
}

pub async fn run_wiki_command(options: WikiCommandOptions) -> Result<ExecResult> {
    // 1. Load workflow and verify Asahi tracker
    let store = WorkflowStore::load(options.target.workflow_path.clone())?;
    let workflow = store.current().clone();

    let asahi_config = match &workflow.config.tracker {
        TrackerConfig::Asahi(config) => config.clone(),
        _ => {
            return Err(LunaError::Tracker(
                "wiki is only supported with the asahi tracker".to_string(),
            ))
        }
    };

    // 2. Create tracker and resolve current issue
    let tracker = AsahiTracker::new(asahi_config);
    let issue = resolve_issue(&tracker, &options.target).await?;

    // 3. Extract project from issue
    let project = issue.project.ok_or_else(|| {
        LunaError::Tracker(
            "wiki requires an issue associated with a project".to_string(),
        )
    })?;

    // 4. Fetch all wiki nodes for the project
    let nodes = tracker.fetch_project_wiki(&project.slug).await?;

    if nodes.is_empty() {
        return Ok(ExecResult {
            stdout: String::new(),
            stderr: "project wiki is empty".to_string(),
            exit_code: 0,
            ..Default::default()
        });
    }

    // 5. Build virtual filesystem
    let fs = build_wiki_fs(nodes).await?;

    // 6. Execute shell command in sandbox
    let command = if options.args.is_empty() {
        "ls".to_string()
    } else {
        options.args.join(" ")
    };

    let mut shell = WikiShell::new(fs);
    let result = shell.exec(&command).await?;

    Ok(result)
}
