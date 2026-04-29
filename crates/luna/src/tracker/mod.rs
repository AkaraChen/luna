use async_trait::async_trait;

use crate::{
    config::TrackerConfig,
    error::Result,
    model::Issue,
};

mod github_project;
mod linear;

pub use github_project::GitHubProjectTracker;
pub use linear::LinearTracker;

#[async_trait]
pub trait Tracker: Send + Sync {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>>;
    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>>;
    async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>>;
    async fn create_comment(&self, issue_id: &str, body: &str) -> Result<()>;
    async fn update_issue_state(&self, issue_id: &str, state_name: &str) -> Result<()>;
}

pub fn build_tracker(config: &TrackerConfig) -> Result<Box<dyn Tracker>> {
    match config {
        TrackerConfig::GitHubProject(project) => {
            Ok(Box::new(GitHubProjectTracker::new(project.clone())))
        }
        TrackerConfig::Linear(linear) => {
            Ok(Box::new(LinearTracker::new(linear.clone())))
        }
    }
}
