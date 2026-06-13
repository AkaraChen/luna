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
            ));
        }
    };

    // 2. Create tracker and resolve current issue
    let tracker = AsahiTracker::new(asahi_config);
    let issue = resolve_issue(&tracker, &options.target).await?;

    // 3. Extract project from issue
    let project = issue.project.ok_or_else(|| {
        LunaError::Tracker("wiki requires an issue associated with a project".to_string())
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tempfile::tempdir;

    use crate::{
        test_support::{MockHttpServer, MockResponse, issue_json},
        tracker::TrackerTargetOptions,
    };

    use super::{WikiCommandOptions, run_wiki_command};

    async fn write_workflow(contents: String) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(&workflow_path, contents).await.unwrap();
        (temp, workflow_path)
    }

    async fn asahi_workflow(endpoint: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        write_workflow(format!(
            r#"---
tracker:
  kind: asahi
  endpoint: {endpoint}
runner:
  kind: codex
---
test prompt
"#
        ))
        .await
    }

    fn target(workflow_path: std::path::PathBuf, issue: &str) -> TrackerTargetOptions {
        TrackerTargetOptions {
            workflow_path,
            issue_locator: Some(issue.to_string()),
            cwd: std::env::temp_dir(),
        }
    }

    #[tokio::test]
    async fn wiki_command_rejects_non_asahi_tracker_even_with_codex_runner() {
        let (_temp, workflow_path) = write_workflow(
            r#"---
tracker:
  kind: github_project
  owner: acme
  project_number: 12
runner:
  kind: codex
---
test prompt
"#
            .to_string(),
        )
        .await;

        let err = run_wiki_command(WikiCommandOptions {
            target: target(workflow_path, "acme/repo#1"),
            args: vec!["ls".to_string()],
        })
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("only supported with the asahi tracker")
        );
    }

    #[tokio::test]
    async fn wiki_command_fetches_project_wiki_and_runs_shell_command() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                issue_json("issue-id", "ASAHI-1", "Todo", Some("proj-a")),
            ),
            MockResponse::json(
                200,
                json!({
                    "nodes": [
                        {
                            "id": "folder-1",
                            "project_id": "project-proj-a",
                            "parent_id": null,
                            "kind": "folder",
                            "title": "Guides",
                            "slug": "guides",
                            "content": null,
                            "current_version": null,
                            "created_at": null,
                            "updated_at": null,
                            "deleted_at": null
                        },
                        {
                            "id": "page-1",
                            "project_id": "project-proj-a",
                            "parent_id": null,
                            "kind": "page",
                            "title": "Readme",
                            "slug": "readme",
                            "content": "<p>Hello wiki</p>",
                            "current_version": null,
                            "created_at": null,
                            "updated_at": null,
                            "deleted_at": null
                        },
                        {
                            "id": "page-2",
                            "project_id": "project-proj-a",
                            "parent_id": "folder-1",
                            "kind": "page",
                            "title": "Design",
                            "slug": "design",
                            "content": "<h1>Design</h1>",
                            "current_version": null,
                            "created_at": null,
                            "updated_at": null,
                            "deleted_at": null
                        }
                    ]
                }),
            ),
        ])
        .await;
        let (_temp, workflow_path) = asahi_workflow(&server.endpoint).await;

        let result = run_wiki_command(WikiCommandOptions {
            target: target(workflow_path, "ASAHI-1"),
            args: vec!["cat".to_string(), "readme.md".to_string()],
        })
        .await
        .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("Hello wiki"));
        let requests = server.recorded_requests().await;
        assert_eq!(requests[0].target, "/api/issues/ASAHI-1");
        assert_eq!(requests[1].method, "GET");
        assert!(requests[1].target.starts_with("/api/projects/proj-a/wiki?"));
        assert!(requests[1].target.contains("recursive=true"));
    }

    #[tokio::test]
    async fn wiki_command_returns_empty_message_for_empty_project_wiki() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(
                200,
                issue_json("issue-id", "ASAHI-2", "Todo", Some("proj-a")),
            ),
            MockResponse::json(200, json!({ "nodes": [] })),
        ])
        .await;
        let (_temp, workflow_path) = asahi_workflow(&server.endpoint).await;

        let result = run_wiki_command(WikiCommandOptions {
            target: target(workflow_path, "ASAHI-2"),
            args: Vec::new(),
        })
        .await
        .unwrap();

        assert_eq!(result.exit_code, 0);
        assert_eq!(result.stdout, "");
        assert_eq!(result.stderr, "project wiki is empty");
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 2);
    }

    #[tokio::test]
    async fn wiki_command_requires_issue_project_association() {
        let server = MockHttpServer::spawn(vec![MockResponse::json(
            200,
            issue_json("issue-id", "ASAHI-3", "Todo", None),
        )])
        .await;
        let (_temp, workflow_path) = asahi_workflow(&server.endpoint).await;

        let err = run_wiki_command(WikiCommandOptions {
            target: target(workflow_path, "ASAHI-3"),
            args: Vec::new(),
        })
        .await
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("requires an issue associated with a project")
        );
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, "/api/issues/ASAHI-3");
    }
}
