use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::project::ProjectRef;

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
    pub project_id: Option<String>,
    pub project: Option<ProjectRef>,
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
