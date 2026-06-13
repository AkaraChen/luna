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
        let url = base
            .join("api/issues")
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;

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

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi list_issues failed: status={status}"
            )));
        }

        let body: IssueListResponse = response.json().await?;
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

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi fetch_project_wiki failed: status={status}"
            )));
        }

        let body: WikiNodeListResponse = response.json().await?;
        Ok(body.nodes)
    }
}

#[async_trait]
impl Tracker for AsahiTracker {
    async fn fetch_candidate_issues(&self) -> Result<Vec<Issue>> {
        let active_states = vec!["Todo".to_string(), "In Progress".to_string()];
        self.list_issues(Some(&active_states), None).await
    }

    async fn fetch_issues_by_states(&self, states: &[String]) -> Result<Vec<Issue>> {
        if states.is_empty() {
            return Ok(Vec::new());
        }
        self.list_issues(Some(states), None).await
    }

    async fn fetch_issue_states_by_ids(&self, issue_ids: &[String]) -> Result<Vec<Issue>> {
        if issue_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.list_issues(None, Some(issue_ids)).await
    }

    async fn find_issue_by_locator(&self, locator: &str) -> Result<Option<Issue>> {
        self.get_issue(locator).await
    }

    async fn fetch_comments(&self, issue: &Issue) -> Result<Vec<crate::model::Comment>> {
        let url = self
            .base_url()?
            .join(&format!("api/issues/{}/comments", issue.id))
            .map_err(|e| LunaError::InvalidConfig(format!("invalid asahi url: {e}")))?;

        let response = self.client.get(url).send().await?;
        let status = response.status();

        if !status.is_success() {
            return Err(LunaError::Tracker(format!(
                "asahi fetch_comments failed: status={status}"
            )));
        }

        let body: CommentListResponse = response.json().await?;
        Ok(body.comments)
    }

    async fn create_comment(&self, issue: &Issue, body: &str) -> Result<()> {
        let url = self
            .base_url()?
            .join(&format!("api/issues/{}/comments", issue.id))
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
        let url = self
            .base_url()?
            .join(&format!("api/issues/{}/state", issue_id))
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
        let url = self
            .base_url()?
            .join(&format!("api/issues/{}/activities", issue.id))
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

#[derive(Debug, Deserialize)]
struct CommentListResponse {
    comments: Vec<crate::model::Comment>,
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

    use crate::{
        config::AsahiTrackerConfig,
        tracker::{AsahiTracker, Tracker},
    };

    #[derive(Clone, Debug)]
    struct RecordedRequest {
        method: String,
        target: String,
        body: String,
    }

    #[derive(Clone, Debug)]
    struct MockResponse {
        status: u16,
        body: String,
    }

    impl MockResponse {
        fn json(status: u16, body: Value) -> Self {
            Self {
                status,
                body: body.to_string(),
            }
        }

        fn text(status: u16, body: &str) -> Self {
            Self {
                status,
                body: body.to_string(),
            }
        }
    }

    async fn spawn_mock_server(
        responses: Vec<MockResponse>,
    ) -> (String, Arc<Mutex<Vec<RecordedRequest>>>, JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let records = Arc::new(Mutex::new(Vec::new()));
        let server_records = Arc::clone(&records);
        let handle = tokio::spawn(async move {
            for response in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let request = read_request(&mut stream).await;
                server_records.lock().unwrap().push(request);

                let reason = match response.status {
                    200..=299 => "OK",
                    404 => "Not Found",
                    500 => "Internal Server Error",
                    _ => "Status",
                };
                let header = format!(
                    "HTTP/1.1 {} {}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    response.status,
                    reason,
                    response.body.len()
                );
                stream.write_all(header.as_bytes()).await.unwrap();
                stream.write_all(response.body.as_bytes()).await.unwrap();
            }
        });

        (endpoint, records, handle)
    }

    async fn read_request(stream: &mut tokio::net::TcpStream) -> RecordedRequest {
        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 1024];

        loop {
            let read = stream.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);

            let Some(header_end) = find_bytes(&buffer, b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            if buffer.len() >= header_end + 4 + content_length {
                break;
            }
        }

        let header_end = find_bytes(&buffer, b"\r\n\r\n").unwrap_or(buffer.len());
        let headers = String::from_utf8_lossy(&buffer[..header_end]);
        let request_line = headers.lines().next().unwrap_or_default();
        let mut parts = request_line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_string();
        let target = parts.next().unwrap_or_default().to_string();
        let body = if header_end + 4 <= buffer.len() {
            String::from_utf8_lossy(&buffer[header_end + 4..]).to_string()
        } else {
            String::new()
        };

        RecordedRequest {
            method,
            target,
            body,
        }
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn tracker(endpoint: String) -> AsahiTracker {
        AsahiTracker::new(AsahiTrackerConfig {
            endpoint,
            db: None,
            port: None,
        })
    }

    fn issue_json(id: &str, identifier: &str, state: &str) -> Value {
        json!({
            "id": id,
            "identifier": identifier,
            "title": format!("Issue {identifier}"),
            "description": null,
            "priority": null,
            "state": state,
            "branch_name": null,
            "url": null,
            "labels": [],
            "blocked_by": [],
            "created_at": null,
            "updated_at": null
        })
    }

    #[tokio::test]
    async fn fetch_candidate_issues_requests_asahi_active_states() {
        let (endpoint, records, server) = spawn_mock_server(vec![MockResponse::json(
            200,
            json!({
                "issues": [
                    issue_json("1", "ASAHI-1", "Todo"),
                    issue_json("2", "ASAHI-2", "In Progress")
                ]
            }),
        )])
        .await;
        let tracker = tracker(endpoint);

        let issues = tracker.fetch_candidate_issues().await.unwrap();

        assert_eq!(issues.len(), 2);
        assert_eq!(issues[0].identifier, "ASAHI-1");
        server.await.unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests[0].method, "GET");
        assert!(requests[0].target.starts_with("/api/issues?"));
        assert!(requests[0].target.contains("states="));
        assert!(requests[0].target.contains("Todo"));
        assert!(
            requests[0].target.contains("In+Progress")
                || requests[0].target.contains("In%20Progress")
        );
    }

    #[tokio::test]
    async fn empty_state_and_id_filters_return_empty_without_http() {
        let tracker = tracker("not a url".to_string());

        let by_states = tracker.fetch_issues_by_states(&[]).await.unwrap();
        let by_ids = tracker.fetch_issue_states_by_ids(&[]).await.unwrap();

        assert!(by_states.is_empty());
        assert!(by_ids.is_empty());
    }

    #[tokio::test]
    async fn find_issue_by_locator_maps_success_and_not_found() {
        let (endpoint, records, server) = spawn_mock_server(vec![
            MockResponse::json(200, issue_json("1", "ASAHI-1", "Todo")),
            MockResponse::json(404, json!({ "error": "missing" })),
        ])
        .await;
        let tracker = tracker(endpoint);

        let found = tracker.find_issue_by_locator("ASAHI-1").await.unwrap();
        let missing = tracker.find_issue_by_locator("ASAHI-404").await.unwrap();

        assert_eq!(found.unwrap().id, "1");
        assert!(missing.is_none());
        server.await.unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests[0].target, "/api/issues/ASAHI-1");
        assert_eq!(requests[1].target, "/api/issues/ASAHI-404");
    }

    #[tokio::test]
    async fn list_issues_reports_http_status_before_json_decode() {
        let (endpoint, _records, server) =
            spawn_mock_server(vec![MockResponse::text(500, "not json")]).await;
        let tracker = tracker(endpoint);

        let err = tracker.fetch_candidate_issues().await.unwrap_err();

        assert!(err.to_string().contains("asahi list_issues failed"));
        assert!(err.to_string().contains("status=500"));
        server.await.unwrap();
    }

    #[tokio::test]
    async fn comments_and_wiki_use_expected_asahi_endpoints() {
        let (endpoint, records, server) = spawn_mock_server(vec![
            MockResponse::json(
                200,
                json!({
                    "comments": [{
                        "id": "c1",
                        "issue_id": "1",
                        "body": "hello",
                        "created_at": "2026-01-01T00:00:00Z"
                    }]
                }),
            ),
            MockResponse::json(
                200,
                json!({
                    "nodes": [{
                        "id": "w1",
                        "project_id": "p1",
                        "parent_id": null,
                        "kind": "page",
                        "title": "Readme",
                        "slug": "readme",
                        "content": "hello",
                        "current_version": null,
                        "created_at": null,
                        "updated_at": null,
                        "deleted_at": null
                    }]
                }),
            ),
        ])
        .await;
        let tracker = tracker(endpoint);
        let issue = serde_json::from_value(issue_json("1", "ASAHI-1", "Todo")).unwrap();

        let comments = tracker.fetch_comments(&issue).await.unwrap();
        let wiki = tracker.fetch_project_wiki("project-a").await.unwrap();

        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "hello");
        assert_eq!(wiki.len(), 1);
        assert_eq!(wiki[0].slug, "readme");
        server.await.unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].target, "/api/issues/1/comments");
        assert_eq!(requests[1].method, "GET");
        assert!(
            requests[1]
                .target
                .starts_with("/api/projects/project-a/wiki?")
        );
        assert!(requests[1].target.contains("recursive=true"));
    }

    #[tokio::test]
    async fn comment_state_and_activity_mutations_send_json_payloads() {
        let (endpoint, records, server) = spawn_mock_server(vec![
            MockResponse::json(200, json!({})),
            MockResponse::json(200, json!({})),
            MockResponse::json(200, json!({})),
        ])
        .await;
        let tracker = tracker(endpoint);
        let issue = serde_json::from_value(issue_json("1", "ASAHI-1", "Todo")).unwrap();

        tracker
            .create_comment(&issue, "progress update")
            .await
            .unwrap();
        tracker.update_issue_state("1", "Done").await.unwrap();
        tracker
            .create_activity(&issue, "agent_started", "Agent started", Some("Working"))
            .await
            .unwrap();

        server.await.unwrap();
        let requests = records.lock().unwrap();
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].target, "/api/issues/1/comments");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[0].body).unwrap(),
            json!({"body": "progress update"})
        );
        assert_eq!(requests[1].method, "PATCH");
        assert_eq!(requests[1].target, "/api/issues/1/state");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[1].body).unwrap(),
            json!({"state": "Done"})
        );
        assert_eq!(requests[2].method, "POST");
        assert_eq!(requests[2].target, "/api/issues/1/activities");
        assert_eq!(
            serde_json::from_str::<Value>(&requests[2].body).unwrap(),
            json!({
                "kind": "agent_started",
                "title": "Agent started",
                "body": "Working"
            })
        );
    }
}
