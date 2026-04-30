use rocket::{
    FromForm, Route, State, get, patch, post, routes,
    serde::json::{Json, Value, json},
};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    domain::{Comment, Issue},
    service::{CreateIssueInput, IssueFilter, IssueService},
};

#[derive(Debug, FromForm)]
pub struct ListIssuesQuery {
    project_slug: Option<String>,
    states: Option<String>,
    ids: Option<String>,
    assignee_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateIssueRequest {
    pub project_slug: Option<String>,
    pub team_key: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: Option<String>,
    pub branch_name: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub blocked_by: Vec<String>,
    pub assignee_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStateRequest {
    pub state: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueListResponse {
    pub issues: Vec<Issue>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CommentListResponse {
    pub comments: Vec<Comment>,
}

#[get("/issues?<query..>")]
async fn list_issues(
    query: ListIssuesQuery,
    service: &State<IssueService>,
) -> Result<Json<IssueListResponse>, ApiError> {
    let issues = service
        .list_issues(IssueFilter {
            project_slug: query.project_slug,
            states: split_csv(query.states),
            ids: split_csv(query.ids),
            assignee_id: query.assignee_id,
        })
        .await?;

    Ok(Json(IssueListResponse { issues }))
}

#[post("/issues", data = "<body>")]
async fn create_issue(
    body: Json<CreateIssueRequest>,
    service: &State<IssueService>,
) -> Result<Json<Issue>, ApiError> {
    let body = body.into_inner();
    let issue = service
        .create_issue(CreateIssueInput {
            project_slug: body.project_slug,
            team_key: body.team_key,
            title: Some(body.title),
            description: body.description,
            priority: body.priority,
            state: body.state,
            branch_name: body.branch_name,
            labels: body.labels,
            blocked_by: body.blocked_by,
            assignee_id: body.assignee_id,
        })
        .await?;

    Ok(Json(issue))
}

#[get("/issues/<locator>")]
async fn get_issue(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Option<Json<Issue>>, ApiError> {
    Ok(service.find_issue(locator).await?.map(Json))
}

#[patch("/issues/<locator>/state", data = "<body>")]
async fn update_issue_state(
    locator: &str,
    body: Json<UpdateStateRequest>,
    service: &State<IssueService>,
) -> Result<Json<Issue>, ApiError> {
    Ok(Json(
        service
            .update_issue_state(locator, body.into_inner().state)
            .await?,
    ))
}

#[post("/issues/<locator>/comments", data = "<body>")]
async fn create_comment(
    locator: &str,
    body: Json<CreateCommentRequest>,
    service: &State<IssueService>,
) -> Result<Json<Comment>, ApiError> {
    Ok(Json(
        service
            .create_comment(locator, body.into_inner().body)
            .await?,
    ))
}

#[get("/issues/<locator>/comments")]
async fn list_comments(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Json<CommentListResponse>, ApiError> {
    Ok(Json(CommentListResponse {
        comments: service.list_comments(locator).await?,
    }))
}

#[get("/")]
fn api_root() -> Json<Value> {
    Json(json!({
        "name": "asahi",
        "endpoints": [
            "GET /api/issues?project_slug=&states=Todo,In%20Progress&ids=&assignee_id=",
            "POST /api/issues",
            "GET /api/issues/{locator}",
            "PATCH /api/issues/{locator}/state",
            "POST /api/issues/{locator}/comments",
            "GET /api/issues/{locator}/comments"
        ]
    }))
}

pub fn routes() -> Vec<Route> {
    routes![
        api_root,
        list_issues,
        create_issue,
        get_issue,
        update_issue_state,
        create_comment,
        list_comments
    ]
}

fn split_csv(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(ToString::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use rocket::{
        http::{ContentType, Status},
        local::blocking::Client,
    };

    use crate::{api::issues::CommentListResponse, app, domain::Issue};

    use super::IssueListResponse;

    #[test]
    fn manages_issue_lifecycle() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(
                r#"{
                    "project_slug": "engineering",
                    "team_key": "ENG",
                    "title": "Build the HTTP tracker API",
                    "labels": ["backend"],
                    "assignee_id": "agent-1"
                }"#,
            )
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");
        assert_eq!(issue.identifier, "ENG-1");
        assert_eq!(issue.state, "Todo");

        let listed = client
            .get("/api/issues?project_slug=engineering&states=Todo&assignee_id=agent-1")
            .dispatch();
        assert_eq!(listed.status(), Status::Ok);
        let listed: IssueListResponse = listed.into_json().expect("list json");
        assert_eq!(listed.issues.len(), 1);
        assert_eq!(listed.issues[0].id, issue.id);

        let updated = client
            .patch(format!("/api/issues/{}/state", issue.identifier))
            .header(ContentType::JSON)
            .body(r#"{"state":"In Progress"}"#)
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);
        let updated: Issue = updated.into_json().expect("updated issue json");
        assert_eq!(updated.state, "In Progress");

        let commented = client
            .post(format!("/api/issues/{}/comments", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"body":"Started implementation."}"#)
            .dispatch();
        assert_eq!(commented.status(), Status::Ok);

        let comments = client
            .get(format!("/api/issues/{}/comments", issue.id))
            .dispatch();
        assert_eq!(comments.status(), Status::Ok);
        let comments: CommentListResponse = comments.into_json().expect("comments json");
        assert_eq!(comments.comments.len(), 1);
        assert_eq!(comments.comments[0].body, "Started implementation.");
    }
}
