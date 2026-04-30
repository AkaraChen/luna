use std::{collections::HashSet, process::Output};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
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
    use crate::model::Issue;

    use super::{issue_matches_locator, parse_github_issue_reference};

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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(tag = "__typename")]
enum ProjectItemContent {
    Issue(ProjectIssueContent),
    DraftIssue(ProjectDraftIssueContent),
    PullRequest {},
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectIssueRepository {
    name_with_owner: String,
}

#[derive(Debug, Deserialize)]
struct ProjectIssueLabels {
    nodes: Vec<ProjectIssueLabelNode>,
}

#[derive(Debug, Deserialize)]
struct ProjectIssueLabelNode {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectDraftIssueContent {
    title: String,
    body: Option<String>,
    created_at: String,
    updated_at: String,
}
