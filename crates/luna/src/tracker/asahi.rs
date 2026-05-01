use async_trait::async_trait;
use reqwest::Url;
use serde::{Deserialize, Serialize};

use asahi::domain::WikiNode;

use crate::{
    config::AsahiTrackerConfig,
    error::{LunaError, Result},
    model::Issue,
    tracker::Tracker,
};

#[derive(Clone, Debug)]
pub struct AsahiTracker {
    config: AsahiTrackerConfig,
    client: reqwest::Client,
}

impl AsahiTracker {
    pub fn new(config: AsahiTrackerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn base_url(&self) -> Result<Url> {
        Url::parse(&self.config.endpoint)
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi endpoint: {e}")))
    }

    fn issue_url(&self, locator: &str) -> Result<Url> {
        let base = self.base_url()?;
        let path = format!("api/issues/{}", locator);
        base.join(&path)
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))
    }

    async fn list_issues(
        &self,
        states: Option<&[String]>,
        ids: Option<&[String]>,
    ) -> Result<Vec<Issue>> {
        let base = self.base_url()?;
        let url = base.join("api/issues").map_err(|e| {
            LunaError::InvalidConfig(format!("invalid asahi url: {e}"))
        })?;

        let mut req = self.client.get(url);
        if let Some(states) = states {
            let joined = states.join(",");
            if !joined.is_empty() {
                req = req.query(&[("states", joined)]);
            }
        }
        if let Some(ids) = ids {
            let joined = ids.join(",");
            if !joined.is_empty() {
                req = req.query(&[("ids", joined)]);
            }
        }

        let response = req.send().await?;
        let status = response.status();
        let body: IssueListResponse = response.json().await?;

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi list_issues failed: status={status}"
            )));
        }

        Ok(body.issues)
    }

    async fn get_issue(&self, locator: &str) -> Result<Option<Issue>> {
        let url = self.issue_url(locator)?;
        let response = self.client.get(url).send().await?;
        let status = response.status();

        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi get_issue failed: status={status}"
            )));
        }

        let issue: Issue = response.json().await?;
        Ok(Some(issue))
    }

    pub async fn fetch_project_wiki(&self, project_locator: &str) -> Result<Vec<WikiNode>> {
        let base = self.base_url()?;
        let url = base
            .join(&format!("api/projects/{}/wiki", project_locator))
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;

        let response = self
            .client
            .get(url)
            .query(&[("recursive", "true")])
            .send()
            .await?;
        let status = response.status();
        let body: WikiNodeListResponse = response.json().await?;

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi fetch_project_wiki failed: status={status}"
            )));
        }

        Ok(body.nodes)
    }
}

#[async_trait]
impl Tracker for AsahiTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        if self.config.active_states.is_empty() {
            return Ok(Vec::new());
        }
        self.list_issues(Some(&self.config.active_states), None).await
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        self.list_issues(Some(states), None).await
    }

    async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
        self.list_issues(None, Some(issue_ids)).await
    }

    async fn find_issue_by_locator(&self, locator: &str) -> Result<Option<Issue>> {
        self.get_issue(locator).await
    }

    async fn create_comment(&self, issue: &Issue, body: &str) -> Result<()> {
        let url = self.base_url()?.join(&format!("api/issues/{}/comments", issue.id))
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;
        let payload = CreateCommentRequest {
            body: body.to_string(),
        };
        let response = self.client.post(url).json(&payload).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi create_comment failed: status={status}"
            )));
        }

        Ok(())
    }

    async fn update_issue_state(&self, issue_id: &str, state_name: &str) -> Result<()> {
        let url = self.base_url()?.join(&format!("api/issues/{}/state", issue_id))
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;
        let payload = UpdateStateRequest {
            state: state_name.to_string(),
        };
        let response = self.client.patch(url).json(&payload).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi update_issue_state failed: status={status}"
            )));
        }

        Ok(())
    }

    async fn create_activity(
        &self,
        issue: &Issue,
        kind: &str,
        title: &str,
        body: Option<&str>,
    ) -> Result<()> {
        let url = self.base_url()?.join(&format!("api/issues/{}/activities", issue.id))
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;
        let payload = CreateActivityRequest {
            kind: kind.to_string(),
            title: title.to_string(),
            body: body.map(|s| s.to_string()),
        };
        let response = self.client.post(url).json(&payload).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi create_activity failed: status={status}"
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct IssueListResponse {
    issues: Vec<Issue>,
}

#[derive(Debug, Deserialize)]
struct WikiNodeListResponse {
    nodes: Vec<WikiNode>,
}

#[derive(Debug, Serialize)]
struct CreateCommentRequest {
    body: String,
}

#[derive(Debug, Serialize)]
struct CreateActivityRequest {
    kind: String,
    title: String,
    body: Option<String>,
}

#[derive(Debug, Serialize)]
struct UpdateStateRequest {
    state: String,
}
