use rocket::{FromForm, Route, State, delete, get, patch, post, routes, serde::json::Json};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    api::error::ApiError,
    domain::Project,
    service::{CreateProjectInput, IssueService, ProjectFilter, UpdateProjectInput},
};

#[derive(Debug, FromForm)]
pub struct ListProjectsQuery {
    states: Option<String>,
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub description: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub priority: Option<Option<i64>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectStateRequest {
    pub state: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProjectListResponse {
    pub projects: Vec<Project>,
}

#[get("/projects?<query..>")]
async fn list_projects(
    query: ListProjectsQuery,
    service: &State<IssueService>,
) -> Result<Json<ProjectListResponse>, ApiError> {
    let projects = service
        .list_projects(ProjectFilter {
            states: split_csv(query.states),
            ids: split_csv(query.ids),
        })
        .await?;

    Ok(Json(ProjectListResponse { projects }))
}

#[post("/projects", data = "<body>")]
async fn create_project(
    body: Json<CreateProjectRequest>,
    service: &State<IssueService>,
) -> Result<Json<Project>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .create_project(CreateProjectInput {
                slug: body.slug,
                name: body.name,
                description: body.description,
                priority: body.priority,
                state: body.state,
            })
            .await?,
    ))
}

#[get("/projects/<locator>")]
async fn get_project(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Option<Json<Project>>, ApiError> {
    Ok(service.find_project(locator).await?.map(Json))
}

#[patch("/projects/<locator>", data = "<body>")]
async fn update_project(
    locator: &str,
    body: Json<UpdateProjectRequest>,
    service: &State<IssueService>,
) -> Result<Json<Project>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .update_project(
                locator,
                UpdateProjectInput {
                    name: body.name,
                    description: body.description,
                    priority: body.priority,
                },
            )
            .await?,
    ))
}

#[patch("/projects/<locator>/state", data = "<body>")]
async fn update_project_state(
    locator: &str,
    body: Json<UpdateProjectStateRequest>,
    service: &State<IssueService>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(
        service
            .update_project_state(locator, body.into_inner().state)
            .await?,
    ))
}

#[delete("/projects/<locator>")]
async fn delete_project(
    locator: &str,
    service: &State<IssueService>,
) -> Result<Json<Project>, ApiError> {
    Ok(Json(service.delete_project(locator).await?))
}

pub fn routes() -> Vec<Route> {
    routes![
        list_projects,
        create_project,
        get_project,
        update_project,
        update_project_state,
        delete_project
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

    use crate::{app, domain::Issue};

    use super::ProjectListResponse;

    #[test]
    fn manages_project_lifecycle_and_issue_association() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let created = client
            .post("/api/projects")
            .header(ContentType::JSON)
            .body(
                r#"{
                    "slug": "Project Alpha",
                    "name": "Project Alpha",
                    "description": "Goal: ship the first project abstraction.",
                    "priority": 1,
                    "state": "Backlog"
                }"#,
            )
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        let project: crate::domain::Project = created.into_json().expect("project json");
        assert_eq!(project.slug, "project-alpha");
        assert_eq!(project.priority, Some(1));
        assert_eq!(project.state, "Backlog");

        let issue = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(format!(
                r#"{{
                    "project_id": "{}",
                    "team_key": "ALP",
                    "title": "Wire issue to project"
                }}"#,
                project.id
            ))
            .dispatch();
        assert_eq!(issue.status(), Status::Ok);
        let issue: Issue = issue.into_json().expect("issue json");
        assert_eq!(issue.project_id.as_deref(), Some(project.id.as_str()));
        assert_eq!(
            issue.project.as_ref().map(|project| project.slug.as_str()),
            Some("project-alpha")
        );

        let listed = client
            .get(format!("/api/issues?project_id={}", project.id))
            .dispatch();
        assert_eq!(listed.status(), Status::Ok);
        let listed: crate::api::issues::IssueListResponse =
            listed.into_json().expect("issue list json");
        assert_eq!(listed.issues.len(), 1);
        assert_eq!(listed.issues[0].id, issue.id);

        let updated = client
            .patch(format!("/api/projects/{}/state", project.slug))
            .header(ContentType::JSON)
            .body(r#"{"state":"In Progress"}"#)
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);
        let updated: crate::domain::Project = updated.into_json().expect("updated project json");
        assert_eq!(updated.state, "In Progress");

        let projects = client.get("/api/projects?states=In%20Progress").dispatch();
        assert_eq!(projects.status(), Status::Ok);
        let projects: ProjectListResponse = projects.into_json().expect("projects json");
        assert_eq!(projects.projects.len(), 1);
        assert_eq!(projects.projects[0].id, project.id);
    }
}
