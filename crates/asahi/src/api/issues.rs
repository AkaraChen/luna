use rocket::{
    FromForm, Route, State, delete, get, patch, post, routes,
    serde::json::{Json, Value, json},
};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    api::error::ApiError,
    domain::{Activity, Comment, Issue},
    service::{CreateIssueInput, IssueFilter, IssueService, UpdateIssueInput},
};

#[derive(Debug, FromForm)]
pub struct ListIssuesQuery {
    project_id: Option<String>,
    project_slug: Option<String>,
    states: Option<String>,
    ids: Option<String>,
    assignee_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateIssueRequest {
    pub project_id: Option<String>,
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
pub struct UpdateIssueRequest {
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub project_id: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub priority: Option<Option<i64>>,
    pub blocked_by: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateActivityRequest {
    pub kind: String,
    pub title: String,
    pub body: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IssueListResponse {
    pub issues: Vec<Issue>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CommentListResponse {
    pub comments: Vec<Comment>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ActivityListResponse {
    pub activities: Vec<Activity>,
}

#[get("/issues?<query..>")]
async fn list_issues(
    query: ListIssuesQuery,
    service: &State<IssueService>,
) -> Result<Json<IssueListResponse>, ApiError> {
    let issues = service
        .list_issues(IssueFilter {
            project_slug: query.project_slug,
            project_id: query.project_id,
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
            project_id: body.project_id,
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

#[patch("/issues/<locator>", data = "<body>")]
async fn update_issue(
    locator: &str,
    body: Json<UpdateIssueRequest>,
    service: &State<IssueService>,
) -> Result<Json<Issue>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .update_issue(
                locator,
                UpdateIssueInput {
                    title: body.title,
                    project_id: body.project_id,
                    description: body.description,
                    priority: body.priority,
                    blocked_by: body.blocked_by,
                },
            )
            .await?,
    ))
}

#[get("/issues/<locator>")]
async fn get_issue(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Option<Json<Issue>>, ApiError> {
    Ok(service.find_issue(locator).await?.map(Json))
}

#[delete("/issues/<locator>")]
async fn delete_issue(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Json<Issue>, ApiError> {
    Ok(Json(service.delete_issue(locator).await?))
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

#[post("/issues/<locator>/activities", data = "<body>")]
async fn create_activity(
    locator: &str,
    body: Json<CreateActivityRequest>,
    service: &State<IssueService>,
) -> Result<Json<Activity>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .create_activity(locator, body.kind, body.title, body.body)
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

#[get("/issues/<locator>/activities")]
async fn list_activities(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Json<ActivityListResponse>, ApiError> {
    Ok(Json(ActivityListResponse {
        activities: service.list_activities(locator).await?,
    }))
}

#[get("/")]
fn api_root() -> Json<Value> {
    Json(json!({
        "name": "asahi",
        "endpoints": [
            "GET /api/projects?states=Backlog,In%20Progress&ids=",
            "POST /api/projects",
            "GET /api/projects/{locator}",
            "PATCH /api/projects/{locator}",
            "DELETE /api/projects/{locator}",
            "PATCH /api/projects/{locator}/state",
            "GET /api/projects/{locator}/wiki?parent_id=&include_deleted=false",
            "POST /api/projects/{locator}/wiki",
            "GET /api/projects/{locator}/wiki/audits?node_id=&actor_kind=&limit=50",
            "GET /api/projects/{locator}/wiki/{node_locator}",
            "PATCH /api/projects/{locator}/wiki/{node_locator}",
            "DELETE /api/projects/{locator}/wiki/{node_locator}?actor_kind=&actor_id=",
            "GET /api/projects/{locator}/wiki/{page_locator}/versions",
            "GET /api/projects/{locator}/wiki/{page_locator}/versions/{version}",
            "POST /api/projects/{locator}/wiki/{page_locator}/rollback",
            "GET /api/projects/{locator}/wiki/{node_locator}/audits?actor_kind=&limit=50",
            "GET /api/issues?project_id=&project_slug=&states=Todo,In%20Progress&ids=&assignee_id=",
            "POST /api/issues",
            "GET /api/issues/{locator}",
            "PATCH /api/issues/{locator}",
            "DELETE /api/issues/{locator}",
            "PATCH /api/issues/{locator}/state",
            "POST /api/issues/{locator}/comments",
            "GET /api/issues/{locator}/comments",
            "GET /api/issues/{locator}/activities",
            "GET /api/notifications?include_archived=false&unread_only=false&recipient_id=&issue_id=&limit=50",
            "PATCH /api/notifications/{id}/read",
            "PATCH /api/notifications/{id}/archive"
        ]
    }))
}

pub fn routes() -> Vec<Route> {
    routes![
        api_root,
        list_issues,
        create_issue,
        update_issue,
        get_issue,
        delete_issue,
        update_issue_state,
        create_comment,
        list_comments,
        create_activity,
        list_activities
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

fn deserialize_optional_nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use rocket::{
        http::{ContentType, Status},
        local::blocking::Client,
    };

    use crate::{
        api::{notifications::NotificationListResponse, projects::ProjectListResponse},
        app,
        domain::Issue,
    };

    use super::{ActivityListResponse, CommentListResponse, IssueListResponse};

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
                    "priority": 1,
                    "labels": ["backend"],
                    "assignee_id": "agent-1"
                }"#,
            )
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");
        assert_eq!(issue.identifier, "ENG-1");
        assert_eq!(issue.priority, Some(1));
        assert_eq!(issue.state, "Todo");
        assert!(issue.updated_at.is_some());

        let blocked = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{
                    "project_slug": "engineering",
                    "team_key": "ENG",
                    "title": "Follow up after tracker API",
                    "priority": 2,
                    "blocked_by": ["{}"]
                }}"#,
                issue.identifier
            ))
            .dispatch();
        assert_eq!(blocked.status(), Status::Ok);
        let blocked: Issue = blocked.into_json().expect("blocked issue json");
        assert_eq!(blocked.identifier, "ENG-2");
        assert_eq!(blocked.priority, Some(2));
        assert_eq!(blocked.blocked_by.len(), 1);
        assert_eq!(blocked.blocked_by[0].identifier.as_deref(), Some("ENG-1"));

        let edited = client
            .patch(format!("/api/issues/{}", blocked.id))
            .header(ContentType::JSON)
            .body(r#"{"priority":null,"blocked_by":[]}"#)
            .dispatch();
        assert_eq!(edited.status(), Status::Ok);
        let edited: Issue = edited.into_json().expect("edited issue json");
        assert_eq!(edited.priority, None);
        assert!(edited.blocked_by.is_empty());

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

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        // After aggregation: 2 issues = 2 notifications
        assert_eq!(notifications.notifications.len(), 2);
        assert_eq!(notifications.unread_count, 2);

        let notification_id = notifications.notifications[0].id.clone();
        let read = client
            .patch(format!("/api/notifications/{notification_id}/read"))
            .dispatch();
        assert_eq!(read.status(), Status::Ok);

        let unread = client.get("/api/notifications?unread_only=true").dispatch();
        assert_eq!(unread.status(), Status::Ok);
        let unread: NotificationListResponse = unread.into_json().expect("unread json");
        assert_eq!(unread.notifications.len(), 1);

        let archived = client
            .patch(format!("/api/notifications/{notification_id}/archive"))
            .dispatch();
        assert_eq!(archived.status(), Status::Ok);

        let active = client.get("/api/notifications").dispatch();
        assert_eq!(active.status(), Status::Ok);
        let active: NotificationListResponse = active.into_json().expect("active json");
        assert_eq!(active.notifications.len(), 1);

        let deleted = client
            .delete(format!("/api/issues/{}", blocked.id))
            .dispatch();
        assert_eq!(deleted.status(), Status::Ok);
        let deleted: Issue = deleted.into_json().expect("deleted issue json");
        assert_eq!(deleted.id, blocked.id);

        let after_delete = client
            .get("/api/issues?project_slug=engineering")
            .dispatch();
        assert_eq!(after_delete.status(), Status::Ok);
        let after_delete: IssueListResponse =
            after_delete.into_json().expect("after delete list json");
        assert_eq!(after_delete.issues.len(), 1);
        assert_eq!(after_delete.issues[0].id, issue.id);
    }

    #[test]
    fn creates_issue_without_project_when_project_omitted() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"No project issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");
        assert_eq!(issue.project_id, None);
        assert_eq!(issue.project, None);

        let projects = client.get("/api/projects").dispatch();
        assert_eq!(projects.status(), Status::Ok);
        let projects: ProjectListResponse = projects.into_json().expect("projects json");
        assert!(projects.projects.is_empty());
    }

    #[test]
    fn notification_not_duplicated_on_priority_update() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 notification after create, got {}",
            notifications.notifications.len()
        );

        let updated = client
            .patch(format!("/api/issues/{}", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"priority":2}"#)
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 aggregated notification after priority update, got {}",
            notifications.notifications.len()
        );
    }

    #[test]
    fn notification_not_duplicated_on_state_update() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");

        let updated = client
            .patch(format!("/api/issues/{}/state", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"state":"In Progress"}"#)
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 aggregated notification after state update, got {}",
            notifications.notifications.len()
        );
    }

    #[test]
    fn mark_read_does_not_duplicate_notification() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let _issue: Issue = created.into_json().expect("issue json");

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(notifications.notifications.len(), 1);
        let notification_id = notifications.notifications[0].id.clone();

        let read = client
            .patch(format!("/api/notifications/{}/read", notification_id))
            .dispatch();
        assert_eq!(read.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 notification after mark-read, got {}",
            notifications.notifications.len()
        );

        let archive = client
            .patch(format!("/api/notifications/{}/archive", notification_id))
            .dispatch();
        assert_eq!(archive.status(), Status::Ok);

        let notifications = client
            .get("/api/notifications?include_archived=true&limit=10")
            .dispatch();
        assert_eq!(notifications.status(), Status::Ok);
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 notification after archive, got {}",
            notifications.notifications.len()
        );
    }

    #[test]
    fn notification_aggregates_per_issue() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");

        // Multiple updates should still result in only 1 notification
        let _ = client
            .patch(format!("/api/issues/{}", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"priority":2}"#)
            .dispatch();
        let _ = client
            .patch(format!("/api/issues/{}/state", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"state":"In Progress"}"#)
            .dispatch();
        let _ = client
            .post(format!("/api/issues/{}/comments", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"body":"A comment"}"#)
            .dispatch();

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 aggregated notification, got {}",
            notifications.notifications.len()
        );

        // But activities should contain all operations
        let activities = client
            .get(format!("/api/issues/{}/activities", issue.id))
            .dispatch();
        assert_eq!(activities.status(), Status::Ok);
        let activities: ActivityListResponse = activities.into_json().expect("activities json");
        assert_eq!(
            activities.activities.len(),
            3,
            "Expected 3 activities (priority + state + comment), got {}",
            activities.activities.len()
        );
    }

    #[test]
    fn archived_notification_creates_new_one_on_next_update() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let issue: Issue = created.into_json().expect("issue json");

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        let notification_id = notifications.notifications[0].id.clone();

        // Archive the notification
        let archived = client
            .patch(format!("/api/notifications/{}/archive", notification_id))
            .dispatch();
        assert_eq!(archived.status(), Status::Ok);

        // Update issue again — should create a brand new notification
        let updated = client
            .patch(format!("/api/issues/{}/state", issue.id))
            .header(ContentType::JSON)
            .body(r#"{"state":"In Progress"}"#)
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 new notification after archive + update, got {}",
            notifications.notifications.len()
        );
        assert_ne!(
            notifications.notifications[0].id, notification_id,
            "New notification should have a different id"
        );
    }

    #[test]
    fn concurrent_mark_read_should_not_duplicate() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(r#"{"title":"Test issue"}"#)
            .dispatch();
        assert_eq!(created.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        let notification_id = notifications.notifications[0].id.clone();

        // Simulate double-click / rapid requests
        let read1 = client
            .patch(format!("/api/notifications/{}/read", notification_id))
            .dispatch();
        let read2 = client
            .patch(format!("/api/notifications/{}/read", notification_id))
            .dispatch();
        assert_eq!(read1.status(), Status::Ok);
        assert_eq!(read2.status(), Status::Ok);

        let notifications = client.get("/api/notifications?limit=10").dispatch();
        let notifications: NotificationListResponse =
            notifications.into_json().expect("notifications json");
        assert_eq!(
            notifications.notifications.len(),
            1,
            "Expected 1 notification after double mark-read, got {}",
            notifications.notifications.len()
        );
    }
}
