use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::project::ProjectRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IssueState {
    Backlog,
    Todo,
    #[serde(rename = "In Progress")]
    InProgress,
    Done,
}

impl IssueState {
    pub const ALL: &[IssueState] = &[
        IssueState::Backlog,
        IssueState::Todo,
        IssueState::InProgress,
        IssueState::Done,
    ];
}

impl fmt::Display for IssueState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backlog => write!(f, "Backlog"),
            Self::Todo => write!(f, "Todo"),
            Self::InProgress => write!(f, "In Progress"),
            Self::Done => write!(f, "Done"),
        }
    }
}

impl FromStr for IssueState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Backlog" => Ok(Self::Backlog),
            "Todo" => Ok(Self::Todo),
            "In Progress" => Ok(Self::InProgress),
            "Done" => Ok(Self::Done),
            _ => Err(format!("invalid issue state: {s}")),
        }
    }
}

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
