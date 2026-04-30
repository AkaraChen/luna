use std::{
    collections::{HashMap, HashSet},
    sync::RwLock,
};

use chrono::Utc;
use thiserror::Error;
use uuid::Uuid;

use crate::domain::{
    Comment, CreateIssue, Issue, IssueRecord, issue::default_team_key, issue_matches_locator,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("invalid_input: {0}")]
    InvalidInput(String),
    #[error("issue_not_found: {0}")]
    IssueNotFound(String),
    #[error("store_lock_poisoned")]
    LockPoisoned,
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Clone, Debug, Default)]
pub struct IssueFilter {
    pub project_slug: Option<String>,
    pub states: Vec<String>,
    pub ids: Vec<String>,
    pub assignee_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct IssueStore {
    inner: RwLock<StoreData>,
}

#[derive(Debug, Default)]
struct StoreData {
    issues: HashMap<String, IssueRecord>,
    comments: HashMap<String, Vec<Comment>>,
    counters: HashMap<String, u64>,
}

impl IssueStore {
    pub fn create_issue(&self, input: CreateIssueInput) -> StoreResult<Issue> {
        let mut data = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        let title = require_non_empty("title", input.title)?;
        let project_slug = input
            .project_slug
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "default".to_string());
        let team_key = input
            .team_key
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_team_key(&project_slug));
        let state = input
            .state
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Todo".to_string());
        let labels = input
            .labels
            .into_iter()
            .map(|label| label.trim().to_string())
            .filter(|label| !label.is_empty())
            .collect();
        let blocked_by_ids = resolve_issue_locators(&data.issues, &input.blocked_by)?;
        let counter = data.counters.entry(team_key.clone()).or_insert(0);
        *counter += 1;

        let record = IssueRecord::new(
            CreateIssue {
                project_slug,
                team_key,
                title,
                description: input.description,
                priority: input.priority,
                state,
                branch_name: input.branch_name,
                labels,
                blocked_by_ids,
                assignee_id: input.assignee_id,
            },
            *counter,
        );

        let id = record.issue.id.clone();
        data.issues.insert(id.clone(), record);
        let record = data
            .issues
            .get(&id)
            .ok_or_else(|| StoreError::IssueNotFound(id.clone()))?;

        Ok(record.to_issue(&data.issues))
    }

    pub fn list_issues(&self, filter: IssueFilter) -> StoreResult<Vec<Issue>> {
        let data = self.inner.read().map_err(|_| StoreError::LockPoisoned)?;
        let state_lookup = lower_lookup(&filter.states);

        if !filter.ids.is_empty() {
            return Ok(filter
                .ids
                .iter()
                .filter_map(|id| data.issues.get(id))
                .filter(|record| matches_filter(record, &filter, &state_lookup))
                .map(|record| record.to_issue(&data.issues))
                .collect());
        }

        let mut issues = data
            .issues
            .values()
            .filter(|record| matches_filter(record, &filter, &state_lookup))
            .map(|record| record.to_issue(&data.issues))
            .collect::<Vec<_>>();
        issues.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.identifier.cmp(&right.identifier))
        });
        Ok(issues)
    }

    pub fn find_issue(&self, locator: &str) -> StoreResult<Option<Issue>> {
        let data = self.inner.read().map_err(|_| StoreError::LockPoisoned)?;
        Ok(data
            .issues
            .values()
            .find(|record| issue_matches_locator(&record.issue, locator))
            .map(|record| record.to_issue(&data.issues)))
    }

    pub fn update_issue_state(&self, locator: &str, state: String) -> StoreResult<Issue> {
        let state = require_non_empty("state", Some(state))?;
        let mut data = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        let id = find_issue_id(&data.issues, locator)?;
        {
            let record = data
                .issues
                .get_mut(&id)
                .ok_or_else(|| StoreError::IssueNotFound(locator.to_string()))?;
            record.update_state(state);
        }
        let record = data
            .issues
            .get(&id)
            .ok_or_else(|| StoreError::IssueNotFound(locator.to_string()))?;
        Ok(record.to_issue(&data.issues))
    }

    pub fn create_comment(&self, locator: &str, body: String) -> StoreResult<Comment> {
        let body = require_non_empty("body", Some(body))?;
        let mut data = self.inner.write().map_err(|_| StoreError::LockPoisoned)?;
        let issue_id = find_issue_id(&data.issues, locator)?;
        let comment = Comment {
            id: Uuid::new_v4().to_string(),
            issue_id: issue_id.clone(),
            body,
            created_at: Utc::now(),
        };
        data.comments
            .entry(issue_id)
            .or_default()
            .push(comment.clone());
        Ok(comment)
    }

    pub fn list_comments(&self, locator: &str) -> StoreResult<Vec<Comment>> {
        let data = self.inner.read().map_err(|_| StoreError::LockPoisoned)?;
        let issue_id = find_issue_id(&data.issues, locator)?;
        Ok(data.comments.get(&issue_id).cloned().unwrap_or_default())
    }
}

#[derive(Clone, Debug, Default)]
pub struct CreateIssueInput {
    pub project_slug: Option<String>,
    pub team_key: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: Option<String>,
    pub branch_name: Option<String>,
    pub labels: Vec<String>,
    pub blocked_by: Vec<String>,
    pub assignee_id: Option<String>,
}

fn matches_filter(
    record: &IssueRecord,
    filter: &IssueFilter,
    state_lookup: &HashSet<String>,
) -> bool {
    if let Some(project_slug) = filter.project_slug.as_deref()
        && !record.project_slug.eq_ignore_ascii_case(project_slug)
    {
        return false;
    }

    if let Some(assignee_id) = filter.assignee_id.as_deref()
        && record.assignee_id.as_deref() != Some(assignee_id)
    {
        return false;
    }

    state_lookup.is_empty() || state_lookup.contains(&record.issue.state.to_lowercase())
}

fn lower_lookup(values: &[String]) -> HashSet<String> {
    values
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn require_non_empty(name: &str, value: Option<String>) -> StoreResult<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| StoreError::InvalidInput(format!("{name} is required")))
}

fn resolve_issue_locators(
    records: &HashMap<String, IssueRecord>,
    locators: &[String],
) -> StoreResult<Vec<String>> {
    locators
        .iter()
        .map(|locator| find_issue_id(records, locator))
        .collect()
}

fn find_issue_id(records: &HashMap<String, IssueRecord>, locator: &str) -> StoreResult<String> {
    let locator = locator.trim();
    records
        .values()
        .find(|record| issue_matches_locator(&record.issue, locator))
        .map(|record| record.issue.id.clone())
        .ok_or_else(|| StoreError::IssueNotFound(locator.to_string()))
}
