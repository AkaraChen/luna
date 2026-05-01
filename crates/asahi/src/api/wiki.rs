use rocket::{FromForm, Route, State, delete, get, patch, post, routes, serde::json::Json};
use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    api::error::ApiError,
    domain::{WikiAudit, WikiNode, WikiPageVersion},
    service::{
        CreateWikiNodeInput, IssueService, RollbackWikiPageInput, UpdateWikiNodeInput,
        WikiAuditFilter, WikiNodeFilter,
    },
};

#[derive(Debug, FromForm)]
pub struct ListWikiNodesQuery {
    parent_id: Option<String>,
    include_deleted: Option<bool>,
    recursive: Option<bool>,
}

#[derive(Debug, FromForm)]
pub struct WikiAuditQuery {
    node_id: Option<String>,
    actor_kind: Option<String>,
    limit: Option<u64>,
}

#[derive(Debug, FromForm)]
pub struct DeleteWikiNodeQuery {
    actor_kind: Option<String>,
    actor_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateWikiNodeRequest {
    pub parent_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub content: Option<String>,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateWikiNodeRequest {
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub parent_id: Option<Option<String>>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub content: Option<Option<String>>,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackWikiPageRequest {
    pub version: i64,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WikiNodeListResponse {
    pub nodes: Vec<WikiNode>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WikiVersionListResponse {
    pub versions: Vec<WikiPageVersion>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WikiAuditListResponse {
    pub audits: Vec<WikiAudit>,
}

#[get("/projects/<project_locator>/wiki?<query..>")]
async fn list_wiki_nodes(
    project_locator: &str,
    query: ListWikiNodesQuery,
    service: &State<IssueService>,
) -> Result<Json<WikiNodeListResponse>, ApiError> {
    Ok(Json(WikiNodeListResponse {
        nodes: service
            .list_wiki_nodes(
                project_locator,
                WikiNodeFilter {
                    parent_id: query.parent_id,
                    include_deleted: query.include_deleted.unwrap_or(false),
                    recursive: query.recursive.unwrap_or(false),
                },
            )
            .await?,
    }))
}

#[post("/projects/<project_locator>/wiki", data = "<body>")]
async fn create_wiki_node(
    project_locator: &str,
    body: Json<CreateWikiNodeRequest>,
    service: &State<IssueService>,
) -> Result<Json<WikiNode>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .create_wiki_node(
                project_locator,
                CreateWikiNodeInput {
                    parent_id: body.parent_id,
                    kind: body.kind,
                    title: Some(body.title),
                    content: body.content,
                    actor_kind: body.actor_kind,
                    actor_id: body.actor_id,
                    summary: body.summary,
                },
            )
            .await?,
    ))
}

#[get("/projects/<project_locator>/wiki/audits?<query..>")]
async fn list_wiki_audits(
    project_locator: &str,
    query: WikiAuditQuery,
    service: &State<IssueService>,
) -> Result<Json<WikiAuditListResponse>, ApiError> {
    Ok(Json(WikiAuditListResponse {
        audits: service
            .list_wiki_audits(
                project_locator,
                WikiAuditFilter {
                    node_id: query.node_id,
                    actor_kind: query.actor_kind,
                    limit: query.limit,
                },
            )
            .await?,
    }))
}

#[get("/projects/<project_locator>/wiki/<node_locator>")]
async fn get_wiki_node(
    project_locator: &str,
    node_locator: &str,
    service: &State<IssueService>,
) -> Result<Option<Json<WikiNode>>, ApiError> {
    Ok(service
        .find_wiki_node(project_locator, node_locator)
        .await?
        .map(Json))
}

#[patch("/projects/<project_locator>/wiki/<node_locator>", data = "<body>")]
async fn update_wiki_node(
    project_locator: &str,
    node_locator: &str,
    body: Json<UpdateWikiNodeRequest>,
    service: &State<IssueService>,
) -> Result<Json<WikiNode>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .update_wiki_node(
                project_locator,
                node_locator,
                UpdateWikiNodeInput {
                    parent_id: body.parent_id,
                    title: body.title,
                    content: body.content,
                    actor_kind: body.actor_kind,
                    actor_id: body.actor_id,
                    summary: body.summary,
                },
            )
            .await?,
    ))
}

#[delete("/projects/<project_locator>/wiki/<node_locator>?<query..>")]
async fn delete_wiki_node(
    project_locator: &str,
    node_locator: &str,
    query: DeleteWikiNodeQuery,
    service: &State<IssueService>,
) -> Result<Json<WikiNode>, ApiError> {
    Ok(Json(
        service
            .delete_wiki_node(
                project_locator,
                node_locator,
                query.actor_kind,
                query.actor_id,
            )
            .await?,
    ))
}

#[get("/projects/<project_locator>/wiki/<page_locator>/versions")]
async fn list_wiki_versions(
    project_locator: &str,
    page_locator: &str,
    service: &State<IssueService>,
) -> Result<Json<WikiVersionListResponse>, ApiError> {
    Ok(Json(WikiVersionListResponse {
        versions: service
            .list_wiki_versions(project_locator, page_locator)
            .await?,
    }))
}

#[get("/projects/<project_locator>/wiki/<page_locator>/versions/<version>")]
async fn get_wiki_version(
    project_locator: &str,
    page_locator: &str,
    version: i64,
    service: &State<IssueService>,
) -> Result<Json<WikiPageVersion>, ApiError> {
    Ok(Json(
        service
            .get_wiki_version(project_locator, page_locator, version)
            .await?,
    ))
}

#[post(
    "/projects/<project_locator>/wiki/<page_locator>/rollback",
    data = "<body>"
)]
async fn rollback_wiki_page(
    project_locator: &str,
    page_locator: &str,
    body: Json<RollbackWikiPageRequest>,
    service: &State<IssueService>,
) -> Result<Json<WikiNode>, ApiError> {
    let body = body.into_inner();
    Ok(Json(
        service
            .rollback_wiki_page(
                project_locator,
                page_locator,
                RollbackWikiPageInput {
                    version: body.version,
                    actor_kind: body.actor_kind,
                    actor_id: body.actor_id,
                    summary: body.summary,
                },
            )
            .await?,
    ))
}

#[get("/projects/<project_locator>/wiki/<node_locator>/audits?<query..>")]
async fn list_wiki_node_audits(
    project_locator: &str,
    node_locator: &str,
    query: WikiAuditQuery,
    service: &State<IssueService>,
) -> Result<Json<WikiAuditListResponse>, ApiError> {
    Ok(Json(WikiAuditListResponse {
        audits: service
            .list_wiki_audits(
                project_locator,
                WikiAuditFilter {
                    node_id: Some(node_locator.to_string()),
                    actor_kind: query.actor_kind,
                    limit: query.limit,
                },
            )
            .await?,
    }))
}

pub fn routes() -> Vec<Route> {
    routes![
        list_wiki_nodes,
        create_wiki_node,
        list_wiki_audits,
        get_wiki_node,
        update_wiki_node,
        delete_wiki_node,
        list_wiki_versions,
        get_wiki_version,
        rollback_wiki_page,
        list_wiki_node_audits
    ]
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

    use crate::{app, domain::WikiNodeKind};

    use super::{WikiAuditListResponse, WikiNodeListResponse, WikiVersionListResponse};

    #[test]
    fn manages_project_wiki_lifecycle_with_versions_and_audits() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        let project = client
            .post("/api/projects")
            .header(ContentType::JSON)
            .body(r#"{"slug":"docs","name":"Docs"}"#)
            .dispatch();
        assert_eq!(project.status(), Status::Ok);
        let project: crate::domain::Project = project.into_json().expect("project json");

        let folder = client
            .post(format!("/api/projects/{}/wiki", project.slug))
            .header(ContentType::JSON)
            .body(
                r#"{
                    "kind": "folder",
                    "title": "Guides",
                    "actor_kind": "human",
                    "actor_id": "user-1"
                }"#,
            )
            .dispatch();
        assert_eq!(folder.status(), Status::Ok);
        let folder: crate::domain::WikiNode = folder.into_json().expect("folder json");
        assert_eq!(folder.kind, WikiNodeKind::Folder);
        assert_eq!(folder.parent_id, None);

        let child_folder = client
            .post(format!("/api/projects/{}/wiki", project.slug))
            .header(ContentType::JSON)
            .body(format!(
                r#"{{
                    "kind": "folder",
                    "parent_id": "{}",
                    "title": "Backend",
                    "actor_kind": "agent",
                    "actor_id": "agent-1"
                }}"#,
                folder.id
            ))
            .dispatch();
        assert_eq!(child_folder.status(), Status::Ok);
        let child_folder: crate::domain::WikiNode =
            child_folder.into_json().expect("child folder json");
        assert_eq!(child_folder.parent_id.as_deref(), Some(folder.id.as_str()));

        let page = client
            .post(format!("/api/projects/{}/wiki", project.slug))
            .header(ContentType::JSON)
            .body(format!(
                r#"{{
                    "kind": "page",
                    "parent_id": "{}",
                    "title": "Project Wiki",
                    "content": "Initial design",
                    "actor_kind": "agent",
                    "actor_id": "agent-1",
                    "summary": "Seed page"
                }}"#,
                child_folder.id
            ))
            .dispatch();
        assert_eq!(page.status(), Status::Ok);
        let page: crate::domain::WikiNode = page.into_json().expect("page json");
        assert_eq!(page.kind, WikiNodeKind::Page);
        assert_eq!(page.content.as_deref(), Some("Initial design"));
        assert_eq!(
            page.current_version.as_ref().map(|version| version.version),
            Some(1)
        );

        let root_nodes = client
            .get(format!("/api/projects/{}/wiki", project.slug))
            .dispatch();
        assert_eq!(root_nodes.status(), Status::Ok);
        let root_nodes: WikiNodeListResponse = root_nodes.into_json().expect("root list json");
        assert_eq!(root_nodes.nodes.len(), 1);
        assert_eq!(root_nodes.nodes[0].id, folder.id);

        let child_nodes = client
            .get(format!(
                "/api/projects/{}/wiki?parent_id={}",
                project.slug, child_folder.id
            ))
            .dispatch();
        assert_eq!(child_nodes.status(), Status::Ok);
        let child_nodes: WikiNodeListResponse = child_nodes.into_json().expect("child list json");
        assert_eq!(child_nodes.nodes.len(), 1);
        assert_eq!(child_nodes.nodes[0].id, page.id);

        let updated = client
            .patch(format!("/api/projects/{}/wiki/{}", project.slug, page.id))
            .header(ContentType::JSON)
            .body(
                r#"{
                    "content": "Revised design",
                    "actor_kind": "human",
                    "actor_id": "user-1",
                    "summary": "Clarify audit rules"
                }"#,
            )
            .dispatch();
        assert_eq!(updated.status(), Status::Ok);
        let updated: crate::domain::WikiNode = updated.into_json().expect("updated page json");
        assert_eq!(updated.content.as_deref(), Some("Revised design"));
        assert_eq!(
            updated
                .current_version
                .as_ref()
                .map(|version| version.version),
            Some(2)
        );

        let versions = client
            .get(format!(
                "/api/projects/{}/wiki/{}/versions",
                project.slug, page.id
            ))
            .dispatch();
        assert_eq!(versions.status(), Status::Ok);
        let versions: WikiVersionListResponse = versions.into_json().expect("versions json");
        assert_eq!(versions.versions.len(), 2);
        assert_eq!(versions.versions[0].version, 2);
        assert_eq!(versions.versions[1].version, 1);

        let first_version = client
            .get(format!(
                "/api/projects/{}/wiki/{}/versions/1",
                project.slug, page.id
            ))
            .dispatch();
        assert_eq!(first_version.status(), Status::Ok);
        let first_version: crate::domain::WikiPageVersion =
            first_version.into_json().expect("version json");
        assert_eq!(first_version.content, "Initial design");

        let audits = client
            .get(format!(
                "/api/projects/{}/wiki/{}/audits?actor_kind=human",
                project.slug, page.id
            ))
            .dispatch();
        assert_eq!(audits.status(), Status::Ok);
        let audits: WikiAuditListResponse = audits.into_json().expect("audits json");
        assert_eq!(audits.audits.len(), 1);
        assert_eq!(audits.audits[0].action, "page_updated");

        let rolled_back = client
            .post(format!(
                "/api/projects/{}/wiki/{}/rollback",
                project.slug, page.id
            ))
            .header(ContentType::JSON)
            .body(
                r#"{
                    "version": 1,
                    "actor_kind": "agent",
                    "actor_id": "agent-1"
                }"#,
            )
            .dispatch();
        assert_eq!(rolled_back.status(), Status::Ok);
        let rolled_back: crate::domain::WikiNode =
            rolled_back.into_json().expect("rolled back page json");
        assert_eq!(rolled_back.content.as_deref(), Some("Initial design"));
        assert_eq!(
            rolled_back
                .current_version
                .as_ref()
                .map(|version| version.version),
            Some(3)
        );

        let deleted = client
            .delete(format!(
                "/api/projects/{}/wiki/{}?actor_kind=human&actor_id=user-1",
                project.slug, folder.id
            ))
            .dispatch();
        assert_eq!(deleted.status(), Status::Ok);
        let deleted: crate::domain::WikiNode = deleted.into_json().expect("deleted folder json");
        assert!(deleted.deleted_at.is_some());

        let active_root_nodes = client
            .get(format!("/api/projects/{}/wiki", project.slug))
            .dispatch();
        assert_eq!(active_root_nodes.status(), Status::Ok);
        let active_root_nodes: WikiNodeListResponse = active_root_nodes
            .into_json()
            .expect("active root list json");
        assert!(active_root_nodes.nodes.is_empty());

        let deleted_root_nodes = client
            .get(format!(
                "/api/projects/{}/wiki?include_deleted=true",
                project.slug
            ))
            .dispatch();
        assert_eq!(deleted_root_nodes.status(), Status::Ok);
        let deleted_root_nodes: WikiNodeListResponse = deleted_root_nodes
            .into_json()
            .expect("deleted root list json");
        assert_eq!(deleted_root_nodes.nodes.len(), 1);
        assert_eq!(deleted_root_nodes.nodes[0].id, folder.id);
    }
}
