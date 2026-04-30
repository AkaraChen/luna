use rocket::{FromForm, Route, State, get, patch, routes, serde::json::Json};
use serde::{Deserialize, Serialize};

use crate::{
    api::error::ApiError,
    domain::Notification,
    service::{IssueService, NotificationFilter},
};

#[derive(Debug, FromForm)]
pub struct ListNotificationsQuery {
    include_archived: Option<bool>,
    unread_only: Option<bool>,
    recipient_id: Option<String>,
    issue_id: Option<String>,
    limit: Option<u64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct NotificationListResponse {
    pub notifications: Vec<Notification>,
    pub unread_count: u64,
}

#[get("/notifications?<query..>")]
async fn list_notifications(
    query: ListNotificationsQuery,
    service: &State<IssueService>,
) -> Result<Json<NotificationListResponse>, ApiError> {
    let include_archived = query.include_archived.unwrap_or(false);
    let recipient_id = query.recipient_id.clone();
    let issue_id = query.issue_id.clone();
    let notifications = service
        .list_notifications(NotificationFilter {
            include_archived,
            unread_only: query.unread_only.unwrap_or(false),
            recipient_id: query.recipient_id,
            issue_id: query.issue_id,
            limit: query.limit,
        })
        .await?;
    let unread_count = service
        .count_notifications(NotificationFilter {
            include_archived,
            unread_only: true,
            recipient_id,
            issue_id,
            limit: None,
        })
        .await?;

    Ok(Json(NotificationListResponse {
        notifications,
        unread_count,
    }))
}

#[patch("/notifications/<id>/read")]
async fn mark_notification_read(
    id: &str,
    service: &State<IssueService>,
) -> Result<Json<Notification>, ApiError> {
    Ok(Json(service.mark_notification_read(id).await?))
}

#[patch("/notifications/<id>/archive")]
async fn archive_notification(
    id: &str,
    service: &State<IssueService>,
) -> Result<Json<Notification>, ApiError> {
    Ok(Json(service.archive_notification(id).await?))
}

pub fn routes() -> Vec<Route> {
    routes![
        list_notifications,
        mark_notification_read,
        archive_notification
    ]
}
