use crate::{error::Result, model::Issue};

use super::context::{TrackerTargetOptions, resolve_tracker_issue};

#[derive(Debug, Clone)]
pub struct CommentCommandOptions {
    pub target: TrackerTargetOptions,
    pub body: String,
}

#[derive(Debug, Clone)]
pub struct ShowCommandOptions {
    pub target: TrackerTargetOptions,
    pub json: bool,
}

#[derive(Debug, Clone)]
pub struct MoveCommandOptions {
    pub target: TrackerTargetOptions,
    pub state: String,
}

pub async fn run_comment_command(options: CommentCommandOptions) -> Result<String> {
    let (tracker, issue) = resolve_tracker_issue(&options.target).await?;
    tracker.create_comment(&issue, &options.body).await?;
    Ok(issue.identifier)
}

pub async fn run_show_command(options: ShowCommandOptions) -> Result<String> {
    let (_tracker, issue) = resolve_tracker_issue(&options.target).await?;
    if options.json {
        Ok(serde_json::to_string_pretty(&issue)?)
    } else {
        Ok(format_issue(&issue))
    }
}

pub async fn run_move_command(options: MoveCommandOptions) -> Result<String> {
    let (tracker, issue) = resolve_tracker_issue(&options.target).await?;
    tracker
        .update_issue_state(&issue.id, &options.state)
        .await?;
    Ok(issue.identifier)
}

fn format_issue(issue: &Issue) -> String {
    let mut output = Vec::new();
    output.push(format!("Issue: {}", issue.identifier));
    output.push(format!("Title: {}", issue.title));
    output.push(format!("State: {}", issue.state));

    if let Some(priority) = issue.priority {
        output.push(format!("Priority: {priority}"));
    }
    if let Some(url) = issue.url.as_deref() {
        output.push(format!("URL: {url}"));
    }
    if let Some(branch_name) = issue.branch_name.as_deref() {
        output.push(format!("Branch: {branch_name}"));
    }
    if !issue.labels.is_empty() {
        output.push(format!("Labels: {}", issue.labels.join(", ")));
    }
    if !issue.blocked_by.is_empty() {
        let blocked_by = issue
            .blocked_by
            .iter()
            .map(|blocker| {
                blocker
                    .identifier
                    .clone()
                    .or_else(|| blocker.id.clone())
                    .unwrap_or_else(|| "unknown".to_string())
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push(format!("Blocked by: {blocked_by}"));
    }
    if let Some(description) = issue.description.as_deref() {
        output.push(String::new());
        output.push("Description:".to_string());
        output.push(description.to_string());
    }

    output.join("\n")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::{Value, json};
    use tempfile::{TempDir, tempdir};

    use crate::model::Issue;
    use crate::{
        test_support::{MockHttpServer, MockResponse, issue_json},
        tracker::TrackerTargetOptions,
    };

    use super::{
        CommentCommandOptions, MoveCommandOptions, ShowCommandOptions, format_issue,
        run_comment_command, run_move_command, run_show_command,
    };

    struct FakeGh {
        _dir: TempDir,
        command: String,
        log_path: std::path::PathBuf,
    }

    fn fake_gh() -> FakeGh {
        let dir = tempdir().unwrap();
        let gh_path = dir.path().join("gh");
        let log_path = dir.path().join("calls.log");
        let script = format!(
            r#"#!/usr/bin/env bash
set -euo pipefail
CALL_LOG='{}'
printf '%q ' "$@" >> "$CALL_LOG"
printf '\n' >> "$CALL_LOG"
joined="$*"
if [[ "${{1:-}}" == "api" && "${{2:-}}" == "graphql" ]]; then
  if [[ "$joined" == *"itemId=PVTI_1"* ]]; then
    cat <<'JSON'
{{"data":{{"updateProjectV2ItemFieldValue":{{"projectV2Item":{{"id":"PVTI_1"}}}}}}}}
JSON
  elif [[ "$joined" == *"statusField=Status"* && "$joined" != *"priorityField=Priority"* ]]; then
    cat <<'JSON'
{{"data":{{"repositoryOwner":{{"projectV2":{{"id":"PVT_1","fields":{{"nodes":[{{"__typename":"ProjectV2SingleSelectField","id":"FIELD_status","name":"Status","options":[{{"id":"OPT_done","name":"Done"}},{{"id":"OPT_progress","name":"In Progress"}}]}}]}}}}}}}}}}
JSON
  else
    cat <<'JSON'
{{"data":{{"repositoryOwner":{{"projectV2":{{"url":"https://github.com/orgs/acme/projects/12","items":{{"pageInfo":{{"hasNextPage":false,"endCursor":null}},"nodes":[{{"id":"PVTI_1","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","statusFieldValue":{{"__typename":"ProjectV2ItemFieldSingleSelectValue","name":"Todo"}},"priorityFieldValue":{{"__typename":"ProjectV2ItemFieldTextValue","text":"P1"}},"content":{{"__typename":"Issue","id":"I_42","number":42,"title":"Fix GitHub workflow","body":"Body","url":"https://github.com/acme/repo/issues/42","state":"OPEN","closed":false,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","repository":{{"nameWithOwner":"acme/repo"}},"labels":{{"nodes":[{{"name":"CI"}}]}}}}}}]}}}}}}}}}}
JSON
  fi
elif [[ "${{1:-}}" == "issue" && "${{2:-}}" == "comment" ]]; then
  exit 0
else
  echo "unexpected gh invocation: $joined" >&2
  exit 64
fi
"#,
            log_path.display()
        );
        fs::write(&gh_path, script).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&gh_path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&gh_path, permissions).unwrap();
        }

        FakeGh {
            _dir: dir,
            command: gh_path.to_string_lossy().to_string(),
            log_path,
        }
    }

    async fn workflow_for_endpoint(endpoint: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: asahi
  endpoint: {endpoint}
runner:
  kind: codex
---
test prompt
"#
            ),
        )
        .await
        .unwrap();
        (temp, workflow_path)
    }

    async fn github_workflow_for_gh(gh_command: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let temp = tempdir().unwrap();
        let workflow_path = temp.path().join("WORKFLOW.md");
        tokio::fs::write(
            &workflow_path,
            format!(
                r#"---
tracker:
  kind: github_project
  owner: acme
  project_number: 12
  gh_command: "{}"
runner:
  kind: codex
---
test prompt
"#,
                gh_command.replace('"', "\\\"")
            ),
        )
        .await
        .unwrap();
        (temp, workflow_path)
    }

    fn target(workflow_path: std::path::PathBuf, issue: &str) -> TrackerTargetOptions {
        TrackerTargetOptions {
            workflow_path,
            issue_locator: Some(issue.to_string()),
            cwd: std::env::temp_dir(),
        }
    }

    #[test]
    fn formats_issue_summary() {
        let issue = Issue {
            id: "id".to_string(),
            identifier: "ENG-42".to_string(),
            title: "Fix tracker CLI".to_string(),
            description: Some("Detailed description".to_string()),
            priority: Some(1),
            state: "In Progress".to_string(),
            branch_name: Some("eng-42".to_string()),
            url: Some("https://example.com".to_string()),
            labels: vec!["backend".to_string(), "cli".to_string()],
            blocked_by: Vec::new(),
            created_at: None,
            updated_at: None,
            project: None,
            source_data: None,
        };

        let text = format_issue(&issue);
        assert!(text.contains("Issue: ENG-42"));
        assert!(text.contains("Priority: 1"));
        assert!(text.contains("Description:"));
    }

    #[tokio::test]
    async fn show_command_resolves_asahi_issue_as_text_and_json() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, issue_json("1", "ASAHI-1", "Todo", None)),
            MockResponse::json(200, issue_json("1", "ASAHI-1", "Todo", None)),
        ])
        .await;
        let (_temp, workflow_path) = workflow_for_endpoint(&server.endpoint).await;

        let text = run_show_command(ShowCommandOptions {
            target: target(workflow_path.clone(), "ASAHI-1"),
            json: false,
        })
        .await
        .unwrap();
        let json_text = run_show_command(ShowCommandOptions {
            target: target(workflow_path, "ASAHI-1"),
            json: true,
        })
        .await
        .unwrap();

        assert!(text.contains("Issue: ASAHI-1"));
        assert!(text.contains("Labels: backend, codex"));
        let parsed: Value = serde_json::from_str(&json_text).unwrap();
        assert_eq!(parsed["identifier"], "ASAHI-1");
        let requests = server.recorded_requests().await;
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].target, "/api/issues/ASAHI-1");
        assert_eq!(requests[1].target, "/api/issues/ASAHI-1");
    }

    #[tokio::test]
    async fn comment_command_posts_to_resolved_asahi_issue_id() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, issue_json("issue-id", "ASAHI-2", "In Progress", None)),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let (_temp, workflow_path) = workflow_for_endpoint(&server.endpoint).await;

        let identifier = run_comment_command(CommentCommandOptions {
            target: target(workflow_path, "ASAHI-2"),
            body: "status update".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(identifier, "ASAHI-2");
        let requests = server.recorded_requests().await;
        assert_eq!(requests[0].target, "/api/issues/ASAHI-2");
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].target, "/api/issues/issue-id/comments");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[1].body).unwrap(),
            json!({"body": "status update"})
        );
    }

    #[tokio::test]
    async fn move_command_patches_resolved_asahi_issue_id() {
        let server = MockHttpServer::spawn(vec![
            MockResponse::json(200, issue_json("issue-id", "ASAHI-3", "Todo", None)),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let (_temp, workflow_path) = workflow_for_endpoint(&server.endpoint).await;

        let identifier = run_move_command(MoveCommandOptions {
            target: target(workflow_path, "ASAHI-3"),
            state: "Done".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(identifier, "ASAHI-3");
        let requests = server.recorded_requests().await;
        assert_eq!(requests[0].target, "/api/issues/ASAHI-3");
        assert_eq!(requests[1].method, "PATCH");
        assert_eq!(requests[1].target, "/api/issues/issue-id/state");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[1].body).unwrap(),
            json!({"state": "Done"})
        );
    }

    #[tokio::test]
    async fn show_command_resolves_github_project_issue_as_text_and_json() {
        let fake = fake_gh();
        let (_temp, workflow_path) = github_workflow_for_gh(&fake.command).await;

        let text = run_show_command(ShowCommandOptions {
            target: target(workflow_path.clone(), "acme/repo#42"),
            json: false,
        })
        .await
        .unwrap();
        let json_text = run_show_command(ShowCommandOptions {
            target: target(workflow_path, "acme_repo_42"),
            json: true,
        })
        .await
        .unwrap();

        assert!(text.contains("Issue: acme/repo#42"));
        assert!(text.contains("Labels: ci"));
        let parsed: Value = serde_json::from_str(&json_text).unwrap();
        assert_eq!(parsed["id"], "PVTI_1");
        assert_eq!(parsed["identifier"], "acme/repo#42");
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.contains("api graphql"));
    }

    #[tokio::test]
    async fn comment_command_posts_to_github_backing_issue() {
        let fake = fake_gh();
        let (_temp, workflow_path) = github_workflow_for_gh(&fake.command).await;

        let identifier = run_comment_command(CommentCommandOptions {
            target: target(workflow_path, "acme/repo#42"),
            body: "ship it".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(identifier, "acme/repo#42");
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert!(calls.contains("api graphql"));
        assert!(calls.contains("issue comment 42 -R acme/repo --body"));
        assert!(calls.contains("ship\\ it"));
    }

    #[tokio::test]
    async fn move_command_updates_github_project_item_status() {
        let fake = fake_gh();
        let (_temp, workflow_path) = github_workflow_for_gh(&fake.command).await;

        let identifier = run_move_command(MoveCommandOptions {
            target: target(workflow_path, "acme/repo#42"),
            state: "Done".to_string(),
        })
        .await
        .unwrap();

        assert_eq!(identifier, "acme/repo#42");
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert_eq!(calls.lines().count(), 3);
        assert!(calls.contains("api graphql"));
        assert!(calls.contains("itemId=PVTI_1"));
        assert!(calls.contains("optionId=OPT_done"));
    }
}
