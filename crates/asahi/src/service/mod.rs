use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DatabaseTransaction, DbErr,
    EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set,
    TransactionTrait,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        Activity, BlockerRef, Comment, Issue, IssueState, Notification, NotificationIssueRef,
        Project, ProjectRef, WikiAudit, WikiNode, WikiNodeKind, WikiPageVersion, WikiVersionRef,
        default_team_key, issue_matches_locator, project_matches_locator,
        wiki_node_matches_locator,
    },
    entity::{
        activity, comment, issue, issue_label, issue_relation, notification, project, wiki_audit,
        wiki_node, wiki_page_version,
    },
};

#[derive(Clone, Debug)]
pub struct IssueService {
    db: DatabaseConnection,
}

impl IssueService {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn create_issue(&self, input: CreateIssueInput) -> ServiceResult<Issue> {
        let title = require_non_empty("title", input.title)?;
        let project_model = self
            .ensure_project_for_issue(input.project_id.as_deref(), input.project_slug.as_deref())
            .await?;
        let project_slug = project_model
            .as_ref()
            .map(|project| project.slug.clone())
            .unwrap_or_else(|| "default".to_string());
        let team_key = input
            .team_key
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_team_key(&project_slug));
        let state = non_empty_or(input.state, &IssueState::Todo.to_string());
        let labels = normalize_list(input.labels);
        let blocked_by_ids = self.resolve_issue_locators(&input.blocked_by).await?;
        let number = self.next_issue_number(&team_key).await?;
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let identifier = format!("{team_key}-{number}");
        let url = Some(format!("/api/issues/{}", url_safe_identifier(&identifier)));

        let transaction = self.db.begin().await?;
        issue::ActiveModel {
            id: Set(id.clone()),
            identifier: Set(identifier),
            project_id: Set(project_model.as_ref().map(|project| project.id.clone())),
            project_slug: Set(project_slug),
            team_key: Set(team_key),
            number: Set(number),
            title: Set(title),
            description: Set(input.description),
            priority: Set(input.priority),
            state: Set(state),
            branch_name: Set(input.branch_name),
            url: Set(url),
            assignee_id: Set(input.assignee_id),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&transaction)
        .await?;

        for name in labels {
            issue_label::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                issue_id: Set(id.clone()),
                name: Set(name),
            }
            .insert(&transaction)
            .await?;
        }

        for blocked_by_issue_id in blocked_by_ids {
            issue_relation::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                issue_id: Set(id.clone()),
                blocked_by_issue_id: Set(blocked_by_issue_id),
            }
            .insert(&transaction)
            .await?;
        }

        transaction.commit().await?;
        let issue = self
            .find_issue_by_id(&id)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(id))?;
        self.upsert_notification(
            &issue,
            "issue_created",
            format!("{} created", issue.identifier),
            Some(issue.title.clone()),
        )
        .await?;
        Ok(issue)
    }

    pub async fn create_project(&self, input: CreateProjectInput) -> ServiceResult<Project> {
        let name = input
            .name
            .as_deref()
            .and_then(non_empty_str)
            .map(ToString::to_string);
        let slug_source = input
            .slug
            .as_deref()
            .and_then(non_empty_str)
            .or(name.as_deref())
            .ok_or_else(|| {
                ServiceError::InvalidInput("project slug or name is required".to_string())
            })?;
        let slug = normalize_slug(slug_source)?;
        if self.find_project_id(&slug).await?.is_some() {
            return Err(ServiceError::InvalidInput(format!(
                "project already exists: {slug}"
            )));
        }

        let name = name.unwrap_or_else(|| slug.clone());
        let state = non_empty_or(input.state, &IssueState::Backlog.to_string());
        let now = Utc::now();
        let id = Uuid::new_v4().to_string();
        let url = Some(format!("/api/projects/{}", url_safe_identifier(&slug)));

        let model = project::ActiveModel {
            id: Set(id.clone()),
            slug: Set(slug),
            name: Set(name),
            description: Set(input.description),
            priority: Set(input.priority),
            state: Set(state),
            url: Set(url),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        Ok(model_to_project(model))
    }

    pub async fn list_projects(&self, filter: ProjectFilter) -> ServiceResult<Vec<Project>> {
        let mut query = project::Entity::find();

        let states = normalize_list(filter.states);
        if !states.is_empty() {
            query = query.filter(project::Column::State.is_in(states));
        }

        let ids = normalize_list(filter.ids);
        if !ids.is_empty() {
            query = query.filter(project::Column::Id.is_in(ids.clone()));
        }

        let mut projects = query
            .order_by_asc(project::Column::CreatedAt)
            .order_by_asc(project::Column::Slug)
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_project)
            .collect::<Vec<_>>();

        if !ids.is_empty() {
            let index = ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
                .collect::<HashMap<_, _>>();
            projects.sort_by_key(|project| index.get(&project.id).copied().unwrap_or(usize::MAX));
        }

        Ok(projects)
    }

    pub async fn find_project(&self, locator: &str) -> ServiceResult<Option<Project>> {
        self.find_project_model(locator)
            .await
            .map(|model| model.map(model_to_project))
    }

    pub async fn update_project(
        &self,
        locator: &str,
        input: UpdateProjectInput,
    ) -> ServiceResult<Project> {
        let project_id = self
            .find_project_id(locator)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))?;
        let model = project::Entity::find_by_id(project_id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))?;
        let mut active = model.into_active_model();
        if let Some(name) = input.name {
            active.name = Set(require_non_empty("name", Some(name))?);
        }
        if let Some(description) = input.description {
            active.description = Set(description);
        }
        if let Some(priority) = input.priority {
            active.priority = Set(priority);
        }
        active.updated_at = Set(Utc::now());
        let model = active.update(&self.db).await?;

        Ok(model_to_project(model))
    }

    pub async fn update_project_state(
        &self,
        locator: &str,
        state: String,
    ) -> ServiceResult<Project> {
        let state = require_non_empty("state", Some(state))?;
        let project_id = self
            .find_project_id(locator)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))?;
        let model = project::Entity::find_by_id(project_id)
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))?;
        let mut active = model.into_active_model();
        active.state = Set(state);
        active.updated_at = Set(Utc::now());
        let model = active.update(&self.db).await?;

        Ok(model_to_project(model))
    }

    pub async fn delete_project(&self, locator: &str) -> ServiceResult<Project> {
        let project_id = self
            .find_project_id(locator)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))?;
        let project = self
            .find_project(&project_id)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(project_id.clone()))?;

        let transaction = self.db.begin().await?;
        issue::Entity::update_many()
            .col_expr(
                issue::Column::ProjectId,
                sea_orm::sea_query::Expr::value(None::<String>),
            )
            .col_expr(
                issue::Column::ProjectSlug,
                sea_orm::sea_query::Expr::value("default"),
            )
            .filter(issue::Column::ProjectId.eq(project_id.clone()))
            .exec(&transaction)
            .await?;
        project::Entity::delete_by_id(project_id)
            .exec(&transaction)
            .await?;
        transaction.commit().await?;

        Ok(project)
    }

    pub async fn list_wiki_nodes(
        &self,
        project_locator: &str,
        filter: WikiNodeFilter,
    ) -> ServiceResult<Vec<WikiNode>> {
        let project = self.resolve_project_locator(project_locator).await?;

        let mut query =
            wiki_node::Entity::find().filter(wiki_node::Column::ProjectId.eq(project.id.clone()));

        if filter.recursive {
            // Return all wiki nodes for the project regardless of depth
            if !filter.include_deleted {
                query = query.filter(wiki_node::Column::DeletedAt.is_null());
            }
        } else {
            let parent_id = match filter.parent_id.as_deref().and_then(non_empty_str) {
                Some(parent_id) => Some(
                    self.resolve_wiki_parent(&project.id, Some(parent_id))
                        .await?
                        .ok_or_else(|| ServiceError::WikiNodeNotFound(parent_id.to_string()))?,
                ),
                None => None,
            };
            query = match parent_id {
                Some(parent_id) => query.filter(wiki_node::Column::ParentId.eq(parent_id)),
                None => query.filter(wiki_node::Column::ParentId.is_null()),
            };
            if !filter.include_deleted {
                query = query.filter(wiki_node::Column::DeletedAt.is_null());
            }
        }

        let models = query
            .order_by_asc(wiki_node::Column::Kind)
            .order_by_asc(wiki_node::Column::Title)
            .all(&self.db)
            .await?;

        self.hydrate_wiki_nodes(models).await
    }

    pub async fn find_wiki_node(
        &self,
        project_locator: &str,
        node_locator: &str,
    ) -> ServiceResult<Option<WikiNode>> {
        let project = self.resolve_project_locator(project_locator).await?;
        let Some(model) = self
            .find_wiki_node_model(&project.id, node_locator, false)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.hydrate_wiki_node(model).await?))
    }

    pub async fn create_wiki_node(
        &self,
        project_locator: &str,
        input: CreateWikiNodeInput,
    ) -> ServiceResult<WikiNode> {
        let project = self.resolve_project_locator(project_locator).await?;
        let kind = parse_wiki_kind(&input.kind)?;
        let title = require_non_empty("title", input.title)?;
        let slug = normalize_slug(&title)?;
        let parent_id = self
            .resolve_wiki_parent(&project.id, input.parent_id.as_deref())
            .await?;
        self.ensure_wiki_sibling_available(&project.id, parent_id.as_deref(), &slug, None)
            .await?;

        let actor_kind = normalize_actor_kind(input.actor_kind)?;
        let actor_id = optional_non_empty(input.actor_id);
        let summary = optional_non_empty(input.summary);
        let now = Utc::now();
        let node_id = Uuid::new_v4().to_string();
        let content = match kind {
            WikiNodeKind::Folder => None,
            WikiNodeKind::Page => Some(input.content.unwrap_or_default()),
        };

        let transaction = self.db.begin().await?;
        let model = wiki_node::ActiveModel {
            id: Set(node_id.clone()),
            project_id: Set(project.id.clone()),
            parent_id: Set(parent_id),
            kind: Set(kind.as_str().to_string()),
            title: Set(title.clone()),
            slug: Set(slug),
            content: Set(content.clone()),
            current_version_id: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
        }
        .insert(&transaction)
        .await?;

        let version_id = if kind == WikiNodeKind::Page {
            let version = wiki_page_version::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                page_id: Set(node_id.clone()),
                version: Set(1),
                title: Set(title.clone()),
                content: Set(content.unwrap_or_default()),
                actor_kind: Set(actor_kind.clone()),
                actor_id: Set(actor_id.clone()),
                summary: Set(summary.clone()),
                created_at: Set(now),
            }
            .insert(&transaction)
            .await?;
            let mut active = model.into_active_model();
            active.current_version_id = Set(Some(version.id.clone()));
            active.update(&transaction).await?;
            Some(version.id)
        } else {
            None
        };

        self.create_wiki_audit_in_transaction(
            &transaction,
            &project.id,
            &node_id,
            version_id.as_deref(),
            match kind {
                WikiNodeKind::Folder => "folder_created",
                WikiNodeKind::Page => "page_created",
            },
            &actor_kind,
            actor_id.as_deref(),
            summary.as_deref(),
            now,
        )
        .await?;
        transaction.commit().await?;

        let model = wiki_node::Entity::find_by_id(node_id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(node_id.clone()))?;
        self.hydrate_wiki_node(model).await
    }

    pub async fn update_wiki_node(
        &self,
        project_locator: &str,
        node_locator: &str,
        input: UpdateWikiNodeInput,
    ) -> ServiceResult<WikiNode> {
        let project = self.resolve_project_locator(project_locator).await?;
        let model = self
            .find_wiki_node_model(&project.id, node_locator, false)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(node_locator.to_string()))?;
        let kind = parse_wiki_kind(&model.kind)?;
        let next_parent_id = match input.parent_id {
            Some(Some(parent_id)) => {
                self.resolve_wiki_parent(&project.id, Some(parent_id.as_str()))
                    .await?
            }
            Some(None) => None,
            None => model.parent_id.clone(),
        };

        if next_parent_id.as_deref() == Some(model.id.as_str()) {
            return Err(ServiceError::InvalidInput(
                "wiki node cannot be moved under itself".to_string(),
            ));
        }
        if let Some(parent_id) = next_parent_id.as_deref() {
            let descendant_ids = self
                .collect_wiki_descendant_ids(&project.id, &model.id)
                .await?;
            if descendant_ids.iter().any(|id| id == parent_id) {
                return Err(ServiceError::InvalidInput(
                    "wiki node cannot be moved under its descendant".to_string(),
                ));
            }
        }

        let next_title = match input.title {
            Some(title) => require_non_empty("title", Some(title))?,
            None => model.title.clone(),
        };
        let next_slug = normalize_slug(&next_title)?;
        self.ensure_wiki_sibling_available(
            &project.id,
            next_parent_id.as_deref(),
            &next_slug,
            Some(model.id.as_str()),
        )
        .await?;

        let next_content = match kind {
            WikiNodeKind::Folder => {
                if input.content.is_some() {
                    return Err(ServiceError::InvalidInput(
                        "folder content cannot be updated".to_string(),
                    ));
                }
                None
            }
            WikiNodeKind::Page => match input.content {
                Some(Some(content)) => Some(content),
                Some(None) => Some(String::new()),
                None => model.content.clone().or_else(|| Some(String::new())),
            },
        };

        let actor_kind = normalize_actor_kind(input.actor_kind)?;
        let actor_id = optional_non_empty(input.actor_id);
        let summary = optional_non_empty(input.summary);
        let now = Utc::now();
        let page_document_changed = kind == WikiNodeKind::Page
            && (next_title != model.title || next_content != model.content);
        let version_number = if page_document_changed {
            Some(self.next_wiki_version_number(&model.id).await?)
        } else {
            None
        };

        let transaction = self.db.begin().await?;
        let mut active = model.clone().into_active_model();
        active.parent_id = Set(next_parent_id);
        active.title = Set(next_title.clone());
        active.slug = Set(next_slug);
        active.content = Set(next_content.clone());
        active.updated_at = Set(now);

        let version_id = if let Some(version_number) = version_number {
            let version = wiki_page_version::ActiveModel {
                id: Set(Uuid::new_v4().to_string()),
                page_id: Set(model.id.clone()),
                version: Set(version_number),
                title: Set(next_title.clone()),
                content: Set(next_content.clone().unwrap_or_default()),
                actor_kind: Set(actor_kind.clone()),
                actor_id: Set(actor_id.clone()),
                summary: Set(summary.clone()),
                created_at: Set(now),
            }
            .insert(&transaction)
            .await?;
            active.current_version_id = Set(Some(version.id.clone()));
            Some(version.id)
        } else {
            None
        };

        active.update(&transaction).await?;
        self.create_wiki_audit_in_transaction(
            &transaction,
            &project.id,
            &model.id,
            version_id.as_deref(),
            match kind {
                WikiNodeKind::Folder => "folder_updated",
                WikiNodeKind::Page => "page_updated",
            },
            &actor_kind,
            actor_id.as_deref(),
            summary.as_deref(),
            now,
        )
        .await?;
        transaction.commit().await?;

        let model = wiki_node::Entity::find_by_id(model.id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(model.id.clone()))?;
        self.hydrate_wiki_node(model).await
    }

    pub async fn delete_wiki_node(
        &self,
        project_locator: &str,
        node_locator: &str,
        actor_kind: Option<String>,
        actor_id: Option<String>,
    ) -> ServiceResult<WikiNode> {
        let project = self.resolve_project_locator(project_locator).await?;
        let model = self
            .find_wiki_node_model(&project.id, node_locator, false)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(node_locator.to_string()))?;
        let kind = parse_wiki_kind(&model.kind)?;
        let actor_kind = normalize_actor_kind(actor_kind)?;
        let actor_id = optional_non_empty(actor_id);
        let now = Utc::now();
        let ids = self
            .collect_wiki_descendant_ids(&project.id, &model.id)
            .await?;

        let transaction = self.db.begin().await?;
        wiki_node::Entity::update_many()
            .col_expr(
                wiki_node::Column::DeletedAt,
                sea_orm::sea_query::Expr::value(Some(now)),
            )
            .col_expr(
                wiki_node::Column::UpdatedAt,
                sea_orm::sea_query::Expr::value(now),
            )
            .filter(wiki_node::Column::Id.is_in(ids.clone()))
            .exec(&transaction)
            .await?;
        self.create_wiki_audit_in_transaction(
            &transaction,
            &project.id,
            &model.id,
            model.current_version_id.as_deref(),
            match kind {
                WikiNodeKind::Folder => "folder_deleted",
                WikiNodeKind::Page => "page_deleted",
            },
            &actor_kind,
            actor_id.as_deref(),
            Some(&format!("Deleted {} wiki node(s)", ids.len())),
            now,
        )
        .await?;
        transaction.commit().await?;

        let model = wiki_node::Entity::find_by_id(model.id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(model.id.clone()))?;
        self.hydrate_wiki_node(model).await
    }

    pub async fn list_wiki_versions(
        &self,
        project_locator: &str,
        page_locator: &str,
    ) -> ServiceResult<Vec<WikiPageVersion>> {
        let project = self.resolve_project_locator(project_locator).await?;
        let page = self
            .resolve_wiki_page(&project.id, page_locator, true)
            .await?;
        let versions = wiki_page_version::Entity::find()
            .filter(wiki_page_version::Column::PageId.eq(page.id))
            .order_by_desc(wiki_page_version::Column::Version)
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_wiki_page_version)
            .collect();
        Ok(versions)
    }

    pub async fn get_wiki_version(
        &self,
        project_locator: &str,
        page_locator: &str,
        version: i64,
    ) -> ServiceResult<WikiPageVersion> {
        let project = self.resolve_project_locator(project_locator).await?;
        let page = self
            .resolve_wiki_page(&project.id, page_locator, true)
            .await?;
        wiki_page_version::Entity::find()
            .filter(wiki_page_version::Column::PageId.eq(page.id))
            .filter(wiki_page_version::Column::Version.eq(version))
            .one(&self.db)
            .await?
            .map(model_to_wiki_page_version)
            .ok_or_else(|| ServiceError::WikiVersionNotFound(version.to_string()))
    }

    pub async fn rollback_wiki_page(
        &self,
        project_locator: &str,
        page_locator: &str,
        input: RollbackWikiPageInput,
    ) -> ServiceResult<WikiNode> {
        let project = self.resolve_project_locator(project_locator).await?;
        let page = self
            .resolve_wiki_page(&project.id, page_locator, false)
            .await?;
        let target = wiki_page_version::Entity::find()
            .filter(wiki_page_version::Column::PageId.eq(page.id.clone()))
            .filter(wiki_page_version::Column::Version.eq(input.version))
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::WikiVersionNotFound(input.version.to_string()))?;
        let next_slug = normalize_slug(&target.title)?;
        self.ensure_wiki_sibling_available(
            &project.id,
            page.parent_id.as_deref(),
            &next_slug,
            Some(page.id.as_str()),
        )
        .await?;

        let actor_kind = normalize_actor_kind(input.actor_kind)?;
        let actor_id = optional_non_empty(input.actor_id);
        let summary = optional_non_empty(input.summary)
            .or_else(|| Some(format!("Rolled back to version {}", target.version)));
        let now = Utc::now();
        let next_version = self.next_wiki_version_number(&page.id).await?;

        let transaction = self.db.begin().await?;
        let version = wiki_page_version::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            page_id: Set(page.id.clone()),
            version: Set(next_version),
            title: Set(target.title.clone()),
            content: Set(target.content.clone()),
            actor_kind: Set(actor_kind.clone()),
            actor_id: Set(actor_id.clone()),
            summary: Set(summary.clone()),
            created_at: Set(now),
        }
        .insert(&transaction)
        .await?;

        let mut active = page.into_active_model();
        active.title = Set(target.title);
        active.slug = Set(next_slug);
        active.content = Set(Some(target.content));
        active.current_version_id = Set(Some(version.id.clone()));
        active.updated_at = Set(now);
        active.update(&transaction).await?;

        self.create_wiki_audit_in_transaction(
            &transaction,
            &project.id,
            &version.page_id,
            Some(&version.id),
            "page_rolled_back",
            &actor_kind,
            actor_id.as_deref(),
            summary.as_deref(),
            now,
        )
        .await?;
        transaction.commit().await?;

        let model = wiki_node::Entity::find_by_id(version.page_id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(version.page_id.clone()))?;
        self.hydrate_wiki_node(model).await
    }

    pub async fn list_wiki_audits(
        &self,
        project_locator: &str,
        filter: WikiAuditFilter,
    ) -> ServiceResult<Vec<WikiAudit>> {
        let project = self.resolve_project_locator(project_locator).await?;
        let mut query =
            wiki_audit::Entity::find().filter(wiki_audit::Column::ProjectId.eq(project.id.clone()));

        if let Some(node_id) = filter.node_id.as_deref().and_then(non_empty_str) {
            let node = self
                .find_wiki_node_model(&project.id, node_id, true)
                .await?
                .ok_or_else(|| ServiceError::WikiNodeNotFound(node_id.to_string()))?;
            query = query.filter(wiki_audit::Column::NodeId.eq(node.id));
        }
        if let Some(actor_kind) = filter.actor_kind.as_deref().and_then(non_empty_str) {
            query = query.filter(wiki_audit::Column::ActorKind.eq(actor_kind.to_ascii_lowercase()));
        }
        if let Some(limit) = filter.limit {
            query = query.limit(limit.clamp(1, 100));
        }

        Ok(query
            .order_by_desc(wiki_audit::Column::CreatedAt)
            .all(&self.db)
            .await?
            .into_iter()
            .map(model_to_wiki_audit)
            .collect())
    }

    pub async fn list_issues(&self, filter: IssueFilter) -> ServiceResult<Vec<Issue>> {
        let mut query = issue::Entity::find();

        if let Some(project_slug) = filter.project_slug.as_deref().and_then(non_empty_str) {
            query = query.filter(issue::Column::ProjectSlug.eq(project_slug.to_string()));
        }

        if let Some(project_id) = filter.project_id.as_deref().and_then(non_empty_str) {
            query = query.filter(issue::Column::ProjectId.eq(project_id.to_string()));
        }

        if let Some(assignee_id) = filter.assignee_id.as_deref().and_then(non_empty_str) {
            query = query.filter(issue::Column::AssigneeId.eq(assignee_id.to_string()));
        }

        let states = normalize_list(filter.states);
        if !states.is_empty() {
            query = query.filter(issue::Column::State.is_in(states));
        }

        let ids = normalize_list(filter.ids);
        if !ids.is_empty() {
            query = query.filter(issue::Column::Id.is_in(ids.clone()));
        }

        let models = query
            .order_by_asc(issue::Column::CreatedAt)
            .order_by_asc(issue::Column::Identifier)
            .all(&self.db)
            .await?;

        let mut issues = self.hydrate_issues(models).await?;
        if !ids.is_empty() {
            let index = ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id, index))
                .collect::<HashMap<_, _>>();
            issues.sort_by_key(|issue| index.get(&issue.id).copied().unwrap_or(usize::MAX));
        }
        Ok(issues)
    }

    pub async fn find_issue(&self, locator: &str) -> ServiceResult<Option<Issue>> {
        let Some(id) = self.find_issue_id(locator).await? else {
            return Ok(None);
        };
        self.find_issue_by_id(&id).await
    }

    pub async fn delete_issue(&self, locator: &str) -> ServiceResult<Issue> {
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let issue = self
            .find_issue_by_id(&issue_id)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(issue_id.clone()))?;

        let transaction = self.db.begin().await?;
        comment::Entity::delete_many()
            .filter(comment::Column::IssueId.eq(issue_id.clone()))
            .exec(&transaction)
            .await?;
        issue_label::Entity::delete_many()
            .filter(issue_label::Column::IssueId.eq(issue_id.clone()))
            .exec(&transaction)
            .await?;
        issue_relation::Entity::delete_many()
            .filter(
                Condition::any()
                    .add(issue_relation::Column::IssueId.eq(issue_id.clone()))
                    .add(issue_relation::Column::BlockedByIssueId.eq(issue_id.clone())),
            )
            .exec(&transaction)
            .await?;
        notification::Entity::delete_many()
            .filter(notification::Column::IssueId.eq(issue_id.clone()))
            .exec(&transaction)
            .await?;
        issue::Entity::delete_by_id(issue_id)
            .exec(&transaction)
            .await?;
        transaction.commit().await?;

        Ok(issue)
    }

    pub async fn update_issue_state(&self, locator: &str, state: String) -> ServiceResult<Issue> {
        let state = require_non_empty("state", Some(state))?;
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let model = issue::Entity::find_by_id(issue_id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let mut active = model.into_active_model();
        active.state = Set(state);
        active.updated_at = Set(Utc::now());
        active.update(&self.db).await?;

        let issue = self
            .find_issue_by_id(&issue_id)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(issue_id.clone()))?;
        self.create_activity_internal(
            &issue,
            "issue_state_changed",
            format!("{} moved to {}", issue.identifier, issue.state),
            Some(issue.title.clone()),
        )
        .await?;
        self.upsert_notification(
            &issue,
            "issue_updated",
            format!("{} moved to {}", issue.identifier, issue.state),
            Some(issue.title.clone()),
        )
        .await?;
        Ok(issue)
    }

    pub async fn update_issue(
        &self,
        locator: &str,
        input: UpdateIssueInput,
    ) -> ServiceResult<Issue> {
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let blocked_by_ids = match input.blocked_by {
            Some(blocked_by) => Some(self.resolve_issue_locators(&blocked_by).await?),
            None => None,
        };
        let project_model = match input.project_id.as_ref() {
            Some(Some(locator)) => Some(Some(self.resolve_project_locator(locator).await?)),
            Some(None) => Some(None),
            None => None,
        };

        if let Some(blocked_by_ids) = blocked_by_ids.as_ref() {
            if blocked_by_ids
                .iter()
                .any(|blocker_id| blocker_id == &issue_id)
            {
                return Err(ServiceError::InvalidInput(
                    "issue cannot be blocked by itself".to_string(),
                ));
            }
        }

        let model = issue::Entity::find_by_id(issue_id.clone())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let now = Utc::now();
        let transaction = self.db.begin().await?;
        let mut active = model.into_active_model();
        if let Some(title) = input.title {
            active.title = Set(title);
        }
        if let Some(description) = input.description {
            active.description = Set(description);
        }
        if let Some(priority) = input.priority {
            active.priority = Set(priority);
        }
        if let Some(project_model) = project_model {
            match project_model {
                Some(project) => {
                    active.project_id = Set(Some(project.id));
                    active.project_slug = Set(project.slug);
                }
                None => {
                    active.project_id = Set(None);
                    active.project_slug = Set("default".to_string());
                }
            }
        }
        active.updated_at = Set(now);
        active.update(&transaction).await?;

        if let Some(blocked_by_ids) = blocked_by_ids {
            issue_relation::Entity::delete_many()
                .filter(issue_relation::Column::IssueId.eq(issue_id.clone()))
                .exec(&transaction)
                .await?;

            for blocked_by_issue_id in blocked_by_ids {
                issue_relation::ActiveModel {
                    id: Set(Uuid::new_v4().to_string()),
                    issue_id: Set(issue_id.clone()),
                    blocked_by_issue_id: Set(blocked_by_issue_id),
                }
                .insert(&transaction)
                .await?;
            }
        }

        transaction.commit().await?;
        let issue = self
            .find_issue_by_id(&issue_id)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(issue_id.clone()))?;
        self.create_activity_internal(
            &issue,
            "issue_updated",
            format!("{} updated", issue.identifier),
            Some(issue.title.clone()),
        )
        .await?;
        self.upsert_notification(
            &issue,
            "issue_updated",
            format!("{} updated", issue.identifier),
            Some(issue.title.clone()),
        )
        .await?;
        Ok(issue)
    }

    pub async fn create_comment(&self, locator: &str, body: String) -> ServiceResult<Comment> {
        let body = require_non_empty("body", Some(body))?;
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;

        let now = Utc::now();
        let model = comment::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            issue_id: Set(issue_id.clone()),
            body: Set(body),
            created_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        self.touch_issue(&issue_id, now).await?;
        let issue = self
            .find_issue_by_id(&issue_id)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(issue_id.clone()))?;
        self.create_activity_internal(
            &issue,
            "comment_created",
            format!("New comment on {}", issue.identifier),
            Some(model.body.clone()),
        )
        .await?;
        self.upsert_notification(
            &issue,
            "comment_created",
            format!("New comment on {}", issue.identifier),
            Some(model.body.clone()),
        )
        .await?;

        if issue.state == IssueState::Done.to_string() {
            self.update_issue_state(locator, IssueState::InProgress.to_string()).await?;
        }

        Ok(model.into())
    }

    pub async fn list_comments(&self, locator: &str) -> ServiceResult<Vec<Comment>> {
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;

        let comments = comment::Entity::find()
            .filter(comment::Column::IssueId.eq(issue_id))
            .order_by_asc(comment::Column::CreatedAt)
            .all(&self.db)
            .await?;

        Ok(comments.into_iter().map(Into::into).collect())
    }

    pub async fn list_notifications(
        &self,
        filter: NotificationFilter,
    ) -> ServiceResult<Vec<Notification>> {
        let mut query = notification::Entity::find();

        if !filter.include_archived {
            query = query.filter(notification::Column::ArchivedAt.is_null());
        }

        if filter.unread_only {
            query = query.filter(notification::Column::ReadAt.is_null());
        }

        if let Some(recipient_id) = filter.recipient_id.as_deref().and_then(non_empty_str) {
            query = query.filter(notification::Column::RecipientId.eq(recipient_id.to_string()));
        }

        if let Some(issue_id) = filter.issue_id.as_deref().and_then(non_empty_str) {
            query = query.filter(notification::Column::IssueId.eq(issue_id.to_string()));
        }

        if let Some(limit) = filter.limit {
            query = query.limit(limit.clamp(1, 100));
        }

        let models = query
            .order_by_desc(notification::Column::CreatedAt)
            .all(&self.db)
            .await?;

        self.hydrate_notifications(models).await
    }

    pub async fn count_notifications(&self, filter: NotificationFilter) -> ServiceResult<u64> {
        let mut query = notification::Entity::find();

        if !filter.include_archived {
            query = query.filter(notification::Column::ArchivedAt.is_null());
        }

        if filter.unread_only {
            query = query.filter(notification::Column::ReadAt.is_null());
        }

        if let Some(recipient_id) = filter.recipient_id.as_deref().and_then(non_empty_str) {
            query = query.filter(notification::Column::RecipientId.eq(recipient_id.to_string()));
        }

        if let Some(issue_id) = filter.issue_id.as_deref().and_then(non_empty_str) {
            query = query.filter(notification::Column::IssueId.eq(issue_id.to_string()));
        }

        Ok(query.count(&self.db).await?)
    }

    pub async fn mark_notification_read(&self, id: &str) -> ServiceResult<Notification> {
        let now = Utc::now();
        let model = notification::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotificationNotFound(id.to_string()))?;
        let mut active = model.into_active_model();
        active.read_at = Set(Some(now));
        active.updated_at = Set(now);
        let model = active.update(&self.db).await?;
        self.hydrate_notification(model).await
    }

    pub async fn mark_notification_unread(&self, id: &str) -> ServiceResult<Notification> {
        let model = notification::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotificationNotFound(id.to_string()))?;
        let mut active = model.into_active_model();
        active.read_at = Set(None);
        let model = active.update(&self.db).await?;
        self.hydrate_notification(model).await
    }

    pub async fn archive_notification(&self, id: &str) -> ServiceResult<Notification> {
        let now = Utc::now();
        let model = notification::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::NotificationNotFound(id.to_string()))?;
        let read_at = model.read_at.unwrap_or(now);
        let mut active = model.into_active_model();
        active.archived_at = Set(Some(now));
        active.read_at = Set(Some(read_at));
        active.updated_at = Set(now);
        let model = active.update(&self.db).await?;
        self.hydrate_notification(model).await
    }

    async fn resolve_wiki_parent(
        &self,
        project_id: &str,
        parent_id: Option<&str>,
    ) -> ServiceResult<Option<String>> {
        let Some(parent_id) = parent_id.and_then(non_empty_str) else {
            return Ok(None);
        };
        let parent = self
            .find_wiki_node_model(project_id, parent_id, false)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(parent_id.to_string()))?;
        let kind = parse_wiki_kind(&parent.kind)?;
        if kind != WikiNodeKind::Folder {
            return Err(ServiceError::InvalidInput(
                "wiki parent must be a folder".to_string(),
            ));
        }
        Ok(Some(parent.id))
    }

    async fn resolve_wiki_page(
        &self,
        project_id: &str,
        page_locator: &str,
        include_deleted: bool,
    ) -> ServiceResult<wiki_node::Model> {
        let page = self
            .find_wiki_node_model(project_id, page_locator, include_deleted)
            .await?
            .ok_or_else(|| ServiceError::WikiNodeNotFound(page_locator.to_string()))?;
        let kind = parse_wiki_kind(&page.kind)?;
        if kind != WikiNodeKind::Page {
            return Err(ServiceError::InvalidInput(
                "wiki node is not a page".to_string(),
            ));
        }
        Ok(page)
    }

    async fn find_wiki_node_model(
        &self,
        project_id: &str,
        locator: &str,
        include_deleted: bool,
    ) -> ServiceResult<Option<wiki_node::Model>> {
        let locator = locator.trim();
        if locator.is_empty() {
            return Ok(None);
        }

        let mut query = wiki_node::Entity::find()
            .filter(wiki_node::Column::ProjectId.eq(project_id.to_string()));
        if !include_deleted {
            query = query.filter(wiki_node::Column::DeletedAt.is_null());
        }

        let models = query.all(&self.db).await?;
        if let Some(model) = models.iter().find(|model| model.id == locator) {
            return Ok(Some(model.clone()));
        }

        let mut slug_matches = models
            .into_iter()
            .filter(|model| {
                let candidate = model_to_wiki_node(model.clone(), None);
                wiki_node_matches_locator(&candidate, locator)
            })
            .collect::<Vec<_>>();
        if slug_matches.len() > 1 {
            return Err(ServiceError::InvalidInput(format!(
                "ambiguous wiki node locator: {locator}"
            )));
        }
        Ok(slug_matches.pop())
    }

    async fn ensure_wiki_sibling_available(
        &self,
        project_id: &str,
        parent_id: Option<&str>,
        slug: &str,
        excluding_id: Option<&str>,
    ) -> ServiceResult<()> {
        let mut query = wiki_node::Entity::find()
            .filter(wiki_node::Column::ProjectId.eq(project_id.to_string()))
            .filter(wiki_node::Column::Slug.eq(slug.to_string()))
            .filter(wiki_node::Column::DeletedAt.is_null());
        query = match parent_id {
            Some(parent_id) => query.filter(wiki_node::Column::ParentId.eq(parent_id.to_string())),
            None => query.filter(wiki_node::Column::ParentId.is_null()),
        };
        if let Some(excluding_id) = excluding_id {
            query = query.filter(wiki_node::Column::Id.ne(excluding_id.to_string()));
        }
        if query.one(&self.db).await?.is_some() {
            return Err(ServiceError::InvalidInput(format!(
                "wiki node already exists in this folder: {slug}"
            )));
        }
        Ok(())
    }

    async fn next_wiki_version_number(&self, page_id: &str) -> ServiceResult<i64> {
        let latest = wiki_page_version::Entity::find()
            .filter(wiki_page_version::Column::PageId.eq(page_id.to_string()))
            .order_by_desc(wiki_page_version::Column::Version)
            .one(&self.db)
            .await?;
        Ok(latest.map(|version| version.version + 1).unwrap_or(1))
    }

    async fn collect_wiki_descendant_ids(
        &self,
        project_id: &str,
        root_id: &str,
    ) -> ServiceResult<Vec<String>> {
        let mut ids = vec![root_id.to_string()];
        let mut frontier = vec![root_id.to_string()];

        while !frontier.is_empty() {
            let children = wiki_node::Entity::find()
                .filter(wiki_node::Column::ProjectId.eq(project_id.to_string()))
                .filter(wiki_node::Column::ParentId.is_in(frontier.clone()))
                .filter(wiki_node::Column::DeletedAt.is_null())
                .all(&self.db)
                .await?;
            frontier = children
                .iter()
                .map(|child| child.id.clone())
                .collect::<Vec<_>>();
            ids.extend(frontier.iter().cloned());
        }

        Ok(ids)
    }

    async fn hydrate_wiki_nodes(
        &self,
        models: Vec<wiki_node::Model>,
    ) -> ServiceResult<Vec<WikiNode>> {
        let mut nodes = Vec::with_capacity(models.len());
        for model in models {
            nodes.push(self.hydrate_wiki_node(model).await?);
        }
        Ok(nodes)
    }

    async fn hydrate_wiki_node(&self, model: wiki_node::Model) -> ServiceResult<WikiNode> {
        let current_version = match model.current_version_id.as_deref() {
            Some(version_id) => wiki_page_version::Entity::find_by_id(version_id.to_string())
                .one(&self.db)
                .await?
                .map(|version| WikiVersionRef {
                    id: version.id,
                    version: version.version,
                    created_at: Some(version.created_at),
                }),
            None => None,
        };

        Ok(model_to_wiki_node(model, current_version))
    }

    async fn create_wiki_audit_in_transaction(
        &self,
        transaction: &DatabaseTransaction,
        project_id: &str,
        node_id: &str,
        version_id: Option<&str>,
        action: &str,
        actor_kind: &str,
        actor_id: Option<&str>,
        summary: Option<&str>,
        created_at: DateTime<Utc>,
    ) -> ServiceResult<wiki_audit::Model> {
        Ok(wiki_audit::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            project_id: Set(project_id.to_string()),
            node_id: Set(node_id.to_string()),
            version_id: Set(version_id.map(ToString::to_string)),
            action: Set(action.to_string()),
            actor_kind: Set(actor_kind.to_string()),
            actor_id: Set(actor_id.map(ToString::to_string)),
            summary: Set(summary.map(ToString::to_string)),
            created_at: Set(created_at),
        }
        .insert(transaction)
        .await?)
    }

    async fn next_issue_number(&self, team_key: &str) -> ServiceResult<i64> {
        let latest = issue::Entity::find()
            .filter(issue::Column::TeamKey.eq(team_key.to_string()))
            .order_by_desc(issue::Column::Number)
            .one(&self.db)
            .await?;

        Ok(latest.map(|issue| issue.number + 1).unwrap_or(1))
    }

    async fn ensure_project_for_issue(
        &self,
        project_id: Option<&str>,
        project_slug: Option<&str>,
    ) -> ServiceResult<Option<project::Model>> {
        if let Some(locator) = project_id.and_then(non_empty_str) {
            return self.resolve_project_locator(locator).await.map(Some);
        }

        if let Some(slug) = project_slug.and_then(non_empty_str) {
            return self.find_or_create_project_for_slug(slug).await.map(Some);
        }

        Ok(None)
    }

    async fn resolve_project_locator(&self, locator: &str) -> ServiceResult<project::Model> {
        self.find_project_model(locator)
            .await?
            .ok_or_else(|| ServiceError::ProjectNotFound(locator.to_string()))
    }

    async fn find_or_create_project_for_slug(&self, slug: &str) -> ServiceResult<project::Model> {
        let slug = normalize_slug(slug)?;
        if let Some(project) = self.find_project_model_by_slug(&slug).await? {
            return Ok(project);
        }

        let now = Utc::now();
        let model = project::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            slug: Set(slug.clone()),
            name: Set(slug.clone()),
            description: Set(None),
            priority: Set(None),
            state: Set(IssueState::Backlog.to_string()),
            url: Set(Some(format!(
                "/api/projects/{}",
                url_safe_identifier(&slug)
            ))),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        Ok(model)
    }

    async fn find_project_id(&self, locator: &str) -> ServiceResult<Option<String>> {
        Ok(self
            .find_project_model(locator)
            .await?
            .map(|project| project.id))
    }

    async fn find_project_model(&self, locator: &str) -> ServiceResult<Option<project::Model>> {
        let locator = locator.trim();
        if locator.is_empty() {
            return Ok(None);
        }

        let models = project::Entity::find().all(&self.db).await?;
        Ok(models.into_iter().find(|model| {
            let candidate = model_to_project(model.clone());
            project_matches_locator(&candidate, locator)
        }))
    }

    async fn find_project_model_by_slug(
        &self,
        slug: &str,
    ) -> ServiceResult<Option<project::Model>> {
        let slug = normalize_slug(slug)?;
        let models = project::Entity::find().all(&self.db).await?;
        Ok(models
            .into_iter()
            .find(|model| model.slug.eq_ignore_ascii_case(&slug)))
    }

    async fn resolve_issue_locators(&self, locators: &[String]) -> ServiceResult<Vec<String>> {
        let mut ids = Vec::with_capacity(locators.len());
        for locator in locators {
            let id = self
                .find_issue_id(locator)
                .await?
                .ok_or_else(|| ServiceError::IssueNotFound(locator.clone()))?;
            ids.push(id);
        }
        Ok(ids)
    }

    async fn find_issue_id(&self, locator: &str) -> ServiceResult<Option<String>> {
        let locator = locator.trim();
        if locator.is_empty() {
            return Ok(None);
        }

        let models = issue::Entity::find().all(&self.db).await?;
        Ok(models
            .into_iter()
            .map(|model| model_to_issue(model, None, Vec::new(), Vec::new()))
            .find(|issue| issue_matches_locator(issue, locator))
            .map(|issue| issue.id))
    }

    async fn find_issue_by_id(&self, id: &str) -> ServiceResult<Option<Issue>> {
        let Some(model) = issue::Entity::find_by_id(id.to_string())
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };
        Ok(Some(self.hydrate_issue(model).await?))
    }

    async fn hydrate_issues(&self, models: Vec<issue::Model>) -> ServiceResult<Vec<Issue>> {
        let mut issues = Vec::with_capacity(models.len());
        for model in models {
            issues.push(self.hydrate_issue(model).await?);
        }
        Ok(issues)
    }

    async fn hydrate_issue(&self, model: issue::Model) -> ServiceResult<Issue> {
        let labels = issue_label::Entity::find()
            .filter(issue_label::Column::IssueId.eq(model.id.clone()))
            .order_by_asc(issue_label::Column::Name)
            .all(&self.db)
            .await?
            .into_iter()
            .map(|label| label.name)
            .collect::<Vec<_>>();

        let project = match model.project_id.as_deref() {
            Some(project_id) => project::Entity::find_by_id(project_id.to_string())
                .one(&self.db)
                .await?
                .map(model_to_project_ref),
            None => None,
        };

        let relations = issue_relation::Entity::find()
            .filter(issue_relation::Column::IssueId.eq(model.id.clone()))
            .all(&self.db)
            .await?;
        let blocker_ids = relations
            .iter()
            .map(|relation| relation.blocked_by_issue_id.clone())
            .collect::<Vec<_>>();
        let blockers = if blocker_ids.is_empty() {
            Vec::new()
        } else {
            let lookup = blocker_ids.iter().cloned().collect::<HashSet<_>>();
            let models = issue::Entity::find()
                .filter(issue::Column::Id.is_in(lookup))
                .all(&self.db)
                .await?
                .into_iter()
                .map(|issue| (issue.id.clone(), issue))
                .collect::<HashMap<_, _>>();

            blocker_ids
                .iter()
                .filter_map(|id| models.get(id))
                .map(|issue| BlockerRef {
                    id: Some(issue.id.clone()),
                    identifier: Some(issue.identifier.clone()),
                    state: Some(issue.state.clone()),
                })
                .collect()
        };

        Ok(model_to_issue(model, project, labels, blockers))
    }

    pub async fn list_activities(&self, locator: &str) -> ServiceResult<Vec<Activity>> {
        let issue_id = self
            .find_issue_id(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let models = activity::Entity::find()
            .filter(activity::Column::IssueId.eq(issue_id))
            .order_by_desc(activity::Column::CreatedAt)
            .all(&self.db)
            .await?;
        Ok(models.into_iter().map(model_to_activity).collect())
    }

    pub async fn create_activity(
        &self,
        locator: &str,
        kind: String,
        title: String,
        body: Option<String>,
    ) -> ServiceResult<Activity> {
        let issue = self
            .find_issue(locator)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(locator.to_string()))?;
        let now = Utc::now();
        let model = activity::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            issue_id: Set(Some(issue.id.clone())),
            kind: Set(kind),
            actor_id: Set(None),
            title: Set(title),
            body: Set(body),
            created_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        Ok(model_to_activity(model))
    }

    async fn create_activity_internal(
        &self,
        issue: &Issue,
        kind: &str,
        title: String,
        body: Option<String>,
    ) -> ServiceResult<Activity> {
        let now = Utc::now();
        let model = activity::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            issue_id: Set(Some(issue.id.clone())),
            kind: Set(kind.to_string()),
            actor_id: Set(None),
            title: Set(title),
            body: Set(body),
            created_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        Ok(model_to_activity(model))
    }

    async fn upsert_notification(
        &self,
        issue: &Issue,
        kind: &str,
        title: String,
        body: Option<String>,
    ) -> ServiceResult<Notification> {
        let now = Utc::now();

        if let Some(existing) = notification::Entity::find()
            .filter(notification::Column::IssueId.eq(issue.id.clone()))
            .filter(notification::Column::ArchivedAt.is_null())
            .one(&self.db)
            .await?
        {
            let mut active = existing.into_active_model();
            active.kind = Set(kind.to_string());
            active.title = Set(title);
            active.body = Set(body);
            active.read_at = Set(None);
            active.updated_at = Set(now);
            let model = active.update(&self.db).await?;
            return self.hydrate_notification(model).await;
        }

        let model = notification::ActiveModel {
            id: Set(Uuid::new_v4().to_string()),
            kind: Set(kind.to_string()),
            issue_id: Set(Some(issue.id.clone())),
            recipient_id: Set(None),
            actor_id: Set(None),
            title: Set(title),
            body: Set(body),
            read_at: Set(None),
            archived_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await?;

        self.hydrate_notification(model).await
    }

    async fn touch_issue(&self, issue_id: &str, updated_at: DateTime<Utc>) -> ServiceResult<()> {
        let model = issue::Entity::find_by_id(issue_id.to_string())
            .one(&self.db)
            .await?
            .ok_or_else(|| ServiceError::IssueNotFound(issue_id.to_string()))?;
        let mut active = model.into_active_model();
        active.updated_at = Set(updated_at);
        active.update(&self.db).await?;
        Ok(())
    }

    async fn hydrate_notifications(
        &self,
        models: Vec<notification::Model>,
    ) -> ServiceResult<Vec<Notification>> {
        let mut notifications = Vec::with_capacity(models.len());
        for model in models {
            notifications.push(self.hydrate_notification(model).await?);
        }
        Ok(notifications)
    }

    async fn hydrate_notification(
        &self,
        model: notification::Model,
    ) -> ServiceResult<Notification> {
        let issue = match model.issue_id.as_deref() {
            Some(issue_id) => issue::Entity::find_by_id(issue_id.to_string())
                .one(&self.db)
                .await?
                .map(notification_issue_ref),
            None => None,
        };

        Ok(model_to_notification(model, issue))
    }
}

#[derive(Clone, Debug, Default)]
pub struct IssueFilter {
    pub project_slug: Option<String>,
    pub project_id: Option<String>,
    pub states: Vec<String>,
    pub ids: Vec<String>,
    pub assignee_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectFilter {
    pub states: Vec<String>,
    pub ids: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct WikiNodeFilter {
    pub parent_id: Option<String>,
    pub include_deleted: bool,
    pub recursive: bool,
}

#[derive(Clone, Debug, Default)]
pub struct WikiAuditFilter {
    pub node_id: Option<String>,
    pub actor_kind: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct NotificationFilter {
    pub include_archived: bool,
    pub unread_only: bool,
    pub recipient_id: Option<String>,
    pub issue_id: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateIssueInput {
    pub title: Option<String>,
    pub project_id: Option<Option<String>>,
    pub description: Option<Option<String>>,
    pub priority: Option<Option<i64>>,
    pub blocked_by: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateProjectInput {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub priority: Option<Option<i64>>,
}

#[derive(Clone, Debug, Default)]
pub struct CreateWikiNodeInput {
    pub parent_id: Option<String>,
    pub kind: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateWikiNodeInput {
    pub parent_id: Option<Option<String>>,
    pub title: Option<String>,
    pub content: Option<Option<String>>,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RollbackWikiPageInput {
    pub version: i64,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct CreateIssueInput {
    pub project_id: Option<String>,
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

#[derive(Clone, Debug, Default)]
pub struct CreateProjectInput {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: Option<String>,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid_input: {0}")]
    InvalidInput(String),
    #[error("issue_not_found: {0}")]
    IssueNotFound(String),
    #[error("project_not_found: {0}")]
    ProjectNotFound(String),
    #[error("wiki_node_not_found: {0}")]
    WikiNodeNotFound(String),
    #[error("wiki_version_not_found: {0}")]
    WikiVersionNotFound(String),
    #[error("notification_not_found: {0}")]
    NotificationNotFound(String),
    #[error("database error: {0}")]
    Database(#[from] DbErr),
}

pub type ServiceResult<T> = Result<T, ServiceError>;

impl From<comment::Model> for Comment {
    fn from(model: comment::Model) -> Self {
        Self {
            id: model.id,
            issue_id: model.issue_id,
            body: model.body,
            created_at: model.created_at,
        }
    }
}

fn model_to_issue(
    model: issue::Model,
    project: Option<ProjectRef>,
    labels: Vec<String>,
    blocked_by: Vec<BlockerRef>,
) -> Issue {
    Issue {
        id: model.id,
        identifier: model.identifier,
        project_id: model.project_id,
        project,
        title: model.title,
        description: model.description,
        priority: model.priority,
        state: model.state,
        branch_name: model.branch_name,
        url: model.url,
        labels,
        blocked_by,
        created_at: Some(model.created_at),
        updated_at: Some(model.updated_at),
    }
}

fn model_to_project(model: project::Model) -> Project {
    Project {
        id: model.id,
        slug: model.slug,
        name: model.name,
        description: model.description,
        priority: model.priority,
        state: model.state,
        url: model.url,
        created_at: Some(model.created_at),
        updated_at: Some(model.updated_at),
    }
}

fn model_to_project_ref(model: project::Model) -> ProjectRef {
    ProjectRef {
        id: model.id,
        slug: model.slug,
        name: model.name,
        state: model.state,
        priority: model.priority,
    }
}

fn model_to_wiki_node(
    model: wiki_node::Model,
    current_version: Option<WikiVersionRef>,
) -> WikiNode {
    let kind = WikiNodeKind::parse(&model.kind).unwrap_or(WikiNodeKind::Page);
    WikiNode {
        id: model.id,
        project_id: model.project_id,
        parent_id: model.parent_id,
        kind,
        title: model.title,
        slug: model.slug,
        content: model.content,
        current_version,
        created_at: Some(model.created_at),
        updated_at: Some(model.updated_at),
        deleted_at: model.deleted_at,
    }
}

fn model_to_wiki_page_version(model: wiki_page_version::Model) -> WikiPageVersion {
    WikiPageVersion {
        id: model.id,
        page_id: model.page_id,
        version: model.version,
        title: model.title,
        content: model.content,
        actor_kind: model.actor_kind,
        actor_id: model.actor_id,
        summary: model.summary,
        created_at: Some(model.created_at),
    }
}

fn model_to_wiki_audit(model: wiki_audit::Model) -> WikiAudit {
    WikiAudit {
        id: model.id,
        project_id: model.project_id,
        node_id: model.node_id,
        version_id: model.version_id,
        action: model.action,
        actor_kind: model.actor_kind,
        actor_id: model.actor_id,
        summary: model.summary,
        created_at: Some(model.created_at),
    }
}

fn notification_issue_ref(model: issue::Model) -> NotificationIssueRef {
    NotificationIssueRef {
        id: model.id,
        identifier: model.identifier,
        title: model.title,
        state: model.state,
        priority: model.priority,
        updated_at: Some(model.updated_at),
    }
}

fn model_to_notification(
    model: notification::Model,
    issue: Option<NotificationIssueRef>,
) -> Notification {
    Notification {
        id: model.id,
        kind: model.kind,
        issue_id: model.issue_id,
        issue,
        recipient_id: model.recipient_id,
        actor_id: model.actor_id,
        title: model.title,
        body: model.body,
        read_at: model.read_at,
        archived_at: model.archived_at,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}

fn model_to_activity(model: activity::Model) -> Activity {
    Activity {
        id: model.id,
        issue_id: model.issue_id,
        kind: model.kind,
        actor_id: model.actor_id,
        title: model.title,
        body: model.body,
        created_at: model.created_at,
    }
}

fn require_non_empty(name: &str, value: Option<String>) -> ServiceResult<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ServiceError::InvalidInput(format!("{name} is required")))
}

fn parse_wiki_kind(value: &str) -> ServiceResult<WikiNodeKind> {
    WikiNodeKind::parse(value)
        .ok_or_else(|| ServiceError::InvalidInput("wiki kind must be folder or page".to_string()))
}

fn normalize_actor_kind(value: Option<String>) -> ServiceResult<String> {
    let actor_kind = non_empty_or(value, "system").to_ascii_lowercase();
    if matches!(actor_kind.as_str(), "human" | "agent" | "system") {
        Ok(actor_kind)
    } else {
        Err(ServiceError::InvalidInput(
            "actor_kind must be human, agent, or system".to_string(),
        ))
    }
}

fn optional_non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn non_empty_or(value: Option<String>, fallback: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn normalize_slug(value: &str) -> ServiceResult<String> {
    let mut slug = String::new();
    let mut previous_separator = false;

    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_separator = false;
        } else if matches!(ch, '.' | '_' | '-') {
            slug.push(ch);
            previous_separator = matches!(ch, '-' | '_');
        } else if !previous_separator {
            slug.push('-');
            previous_separator = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        Err(ServiceError::InvalidInput(
            "project slug is required".to_string(),
        ))
    } else {
        Ok(slug)
    }
}

fn non_empty_str(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_list(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn url_safe_identifier(identifier: &str) -> String {
    identifier.replace(' ', "%20")
}
