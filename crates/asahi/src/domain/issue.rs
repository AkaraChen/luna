use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockerRef {
    pub id: Option<String>,
    pub identifier: Option<String>,
    pub state: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Issue {
    pub id: String,
    pub identifier: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: String,
    pub branch_name: Option<String>,
    pub url: Option<String>,
    pub labels: Vec<String>,
    pub blocked_by: Vec<BlockerRef>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct CreateIssue {
    pub project_slug: String,
    pub team_key: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: String,
    pub branch_name: Option<String>,
    pub labels: Vec<String>,
    pub blocked_by_ids: Vec<String>,
    pub assignee_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IssueRecord {
    pub issue: Issue,
    pub project_slug: String,
    pub assignee_id: Option<String>,
    pub blocked_by_ids: Vec<String>,
}

impl IssueRecord {
    pub fn new(input: CreateIssue, number: u64) -> Self {
        let now = Utc::now();
        let identifier = format!("{}-{number}", input.team_key);
        let id = Uuid::new_v4().to_string();
        let url = Some(format!("/api/issues/{}", url_safe_identifier(&identifier)));

        Self {
            issue: Issue {
                id,
                identifier,
                title: input.title,
                description: input.description,
                priority: input.priority,
                state: input.state,
                branch_name: input.branch_name,
                url,
                labels: input.labels,
                blocked_by: Vec::new(),
                created_at: Some(now),
                updated_at: Some(now),
            },
            project_slug: input.project_slug,
            assignee_id: input.assignee_id,
            blocked_by_ids: input.blocked_by_ids,
        }
    }

    pub fn to_issue(&self, records: &HashMap<String, IssueRecord>) -> Issue {
        let mut issue = self.issue.clone();
        issue.blocked_by = self
            .blocked_by_ids
            .iter()
            .filter_map(|id| records.get(id))
            .map(|record| BlockerRef {
                id: Some(record.issue.id.clone()),
                identifier: Some(record.issue.identifier.clone()),
                state: Some(record.issue.state.clone()),
            })
            .collect();
        issue
    }

    pub fn update_state(&mut self, state: String) {
        self.issue.state = state;
        self.issue.updated_at = Some(Utc::now());
    }
}

pub fn issue_matches_locator(issue: &Issue, locator: &str) -> bool {
    issue.id == locator
        || issue.identifier.eq_ignore_ascii_case(locator)
        || sanitize_workspace_key(&issue.identifier).eq_ignore_ascii_case(locator)
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

pub fn default_team_key(project_slug: &str) -> String {
    let candidate: String = project_slug
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .to_ascii_uppercase();

    if candidate.is_empty() {
        "ASH".to_string()
    } else {
        candidate
    }
}

fn url_safe_identifier(identifier: &str) -> String {
    identifier.replace(' ', "%20")
}
