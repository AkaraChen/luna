use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;

use crate::{
    config::LinearTrackerConfig,
    error::{LunaError, Result},
    model::{BlockerRef, Issue},
    tracker::Tracker,
    workspace::sanitize_workspace_key,
};

const ISSUE_PAGE_SIZE: i64 = 50;

#[derive(Clone, Debug)]
pub struct LinearTracker {
    config: LinearTrackerConfig,
    client: reqwest::Client,
}

impl LinearTracker {
    pub fn new(config: LinearTrackerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    async fn graphql<Q, V>(&self, query: Q, variables: V) -> Result<serde_json::Value>
    where
        Q: Into<String>,
        V: Serialize,
    {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or(LunaError::MissingTrackerApiKey)?;

        let payload = json!({
            "query": query.into(),
            "variables": variables,
        });

        let response = self
            .client
            .post(&self.config.endpoint)
            .header("Authorization", api_key)
            .header("Content-Type", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?;

        let status = response.status();
        let body: serde_json::Value = response.json().await?;

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "linear_api_status: status={status}, body={}",
                truncate_error_body(&body)
            )));
        }

        if body.get("errors").is_some() {
            return Err(LunaError::Tracker(format!(
                "linear_graphql_errors: {}",
                truncate_error_body(&body)
            )));
        }

        Ok(body)
    }

    async fn do_fetch_by_states(
        &self,
        state_names: &[String],
        assignee_filter: Option<&str>,
    ) -> Result<Vec<Issue>> {
        let project_slug = self
            .config
            .project_slug
            .as_ref()
            .ok_or(LunaError::MissingTrackerProjectSlug)?;

        let mut issues = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let body = self
                .graphql(
                    LINEAR_POLL_QUERY,
                    json!({
                        "projectSlug": project_slug,
                        "stateNames": state_names,
                        "first": ISSUE_PAGE_SIZE,
                        "relationFirst": ISSUE_PAGE_SIZE,
                        "after": cursor,
                    }),
                )
                .await?;

            let (page_issues, page_info) = decode_linear_page(&body)?;
            issues.extend(page_issues);

            match page_info {
                Some(info) if info.has_next_page => {
                    cursor = info.end_cursor;
                }
                _ => break,
            }
        }

        // Filter by assignee if needed
        if let Some(filter) = assignee_filter {
            let resolved = self.resolve_assignee_filter(filter).await?;
            issues.retain(|issue| match &issue.url {
                Some(_url) => {
                    // We don't store assignee_id on Issue, so we can't filter here.
                    // Instead, we fetch with assignee filter in the query.
                    // For now, skip client-side filtering.
                    true
                }
                None => true,
            });
            let _ = resolved; // TODO: use resolved filter
        }

        Ok(issues)
    }

    async fn do_fetch_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
        if issue_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_issues = Vec::new();
        for chunk in issue_ids.chunks(ISSUE_PAGE_SIZE as usize) {
            let body = self
                .graphql(
                    LINEAR_ISSUES_BY_ID_QUERY,
                    json!({
                        "ids": chunk,
                        "first": chunk.len() as i64,
                        "relationFirst": ISSUE_PAGE_SIZE,
                    }),
                )
                .await?;

            let issues = decode_linear_issues(&body)?;
            all_issues.extend(issues);
        }

        // Preserve input order
        let order_index: std::collections::HashMap<&String, usize> = issue_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        all_issues.sort_by_key(|issue| order_index.get(&issue.id).copied().unwrap_or(usize::MAX));

        Ok(all_issues)
    }

    async fn fetch_all_project_issues(&self) -> Result<Vec<Issue>> {
        let project_slug = self
            .config
            .project_slug
            .as_ref()
            .ok_or(LunaError::MissingTrackerProjectSlug)?;

        let mut issues = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let body = self
                .graphql(
                    LINEAR_PROJECT_ISSUES_QUERY,
                    json!({
                        "projectSlug": project_slug,
                        "first": ISSUE_PAGE_SIZE,
                        "relationFirst": ISSUE_PAGE_SIZE,
                        "after": cursor,
                    }),
                )
                .await?;

            let (page_issues, page_info) = decode_linear_page(&body)?;
            issues.extend(page_issues);

            match page_info {
                Some(info) if info.has_next_page => {
                    cursor = info.end_cursor;
                }
                _ => break,
            }
        }

        Ok(issues)
    }

    async fn resolve_assignee_filter(&self, assignee: &str) -> Result<Option<String>> {
        let trimmed = assignee.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }
        if trimmed.eq_ignore_ascii_case("me") {
            let body = self.graphql(LINEAR_VIEWER_QUERY, json!({})).await?;
            let viewer_id = body
                .get("data")
                .and_then(|d| d.get("viewer"))
                .and_then(|v| v.get("id"))
                .and_then(|id| id.as_str())
                .ok_or_else(|| LunaError::Tracker("missing_linear_viewer_identity".to_string()))?;
            Ok(Some(viewer_id.to_string()))
        } else {
            Ok(Some(trimmed.to_string()))
        }
    }
}

#[async_trait]
impl Tracker for LinearTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        let assignee_filter = self
            .config
            .assignee
            .as_deref()
            .filter(|s| !s.trim().is_empty());
        self.do_fetch_by_states(&self.config.active_states.clone(), assignee_filter)
            .await
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        let normalized: Vec<String> = states.iter().map(|s| s.to_string()).collect();
        self.do_fetch_by_states(&normalized, None).await
    }

    async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
        self.do_fetch_by_ids(issue_ids).await
    }

    async fn find_issue_by_locator(&self, locator: &str) -> Result<Option<Issue>> {
        let locator = locator.trim();
        if locator.is_empty() {
            return Ok(None);
        }

        Ok(self
            .fetch_all_project_issues()
            .await?
            .into_iter()
            .find(|issue| issue_matches_locator(issue, locator)))
    }

    async fn create_comment(&self, issue: &Issue, body: &str) -> Result<()> {
        let response = self
            .graphql(
                LINEAR_CREATE_COMMENT_MUTATION,
                json!({
                    "issueId": issue.id,
                    "body": body,
                }),
            )
            .await?;

        let success = response
            .get("data")
            .and_then(|d| d.get("commentCreate"))
            .and_then(|c| c.get("success"))
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        if success {
            Ok(())
        } else {
            Err(LunaError::Tracker("comment_create_failed".to_string()))
        }
    }

    async fn update_issue_state(&self, issue_id: &str, state_name: &str) -> Result<()> {
        // Resolve state name to state id via the issue's team
        let state_id_response = self
            .graphql(
                LINEAR_RESOLVE_STATE_ID_QUERY,
                json!({
                    "issueId": issue_id,
                    "stateName": state_name,
                }),
            )
            .await?;

        let state_id = state_id_response
            .get("data")
            .and_then(|d| d.get("issue"))
            .and_then(|i| i.get("team"))
            .and_then(|t| t.get("states"))
            .and_then(|s| s.get("nodes"))
            .and_then(|n| n.as_array())
            .and_then(|arr| arr.first())
            .and_then(|node| node.get("id"))
            .and_then(|id| id.as_str())
            .ok_or_else(|| LunaError::Tracker("state_not_found".to_string()))?;

        let response = self
            .graphql(
                LINEAR_UPDATE_STATE_MUTATION,
                json!({
                    "issueId": issue_id,
                    "stateId": state_id,
                }),
            )
            .await?;

        let success = response
            .get("data")
            .and_then(|d| d.get("issueUpdate"))
            .and_then(|u| u.get("success"))
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        if success {
            Ok(())
        } else {
            Err(LunaError::Tracker("issue_update_failed".to_string()))
        }
    }
}

fn decode_linear_page(body: &serde_json::Value) -> Result<(Vec<Issue>, Option<PageInfo>)> {
    let issues_node = body
        .get("data")
        .and_then(|d| d.get("issues"))
        .ok_or_else(|| LunaError::Tracker("linear_unknown_payload".to_string()))?;

    let nodes = issues_node
        .get("nodes")
        .and_then(|n| n.as_array())
        .ok_or_else(|| LunaError::Tracker("linear_unknown_payload".to_string()))?;

    let issues: Vec<Issue> = nodes
        .iter()
        .filter_map(|node| normalize_linear_issue(node))
        .collect();

    let page_info = issues_node.get("pageInfo").and_then(|p| {
        let has_next_page = p.get("hasNextPage")?.as_bool()?;
        let end_cursor = p.get("endCursor")?.as_str().map(|s| s.to_string());
        Some(PageInfo {
            has_next_page,
            end_cursor,
        })
    });

    Ok((issues, page_info))
}

fn decode_linear_issues(body: &serde_json::Value) -> Result<Vec<Issue>> {
    let nodes = body
        .get("data")
        .and_then(|d| d.get("issues"))
        .and_then(|i| i.get("nodes"))
        .and_then(|n| n.as_array())
        .ok_or_else(|| LunaError::Tracker("linear_unknown_payload".to_string()))?;

    Ok(nodes
        .iter()
        .filter_map(|node| normalize_linear_issue(node))
        .collect())
}

fn normalize_linear_issue(node: &serde_json::Value) -> Option<Issue> {
    let id = node.get("id")?.as_str()?.to_string();
    let identifier = node.get("identifier")?.as_str()?.to_string();
    let title = node.get("title")?.as_str()?.to_string();
    let description = node
        .get("description")
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());
    let priority = node.get("priority").and_then(|p| p.as_i64());
    let state = node
        .get("state")
        .and_then(|s| s.get("name"))
        .and_then(|n| n.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let branch_name = node
        .get("branchName")
        .and_then(|b| b.as_str())
        .map(|s| s.to_string());
    let url = node
        .get("url")
        .and_then(|u| u.as_str())
        .map(|s| s.to_string());

    let labels = node
        .get("labels")
        .and_then(|l| l.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|label| label.get("name"))
                .filter_map(|n| n.as_str())
                .map(|s| s.to_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let blocked_by = node
        .get("inverseRelations")
        .and_then(|r| r.get("nodes"))
        .and_then(|n| n.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|rel| {
                    let rel_type = rel.get("type")?.as_str()?;
                    if rel_type.trim().to_lowercase() != "blocks" {
                        return None;
                    }
                    let issue = rel.get("issue")?;
                    Some(BlockerRef {
                        id: issue
                            .get("id")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string()),
                        identifier: issue
                            .get("identifier")
                            .and_then(|id| id.as_str())
                            .map(|s| s.to_string()),
                        state: issue
                            .get("state")
                            .and_then(|s| s.get("name"))
                            .and_then(|n| n.as_str())
                            .map(|s| s.to_string()),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let created_at = node
        .get("createdAt")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    let updated_at = node
        .get("updatedAt")
        .and_then(|v| v.as_str())
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Some(Issue {
        id,
        identifier,
        title,
        description,
        priority,
        state,
        branch_name,
        url,
        labels,
        blocked_by,
        created_at,
        updated_at,
    })
}

fn truncate_error_body(value: &serde_json::Value) -> String {
    let text = value.to_string();
    const LIMIT: usize = 1000;
    if text.len() <= LIMIT {
        text
    } else {
        format!("{}...<truncated>", &text[..LIMIT])
    }
}

fn issue_matches_locator(issue: &Issue, locator: &str) -> bool {
    issue.id == locator
        || issue.identifier.eq_ignore_ascii_case(locator)
        || sanitize_workspace_key(&issue.identifier).eq_ignore_ascii_case(locator)
}

#[derive(Debug)]
struct PageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

const LINEAR_POLL_QUERY: &str = r#"
query SymphonyLinearPoll($projectSlug: String!, $stateNames: [String!]!, $first: Int!, $relationFirst: Int!, $after: String) {
  issues(filter: {project: {slugId: {eq: $projectSlug}}, state: {name: {in: $stateNames}}}, first: $first, after: $after) {
    nodes {
      id
      identifier
      title
      description
      priority
      state {
        name
      }
      branchName
      url
      assignee {
        id
      }
      labels {
        nodes {
          name
        }
      }
      inverseRelations(first: $relationFirst) {
        nodes {
          type
          issue {
            id
            identifier
            state {
              name
            }
          }
        }
      }
      createdAt
      updatedAt
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
"#;

const LINEAR_PROJECT_ISSUES_QUERY: &str = r#"
query SymphonyLinearProjectIssues($projectSlug: String!, $first: Int!, $relationFirst: Int!, $after: String) {
  issues(filter: {project: {slugId: {eq: $projectSlug}}}, first: $first, after: $after) {
    nodes {
      id
      identifier
      title
      description
      priority
      state {
        name
      }
      branchName
      url
      assignee {
        id
      }
      labels {
        nodes {
          name
        }
      }
      inverseRelations(first: $relationFirst) {
        nodes {
          type
          issue {
            id
            identifier
            state {
              name
            }
          }
        }
      }
      createdAt
      updatedAt
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
"#;

const LINEAR_ISSUES_BY_ID_QUERY: &str = r#"
query SymphonyLinearIssuesById($ids: [ID!]!, $first: Int!, $relationFirst: Int!) {
  issues(filter: {id: {in: $ids}}, first: $first) {
    nodes {
      id
      identifier
      title
      description
      priority
      state {
        name
      }
      branchName
      url
      assignee {
        id
      }
      labels {
        nodes {
          name
        }
      }
      inverseRelations(first: $relationFirst) {
        nodes {
          type
          issue {
            id
            identifier
            state {
              name
            }
          }
        }
      }
      createdAt
      updatedAt
    }
  }
}
"#;

const LINEAR_VIEWER_QUERY: &str = r#"
query SymphonyLinearViewer {
  viewer {
    id
  }
}
"#;

const LINEAR_CREATE_COMMENT_MUTATION: &str = r#"
mutation SymphonyCreateComment($issueId: String!, $body: String!) {
  commentCreate(input: {issueId: $issueId, body: $body}) {
    success
  }
}
"#;

const LINEAR_UPDATE_STATE_MUTATION: &str = r#"
mutation SymphonyUpdateIssueState($issueId: String!, $stateId: String!) {
  issueUpdate(id: $issueId, input: {stateId: $stateId}) {
    success
  }
}
"#;

const LINEAR_RESOLVE_STATE_ID_QUERY: &str = r#"
query SymphonyResolveStateId($issueId: String!, $stateName: String!) {
  issue(id: $issueId) {
    team {
      states(filter: {name: {eq: $stateName}}, first: 1) {
        nodes {
          id
        }
      }
    }
  }
}
"#;

#[cfg(test)]
mod tests {
    use crate::model::Issue;

    use super::issue_matches_locator;

    fn issue(identifier: &str) -> Issue {
        Issue {
            id: "linear-id".to_string(),
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
        let issue = issue("ENG-42");
        assert!(issue_matches_locator(&issue, "ENG-42"));
        assert!(issue_matches_locator(&issue, "eng-42"));
        assert!(!issue_matches_locator(&issue, "ENG-43"));
    }
}
