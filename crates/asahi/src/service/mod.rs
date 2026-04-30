use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait,
    IntoActiveModel, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    domain::{
        Activity, BlockerRef, Comment, Issue, Notification, NotificationIssueRef, default_team_key,
        issue_matches_locator,
    },
    entity::{activity, comment, issue, issue_label, issue_relation, notification},
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
        let project_slug = non_empty_or(input.project_slug, "default");
        let team_key = input
            .team_key
            .map(|value| value.trim().to_ascii_uppercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default_team_key(&project_slug));
        let state = non_empty_or(input.state, "Todo");
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
        self.create_activity(
            &issue,
            "issue_created",
            format!("{} created", issue.identifier),
            Some(issue.title.clone()),
        )
        .await?;
        self.upsert_notification(
            &issue,
            "issue_created",
            format!("{} created", issue.identifier),
            Some(issue.title.clone()),
        )
        .await?;
        Ok(issue)
    }

    pub async fn list_issues(&self, filter: IssueFilter) -> ServiceResult<Vec<Issue>> {
        let mut query = issue::Entity::find();

        if let Some(project_slug) = filter.project_slug.as_deref().and_then(non_empty_str) {
            query = query.filter(issue::Column::ProjectSlug.eq(project_slug.to_string()));
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
        self.create_activity(
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
        if let Some(priority) = input.priority {
            active.priority = Set(priority);
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
        self.create_activity(
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
        self.create_activity(
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

    async fn next_issue_number(&self, team_key: &str) -> ServiceResult<i64> {
        let latest = issue::Entity::find()
            .filter(issue::Column::TeamKey.eq(team_key.to_string()))
            .order_by_desc(issue::Column::Number)
            .one(&self.db)
            .await?;

        Ok(latest.map(|issue| issue.number + 1).unwrap_or(1))
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
            .map(|model| model_to_issue(model, Vec::new(), Vec::new()))
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

        Ok(model_to_issue(model, labels, blockers))
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

    async fn create_activity(
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
    pub states: Vec<String>,
    pub ids: Vec<String>,
    pub assignee_id: Option<String>,
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
    pub priority: Option<Option<i64>>,
    pub blocked_by: Option<Vec<String>>,
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

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("invalid_input: {0}")]
    InvalidInput(String),
    #[error("issue_not_found: {0}")]
    IssueNotFound(String),
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

fn model_to_issue(model: issue::Model, labels: Vec<String>, blocked_by: Vec<BlockerRef>) -> Issue {
    Issue {
        id: model.id,
        identifier: model.identifier,
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

fn non_empty_or(value: Option<String>, fallback: &str) -> String {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| fallback.to_string())
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
