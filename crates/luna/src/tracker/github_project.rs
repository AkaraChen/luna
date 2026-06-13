use std::{collections::HashSet, process::Output};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::{
    config::GitHubProjectTrackerConfig,
    error::{LunaError, Result},
    model::Issue,
    tracker::Tracker,
    workspace::sanitize_workspace_key,
};

#[derive(Clone, Debug)]
pub struct GitHubProjectTracker {
    config: GitHubProjectTrackerConfig,
}

impl GitHubProjectTracker {
    pub fn new(config: GitHubProjectTrackerConfig) -> Self {
        Self { config }
    }

    async fn graphql<T>(&self, query: &str, fields: &[(&str, String)]) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut command = Command::new(&self.config.gh_command);
        command.arg("api").arg("graphql");
        command.arg("-f").arg(format!("query={query}"));
        for (key, value) in fields {
            command.arg("-F").arg(format!("{key}={value}"));
        }

        let output = command.output().await?;
        parse_gh_json_output("gh api graphql", output)
    }

    async fn fetch_all_items(&self) -> Result<Vec<Issue>> {
        let mut issues = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let payload = self.fetch_page(cursor.clone()).await?;
            let project = payload
                .data
                .repository_owner
                .and_then(|owner| owner.project_v2)
                .ok_or_else(|| {
                    LunaError::Tracker(format!(
                        "github_project_not_found: owner={}, project_number={}",
                        self.config.owner, self.config.project_number
                    ))
                })?;

            issues.extend(
                project
                    .items
                    .nodes
                    .into_iter()
                    .filter_map(|node| normalize_project_item(node, &project.url, &self.config)),
            );

            if !project.items.page_info.has_next_page {
                break;
            }

            cursor = project.items.page_info.end_cursor;
            if cursor.is_none() {
                return Err(LunaError::Tracker(
                    "github_project_missing_end_cursor: pagination reported hasNextPage=true without endCursor"
                        .to_string(),
                ));
            }
        }

        Ok(issues)
    }

    async fn fetch_page(&self, cursor: Option<String>) -> Result<ProjectItemsResponse> {
        let query = r#"
query ProjectItems(
  $owner: String!
  $projectNumber: Int!
  $statusField: String!
  $priorityField: String!
  $cursor: String
) {
  repositoryOwner(login: $owner) {
    ... on User {
      projectV2(number: $projectNumber) {
        url
        items(first: 50, after: $cursor) {
          pageInfo {
            hasNextPage
            endCursor
          }
          nodes {
            id
            createdAt
            updatedAt
            statusFieldValue: fieldValueByName(name: $statusField) {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
              }
              ... on ProjectV2ItemFieldTextValue {
                text
              }
            }
            priorityFieldValue: fieldValueByName(name: $priorityField) {
              __typename
              ... on ProjectV2ItemFieldNumberValue {
                number
              }
              ... on ProjectV2ItemFieldTextValue {
                text
              }
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
              }
            }
            content {
              __typename
              ... on Issue {
                id
                number
                title
                body
                url
                state
                closed
                createdAt
                updatedAt
                repository {
                  nameWithOwner
                }
                labels(first: 20) {
                  nodes {
                    name
                  }
                }
              }
              ... on DraftIssue {
                id
                title
                body
                createdAt
                updatedAt
              }
              ... on PullRequest {
                id
              }
            }
          }
        }
      }
    }
    ... on Organization {
      projectV2(number: $projectNumber) {
        url
        items(first: 50, after: $cursor) {
          pageInfo {
            hasNextPage
            endCursor
          }
          nodes {
            id
            createdAt
            updatedAt
            statusFieldValue: fieldValueByName(name: $statusField) {
              __typename
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
              }
              ... on ProjectV2ItemFieldTextValue {
                text
              }
            }
            priorityFieldValue: fieldValueByName(name: $priorityField) {
              __typename
              ... on ProjectV2ItemFieldNumberValue {
                number
              }
              ... on ProjectV2ItemFieldTextValue {
                text
              }
              ... on ProjectV2ItemFieldSingleSelectValue {
                name
              }
            }
            content {
              __typename
              ... on Issue {
                id
                number
                title
                body
                url
                state
                closed
                createdAt
                updatedAt
                repository {
                  nameWithOwner
                }
                labels(first: 20) {
                  nodes {
                    name
                  }
                }
              }
              ... on DraftIssue {
                id
                title
                body
                createdAt
                updatedAt
              }
              ... on PullRequest {
                id
              }
            }
          }
        }
      }
    }
  }
}
"#;

        let mut fields = vec![
            ("owner", self.config.owner.clone()),
            ("projectNumber", self.config.project_number.to_string()),
            ("statusField", self.config.status_field.clone()),
            ("priorityField", self.config.priority_field.clone()),
        ];
        if let Some(cursor) = cursor {
            fields.push(("cursor", cursor));
        }

        self.graphql(query, &fields).await
    }

    async fn resolve_status_field_option(
        &self,
        state_name: &str,
    ) -> Result<ResolvedProjectStatusField> {
        let query = r#"
query ProjectStatusField(
  $owner: String!
  $projectNumber: Int!
  $statusField: String!
) {
  repositoryOwner(login: $owner) {
    ... on User {
      projectV2(number: $projectNumber) {
        id
        fields(first: 50) {
          nodes {
            __typename
            ... on ProjectV2SingleSelectField {
              id
              name
              options {
                id
                name
              }
            }
          }
        }
      }
    }
    ... on Organization {
      projectV2(number: $projectNumber) {
        id
        fields(first: 50) {
          nodes {
            __typename
            ... on ProjectV2SingleSelectField {
              id
              name
              options {
                id
                name
              }
            }
          }
        }
      }
    }
  }
}
"#;

        let payload: ProjectStatusFieldResponse = self
            .graphql(
                query,
                &[
                    ("owner", self.config.owner.clone()),
                    ("projectNumber", self.config.project_number.to_string()),
                    ("statusField", self.config.status_field.clone()),
                ],
            )
            .await?;

        let project = payload
            .data
            .repository_owner
            .and_then(|owner| owner.project_v2)
            .ok_or_else(|| {
                LunaError::Tracker(format!(
                    "github_project_not_found: owner={}, project_number={}",
                    self.config.owner, self.config.project_number
                ))
            })?;

        let field = project
            .fields
            .nodes
            .into_iter()
            .find_map(|field| match field {
                ProjectFieldConfig::ProjectV2SingleSelectField { id, name, options }
                    if name.eq_ignore_ascii_case(&self.config.status_field) =>
                {
                    Some((id, options))
                }
                _ => None,
            })
            .ok_or_else(|| {
                LunaError::Tracker(format!(
                    "github_project_status_field_not_found: {}",
                    self.config.status_field
                ))
            })?;

        let option_id = field
            .1
            .into_iter()
            .find(|option| option.name.eq_ignore_ascii_case(state_name))
            .map(|option| option.id)
            .ok_or_else(|| {
                LunaError::Tracker(format!(
                    "github_project_state_option_not_found: {}",
                    state_name
                ))
            })?;

        Ok(ResolvedProjectStatusField {
            project_id: project.id,
            field_id: field.0,
            option_id,
        })
    }
}

#[async_trait]
impl Tracker for GitHubProjectTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        let all = self.fetch_all_items().await?;
        Ok(all
            .into_iter()
            .filter(|issue| self.config.is_active_state(&issue.state))
            .collect())
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let lookup = states
            .iter()
            .map(|state| state.to_lowercase())
            .collect::<HashSet<_>>();
        let all = self.fetch_all_items().await?;
        Ok(all
            .into_iter()
            .filter(|issue| lookup.contains(&issue.state.to_lowercase()))
            .collect())
    }

    async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
        if issue_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = issue_ids.iter().cloned().collect::<HashSet<_>>();
        let all = self.fetch_all_items().await?;
        Ok(all
            .into_iter()
            .filter(|issue| ids.contains(&issue.id))
            .collect())
    }

    async fn find_issue_by_locator(&self, locator: &str) -> Result<Option<Issue>> {
        let locator = locator.trim();
        if locator.is_empty() {
            return Ok(None);
        }

        Ok(self
            .fetch_all_items()
            .await?
            .into_iter()
            .find(|issue| issue_matches_locator(issue, locator)))
    }

    async fn fetch_comments(&self, issue: &Issue) -> Result<Vec<crate::model::Comment>> {
        let (repo, number) = match parse_github_issue_reference(&issue.identifier) {
            Some(v) => v,
            None => return Ok(vec![]),
        };

        let output = Command::new(&self.config.gh_command)
            .arg("api")
            .arg(format!("repos/{repo}/issues/{number}/comments"))
            .arg("--paginate")
            .output()
            .await?;

        if !output.status.success() {
            return Err(LunaError::Tracker(format!(
                "gh api issue comments failed: status={}, stderr={}",
                output.status,
                truncate(&String::from_utf8_lossy(&output.stderr))
            )));
        }

        let nodes: Vec<serde_json::Value> =
            serde_json::from_slice(&output.stdout).map_err(LunaError::Json)?;

        let mut comments = Vec::new();
        for node in nodes {
            let id = node
                .get("node_id")
                .or_else(|| node.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = node
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let created_at = node
                .get("created_at")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            comments.push(crate::model::Comment {
                id,
                issue_id: issue.id.clone(),
                body,
                created_at,
            });
        }

        Ok(comments)
    }

    async fn create_comment(&self, issue: &Issue, body: &str) -> Result<()> {
        let (repo, number) = parse_github_issue_reference(&issue.identifier).ok_or_else(|| {
            LunaError::Tracker(format!(
                "github_project comment requires a backing GitHub issue: {}",
                issue.identifier
            ))
        })?;

        let output = Command::new(&self.config.gh_command)
            .arg("issue")
            .arg("comment")
            .arg(number.to_string())
            .arg("-R")
            .arg(repo)
            .arg("--body")
            .arg(body)
            .output()
            .await?;

        if output.status.success() {
            Ok(())
        } else {
            Err(LunaError::Tracker(format!(
                "gh issue comment failed: status={}, stderr={}",
                output.status,
                truncate(&String::from_utf8_lossy(&output.stderr))
            )))
        }
    }

    async fn update_issue_state(&self, issue_id: &str, state_name: &str) -> Result<()> {
        let state_name = state_name.trim();
        if state_name.is_empty() {
            return Err(LunaError::Tracker(
                "state name must be non-empty".to_string(),
            ));
        }

        let resolved = self.resolve_status_field_option(state_name).await?;
        let query = r#"
mutation UpdateProjectItemStatus(
  $projectId: ID!
  $itemId: ID!
  $fieldId: ID!
  $optionId: String!
) {
  updateProjectV2ItemFieldValue(
    input: {
      projectId: $projectId
      itemId: $itemId
      fieldId: $fieldId
      value: {singleSelectOptionId: $optionId}
    }
  ) {
    projectV2Item {
      id
    }
  }
}
"#;

        let response: UpdateProjectItemStatusResponse = self
            .graphql(
                query,
                &[
                    ("projectId", resolved.project_id),
                    ("itemId", issue_id.to_string()),
                    ("fieldId", resolved.field_id),
                    ("optionId", resolved.option_id),
                ],
            )
            .await?;

        let updated_id = response
            .data
            .update_project_v2_item_field_value
            .and_then(|update| update.project_v2_item)
            .map(|item| item.id)
            .ok_or_else(|| LunaError::Tracker("github_project_state_update_failed".to_string()))?;

        if updated_id == issue_id {
            Ok(())
        } else {
            Err(LunaError::Tracker(format!(
                "github_project_state_update_failed: unexpected item id {}",
                updated_id
            )))
        }
    }
}

fn parse_gh_json_output<T>(command: &str, output: Output) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    if !output.status.success() {
        return Err(LunaError::Tracker(format!(
            "{command} failed: status={}, stderr={}",
            output.status,
            truncate(&String::from_utf8_lossy(&output.stderr))
        )));
    }

    serde_json::from_slice(&output.stdout).map_err(Into::into)
}

fn normalize_project_item(
    item: ProjectItemNode,
    project_url: &str,
    config: &GitHubProjectTrackerConfig,
) -> Option<Issue> {
    let source_data = serde_json::to_value(&item).ok();

    let fallback_state = match item
        .content
        .as_ref()
        .unwrap_or(&ProjectItemContent::Unknown)
    {
        ProjectItemContent::Issue(issue) => {
            if issue.closed || issue.state.eq_ignore_ascii_case("closed") {
                "Done".to_string()
            } else {
                "Todo".to_string()
            }
        }
        ProjectItemContent::DraftIssue(_) => "Todo".to_string(),
        ProjectItemContent::PullRequest { .. } => return None,
        ProjectItemContent::Unknown => return None,
    };

    let state = item
        .status
        .as_ref()
        .and_then(ProjectFieldValue::as_state_name)
        .unwrap_or(fallback_state);

    let priority = item
        .priority
        .as_ref()
        .and_then(ProjectFieldValue::as_priority);

    let (identifier, title, description, labels, url, created_at, updated_at) =
        match item.content.unwrap_or(ProjectItemContent::Unknown) {
            ProjectItemContent::Issue(issue) => (
                format!(
                    "{repo}#{number}",
                    repo = issue.repository.name_with_owner,
                    number = issue.number
                ),
                issue.title,
                issue.body,
                issue
                    .labels
                    .nodes
                    .into_iter()
                    .map(|label| label.name.to_lowercase())
                    .collect(),
                Some(issue.url),
                parse_datetime(Some(issue.created_at))
                    .or_else(|| parse_datetime(Some(item.created_at.clone()))),
                parse_datetime(Some(issue.updated_at))
                    .or_else(|| parse_datetime(Some(item.updated_at.clone()))),
            ),
            ProjectItemContent::DraftIssue(draft) => (
                format!(
                    "{owner}/projects/{project}#draft-{suffix}",
                    owner = config.owner,
                    project = config.project_number,
                    suffix = short_item_suffix(&item.id)
                ),
                draft.title,
                draft.body,
                Vec::new(),
                Some(project_url.to_string()),
                parse_datetime(Some(draft.created_at))
                    .or_else(|| parse_datetime(Some(item.created_at.clone()))),
                parse_datetime(Some(draft.updated_at))
                    .or_else(|| parse_datetime(Some(item.updated_at.clone()))),
            ),
            ProjectItemContent::PullRequest { .. } | ProjectItemContent::Unknown => return None,
        };

    Some(Issue {
        id: item.id,
        identifier,
        title,
        description,
        priority,
        state,
        branch_name: None,
        url,
        labels,
        blocked_by: Vec::new(),
        created_at,
        updated_at,
        project: None,
        source_data,
    })
}

fn short_item_suffix(item_id: &str) -> &str {
    item_id
        .rsplit('_')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(item_id)
}

fn parse_datetime(value: Option<String>) -> Option<DateTime<Utc>> {
    value.and_then(|timestamp| {
        DateTime::parse_from_rfc3339(&timestamp)
            .ok()
            .map(|parsed| parsed.with_timezone(&Utc))
    })
}

struct ResolvedProjectStatusField {
    project_id: String,
    field_id: String,
    option_id: String,
}

fn issue_matches_locator(issue: &Issue, locator: &str) -> bool {
    issue.id == locator
        || issue.identifier.eq_ignore_ascii_case(locator)
        || sanitize_workspace_key(&issue.identifier).eq_ignore_ascii_case(locator)
}

fn parse_github_issue_reference(identifier: &str) -> Option<(&str, u64)> {
    let (repo, number) = identifier.rsplit_once('#')?;
    if !repo.contains('/') {
        return None;
    }
    let number = number.parse().ok()?;
    Some((repo, number))
}

fn truncate(value: &str) -> String {
    const LIMIT: usize = 400;
    if value.len() <= LIMIT {
        value.to_string()
    } else {
        format!("{}...", &value[..LIMIT])
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use crate::model::Issue;
    use tempfile::TempDir;

    use crate::{
        config::{TrackerConfig, resolve_service_config},
        model::WorkflowDefinition,
        tracker::{GitHubProjectTracker, Tracker},
    };

    use super::{
        ProjectDraftIssueContent, ProjectFieldValue, ProjectIssueContent, ProjectIssueLabelNode,
        ProjectIssueLabels, ProjectIssueRepository, ProjectItemContent, ProjectItemNode,
        issue_matches_locator, normalize_project_item, parse_gh_json_output,
        parse_github_issue_reference, parse_priority_string,
    };

    fn issue(identifier: &str) -> Issue {
        Issue {
            id: "project-item-id".to_string(),
            identifier: identifier.to_string(),
            title: "title".to_string(),
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

    struct FakeGh {
        _dir: TempDir,
        command: String,
        log_path: std::path::PathBuf,
    }

    fn fake_gh(script_body: &str) -> FakeGh {
        let dir = tempfile::tempdir().unwrap();
        let gh_path = dir.path().join("gh");
        let log_path = dir.path().join("calls.log");
        let script = format!(
            "#!/usr/bin/env bash\nset -euo pipefail\nCALL_LOG='{}'\nlog_args() {{ printf '%q ' \"$@\" >> \"$CALL_LOG\"; printf '\\n' >> \"$CALL_LOG\"; }}\n{}\n",
            log_path.display(),
            script_body
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

    fn tracker_with_gh(gh_command: &str) -> GitHubProjectTracker {
        let escaped = gh_command.replace('\'', "''");
        let yaml = serde_yaml::from_str(&format!(
            r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
  gh_command: '{escaped}'
runner:
  kind: codex
"#
        ))
        .unwrap();
        let definition = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md")).unwrap();
        match config.tracker {
            TrackerConfig::GitHubProject(config) => GitHubProjectTracker::new(config),
            other => panic!("expected github tracker, got {other:?}"),
        }
    }

    fn project_item_issue(
        id: &str,
        status: Option<ProjectFieldValue>,
        priority: Option<ProjectFieldValue>,
        number: i64,
        closed: bool,
    ) -> ProjectItemNode {
        ProjectItemNode {
            id: id.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            status,
            priority,
            content: Some(ProjectItemContent::Issue(ProjectIssueContent {
                number,
                title: format!("Issue {number}"),
                body: Some("Body".to_string()),
                url: format!("https://github.com/acme/repo/issues/{number}"),
                state: if closed { "CLOSED" } else { "OPEN" }.to_string(),
                closed,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-02T00:00:00Z".to_string(),
                repository: ProjectIssueRepository {
                    name_with_owner: "acme/repo".to_string(),
                },
                labels: ProjectIssueLabels {
                    nodes: vec![ProjectIssueLabelNode {
                        name: "Bug".to_string(),
                    }],
                },
            })),
        }
    }

    fn project_item_draft(id: &str) -> ProjectItemNode {
        ProjectItemNode {
            id: id.to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            status: None,
            priority: None,
            content: Some(ProjectItemContent::DraftIssue(ProjectDraftIssueContent {
                title: "Draft task".to_string(),
                body: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                updated_at: "2026-01-02T00:00:00Z".to_string(),
            })),
        }
    }

    #[test]
    fn matches_issue_locator_against_identifier_and_workspace_key() {
        let issue = issue("acme/repo#42");
        assert!(issue_matches_locator(&issue, "acme/repo#42"));
        assert!(issue_matches_locator(&issue, "acme_repo_42"));
        assert!(!issue_matches_locator(&issue, "acme/repo#43"));
    }

    #[test]
    fn parses_backing_issue_reference() {
        assert_eq!(
            parse_github_issue_reference("acme/repo#42"),
            Some(("acme/repo", 42))
        );
        assert_eq!(
            parse_github_issue_reference("acme/projects/1#draft-42"),
            None
        );
    }

    #[test]
    fn normalizes_issue_draft_and_ignored_project_items() {
        let config = match resolve_service_config(
            &WorkflowDefinition {
                config: serde_yaml::from_str(
                    r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
runner:
  kind: codex
"#,
                )
                .unwrap(),
                prompt_template: String::new(),
            },
            Path::new("/tmp/WORKFLOW.md"),
        )
        .unwrap()
        .tracker
        {
            TrackerConfig::GitHubProject(config) => config,
            other => panic!("expected github tracker, got {other:?}"),
        };

        let issue = normalize_project_item(
            project_item_issue(
                "PVTI_open",
                Some(ProjectFieldValue::ProjectV2ItemFieldSingleSelectValue {
                    name: Some("In Progress".to_string()),
                }),
                Some(ProjectFieldValue::ProjectV2ItemFieldTextValue {
                    text: Some("P1".to_string()),
                }),
                42,
                false,
            ),
            "https://github.com/orgs/acme/projects/12",
            &config,
        )
        .unwrap();
        assert_eq!(issue.identifier, "acme/repo#42");
        assert_eq!(issue.state, "In Progress");
        assert_eq!(issue.priority, Some(1));
        assert_eq!(issue.labels, vec!["bug"]);
        assert!(issue.source_data.is_some());

        let closed = normalize_project_item(
            project_item_issue("PVTI_closed", None, None, 43, true),
            "https://github.com/orgs/acme/projects/12",
            &config,
        )
        .unwrap();
        assert_eq!(closed.state, "Done");

        let draft = normalize_project_item(
            project_item_draft("PVTI_draft_abc123"),
            "https://github.com/orgs/acme/projects/12",
            &config,
        )
        .unwrap();
        assert_eq!(draft.identifier, "acme/projects/12#draft-abc123");
        assert_eq!(draft.state, "Todo");
        assert_eq!(
            draft.url.as_deref(),
            Some("https://github.com/orgs/acme/projects/12")
        );

        let pull_request = ProjectItemNode {
            id: "PVTI_pr".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-02T00:00:00Z".to_string(),
            status: None,
            priority: None,
            content: Some(ProjectItemContent::PullRequest {}),
        };
        assert!(
            normalize_project_item(
                pull_request,
                "https://github.com/orgs/acme/projects/12",
                &config
            )
            .is_none()
        );
    }

    #[test]
    fn parses_priority_field_variants() {
        assert_eq!(parse_priority_string("P0 urgent"), Some(0));
        assert_eq!(parse_priority_string("critical"), Some(1));
        assert_eq!(parse_priority_string("High"), Some(2));
        assert_eq!(parse_priority_string("medium"), Some(3));
        assert_eq!(parse_priority_string("low"), Some(4));
        assert_eq!(parse_priority_string("none"), None);
        assert_eq!(
            ProjectFieldValue::ProjectV2ItemFieldNumberValue { number: Some(2.6) }.as_priority(),
            Some(3)
        );
    }

    #[test]
    fn parses_gh_json_output_success_failure_and_invalid_json() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        #[cfg(unix)]
        {
            let success = std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: br#"{"ok":true}"#.to_vec(),
                stderr: Vec::new(),
            };
            let parsed: serde_json::Value = parse_gh_json_output("gh test", success).unwrap();
            assert_eq!(parsed["ok"], true);

            let failure = std::process::Output {
                status: std::process::ExitStatus::from_raw(1 << 8),
                stdout: Vec::new(),
                stderr: b"permission denied".to_vec(),
            };
            let err = parse_gh_json_output::<serde_json::Value>("gh test", failure).unwrap_err();
            assert!(err.to_string().contains("gh test failed"));
            assert!(err.to_string().contains("permission denied"));

            let invalid = std::process::Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: b"not json".to_vec(),
                stderr: Vec::new(),
            };
            assert!(parse_gh_json_output::<serde_json::Value>("gh test", invalid).is_err());
        }
    }

    #[tokio::test]
    async fn fetch_candidate_issues_filters_active_items_across_pages() {
        let fake = fake_gh(
            r#"
log_args "$@"
STATE_FILE="$(dirname "$CALL_LOG")/count"
COUNT=0
if [[ -f "$STATE_FILE" ]]; then
  COUNT="$(cat "$STATE_FILE")"
fi
NEXT=$((COUNT + 1))
printf '%s' "$NEXT" > "$STATE_FILE"
if [[ "$COUNT" == "0" ]]; then
cat <<'JSON'
{"data":{"repositoryOwner":{"projectV2":{"url":"https://github.com/orgs/acme/projects/12","items":{"pageInfo":{"hasNextPage":true,"endCursor":"cursor-1"},"nodes":[{"id":"PVTI_1","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","statusFieldValue":{"__typename":"ProjectV2ItemFieldSingleSelectValue","name":"Todo"},"priorityFieldValue":{"__typename":"ProjectV2ItemFieldNumberValue","number":2},"content":{"__typename":"Issue","id":"I_1","number":42,"title":"Open issue","body":"Body","url":"https://github.com/acme/repo/issues/42","state":"OPEN","closed":false,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","repository":{"nameWithOwner":"acme/repo"},"labels":{"nodes":[{"name":"Bug"}]}}},{"id":"PVTI_2","createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","statusFieldValue":{"__typename":"ProjectV2ItemFieldSingleSelectValue","name":"Backlog"},"priorityFieldValue":null,"content":{"__typename":"Issue","id":"I_2","number":43,"title":"Backlog issue","body":null,"url":"https://github.com/acme/repo/issues/43","state":"OPEN","closed":false,"createdAt":"2026-01-01T00:00:00Z","updatedAt":"2026-01-02T00:00:00Z","repository":{"nameWithOwner":"acme/repo"},"labels":{"nodes":[]}}}]}}}}}
JSON
else
cat <<'JSON'
{"data":{"repositoryOwner":{"projectV2":{"url":"https://github.com/orgs/acme/projects/12","items":{"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{"id":"PVTI_draft_xyz","createdAt":"2026-01-03T00:00:00Z","updatedAt":"2026-01-04T00:00:00Z","statusFieldValue":{"__typename":"ProjectV2ItemFieldSingleSelectValue","name":"In Progress"},"priorityFieldValue":{"__typename":"ProjectV2ItemFieldTextValue","text":"High"},"content":{"__typename":"DraftIssue","id":"D_1","title":"Draft task","body":"Draft body","createdAt":"2026-01-03T00:00:00Z","updatedAt":"2026-01-04T00:00:00Z"}}]}}}}}
JSON
fi
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        let candidates = tracker.fetch_candidate_issues().await.unwrap();

        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].identifier, "acme/repo#42");
        assert_eq!(candidates[1].identifier, "acme/projects/12#draft-xyz");
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.contains("cursor=cursor-1"));
    }

    #[tokio::test]
    async fn fetch_candidate_issues_errors_when_paginated_response_has_no_cursor() {
        let fake = fake_gh(
            r#"
cat <<'JSON'
{"data":{"repositoryOwner":{"projectV2":{"url":"https://github.com/orgs/acme/projects/12","items":{"pageInfo":{"hasNextPage":true,"endCursor":null},"nodes":[]}}}}}
JSON
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        let err = tracker.fetch_candidate_issues().await.unwrap_err();

        assert!(
            err.to_string()
                .contains("github_project_missing_end_cursor")
        );
    }

    #[tokio::test]
    async fn fetch_comments_parses_backing_issue_comments_and_skips_drafts() {
        let fake = fake_gh(
            r#"
log_args "$@"
cat <<'JSON'
[{"node_id":"IC_kw1","body":"please update docs","created_at":"2026-01-01T00:00:00Z"}]
JSON
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        let comments = tracker
            .fetch_comments(&issue("acme/repo#42"))
            .await
            .unwrap();
        let draft_comments = tracker
            .fetch_comments(&issue("acme/projects/12#draft-xyz"))
            .await
            .unwrap();

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "please update docs");
        assert!(draft_comments.is_empty());
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert!(calls.contains("repos/acme/repo/issues/42/comments"));
        assert_eq!(calls.lines().count(), 1);
    }

    #[tokio::test]
    async fn create_comment_posts_to_backing_issue_and_rejects_draft() {
        let fake = fake_gh(
            r#"
log_args "$@"
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        tracker
            .create_comment(&issue("acme/repo#42"), "hello from luna")
            .await
            .unwrap();
        let err = tracker
            .create_comment(&issue("acme/projects/12#draft-xyz"), "draft")
            .await
            .unwrap_err();

        assert!(err.to_string().contains("requires a backing GitHub issue"));
        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert!(calls.contains("issue comment 42"));
        assert!(calls.contains("-R acme/repo"));
        assert!(calls.contains("--body"));
        assert!(calls.contains("hello\\ from\\ luna"));
    }

    #[tokio::test]
    async fn update_issue_state_resolves_status_field_and_updates_project_item() {
        let fake = fake_gh(
            r#"
log_args "$@"
case "$*" in
  *ProjectStatusField*)
cat <<'JSON'
{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_1","fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","id":"PVTSSF_status","name":"Status","options":[{"id":"todo-id","name":"Todo"},{"id":"done-id","name":"Done"}]}]}}}}}
JSON
    ;;
  *UpdateProjectItemStatus*)
cat <<'JSON'
{"data":{"updateProjectV2ItemFieldValue":{"projectV2Item":{"id":"PVTI_1"}}}}
JSON
    ;;
  *)
    echo "unexpected query" >&2
    exit 1
    ;;
esac
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        tracker.update_issue_state("PVTI_1", "done").await.unwrap();

        let calls = fs::read_to_string(fake.log_path).unwrap();
        assert_eq!(calls.lines().count(), 2);
        assert!(calls.contains("optionId=done-id"));
        assert!(calls.contains("itemId=PVTI_1"));
    }

    #[tokio::test]
    async fn update_issue_state_reports_missing_status_option() {
        let fake = fake_gh(
            r#"
cat <<'JSON'
{"data":{"repositoryOwner":{"projectV2":{"id":"PVT_1","fields":{"nodes":[{"__typename":"ProjectV2SingleSelectField","id":"PVTSSF_status","name":"Status","options":[{"id":"todo-id","name":"Todo"}]}]}}}}}
JSON
"#,
        );
        let tracker = tracker_with_gh(&fake.command);

        let err = tracker
            .update_issue_state("PVTI_1", "Done")
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains("github_project_state_option_not_found")
        );
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectItemsResponse {
    data: ProjectItemsQueryData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectItemsQueryData {
    repository_owner: Option<RepositoryOwnerNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryOwnerNode {
    #[serde(default)]
    project_v2: Option<ProjectNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectNode {
    url: String,
    items: ProjectItemConnection,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ProjectFieldConnection {
    #[serde(default)]
    nodes: Vec<ProjectFieldConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectItemConnection {
    nodes: Vec<ProjectItemNode>,
    page_info: PageInfo,
}

#[derive(Debug, Deserialize)]
struct ProjectStatusFieldResponse {
    data: ProjectStatusFieldQueryData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStatusFieldQueryData {
    repository_owner: Option<StatusRepositoryOwnerNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusRepositoryOwnerNode {
    #[serde(default)]
    project_v2: Option<ProjectStatusNode>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStatusNode {
    id: String,
    fields: ProjectFieldConnection,
}

#[derive(Debug, Deserialize)]
struct UpdateProjectItemStatusResponse {
    data: UpdateProjectItemStatusData,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProjectItemStatusData {
    #[serde(default)]
    update_project_v2_item_field_value: Option<UpdateProjectItemStatusPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateProjectItemStatusPayload {
    #[serde(default)]
    project_v2_item: Option<UpdatedProjectItem>,
}

#[derive(Debug, Deserialize)]
struct UpdatedProjectItem {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectItemNode {
    id: String,
    created_at: String,
    updated_at: String,
    #[serde(default, rename = "statusFieldValue")]
    status: Option<ProjectFieldValue>,
    #[serde(default, rename = "priorityFieldValue")]
    priority: Option<ProjectFieldValue>,
    content: Option<ProjectItemContent>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "__typename")]
enum ProjectFieldValue {
    ProjectV2ItemFieldSingleSelectValue { name: Option<String> },
    ProjectV2ItemFieldTextValue { text: Option<String> },
    ProjectV2ItemFieldNumberValue { number: Option<f64> },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
enum ProjectFieldConfig {
    ProjectV2SingleSelectField {
        id: String,
        name: String,
        options: Vec<ProjectFieldOption>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ProjectFieldOption {
    id: String,
    name: String,
}

impl ProjectFieldValue {
    fn as_state_name(&self) -> Option<String> {
        match self {
            Self::ProjectV2ItemFieldSingleSelectValue { name } => name.clone(),
            Self::ProjectV2ItemFieldTextValue { text } => text.clone(),
            Self::ProjectV2ItemFieldNumberValue { number } => number.map(|value| value.to_string()),
        }
    }

    fn as_priority(&self) -> Option<i64> {
        match self {
            Self::ProjectV2ItemFieldNumberValue { number } => {
                number.map(|value| value.round() as i64)
            }
            Self::ProjectV2ItemFieldSingleSelectValue { name } => {
                name.as_deref().and_then(parse_priority_string)
            }
            Self::ProjectV2ItemFieldTextValue { text } => {
                text.as_deref().and_then(parse_priority_string)
            }
        }
    }
}

fn parse_priority_string(value: &str) -> Option<i64> {
    let lowered = value.trim().to_lowercase();
    let digits = lowered
        .chars()
        .skip_while(|ch| !ch.is_ascii_digit())
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if !digits.is_empty() {
        return digits.parse::<i64>().ok();
    }

    match lowered.as_str() {
        "critical" | "urgent" => Some(1),
        "high" => Some(2),
        "medium" => Some(3),
        "low" => Some(4),
        _ => None,
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "__typename")]
enum ProjectItemContent {
    Issue(ProjectIssueContent),
    DraftIssue(ProjectDraftIssueContent),
    PullRequest {},
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIssueContent {
    number: i64,
    title: String,
    body: Option<String>,
    url: String,
    state: String,
    closed: bool,
    created_at: String,
    updated_at: String,
    repository: ProjectIssueRepository,
    labels: ProjectIssueLabels,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIssueRepository {
    name_with_owner: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectIssueLabels {
    nodes: Vec<ProjectIssueLabelNode>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProjectIssueLabelNode {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectDraftIssueContent {
    title: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
}
