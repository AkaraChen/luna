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

#[patch("/notifications/<id>/unread")]
async fn mark_notification_unread(
    id: &str,
    service: &State<IssueService>,
) -> Result<Json<Notification>, ApiError> {
    Ok(Json(service.mark_notification_unread(id).await?))
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
        mark_notification_unread,
        archive_notification
    ]
}

#[cfg(test)]
mod tests {
    use rocket::{
        http::{ContentType, Status},
        local::blocking::Client,
    };

    use crate::{app, domain::Issue};

    use super::NotificationListResponse;

    fn create_issue(client: &Client, title: &str) -> Issue {
        let created = client
            .post("/api/issues")
            .header(ContentType::JSON)
            .body(rocket::serde::json::json!({ "title": title }).to_string())
            .dispatch();
        assert_eq!(created.status(), Status::Ok);
        created.into_json().expect("issue json")
    }

    fn list_notifications(client: &Client, query: &str) -> NotificationListResponse {
        let response = client.get(format!("/api/notifications{query}")).dispatch();
        assert_eq!(response.status(), Status::Ok);
        response.into_json().expect("notifications json")
    }

    #[test]
    fn list_returns_seeded_notifications_and_clamps_limit() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");
        create_issue(&client, "First issue");
        create_issue(&client, "Second issue");

        let all = list_notifications(&client, "?limit=10");
        assert_eq!(all.notifications.len(), 2);
        assert_eq!(all.unread_count, 2);
        assert!(all.notifications.iter().all(|notification| {
            notification.issue.is_some() && notification.read_at.is_none()
        }));

        let clamped = list_notifications(&client, "?limit=0");
        assert_eq!(clamped.notifications.len(), 1);
        assert_eq!(clamped.unread_count, 2);
    }

    #[test]
    fn unread_filter_excludes_read_notifications_and_count_reflects_reads() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");
        create_issue(&client, "First issue");
        create_issue(&client, "Second issue");
        let notifications = list_notifications(&client, "?limit=10");
        let notification_id = notifications.notifications[0].id.clone();

        let read = client
            .patch(format!("/api/notifications/{notification_id}/read"))
            .dispatch();
        assert_eq!(read.status(), Status::Ok);

        let unread = list_notifications(&client, "?unread_only=true&limit=10");
        assert_eq!(unread.notifications.len(), 1);
        assert_eq!(unread.unread_count, 1);
        assert_ne!(unread.notifications[0].id, notification_id);

        let all = list_notifications(&client, "?limit=10");
        assert_eq!(all.notifications.len(), 2);
        assert_eq!(all.unread_count, 1);
    }

    #[test]
    fn read_unread_archive_transitions_round_trip() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");
        create_issue(&client, "Transition issue");
        let notifications = list_notifications(&client, "?limit=10");
        let notification_id = notifications.notifications[0].id.clone();

        let read = client
            .patch(format!("/api/notifications/{notification_id}/read"))
            .dispatch();
        assert_eq!(read.status(), Status::Ok);
        let read: crate::domain::Notification = read.into_json().expect("read notification json");
        assert!(read.read_at.is_some());

        let unread = client
            .patch(format!("/api/notifications/{notification_id}/unread"))
            .dispatch();
        assert_eq!(unread.status(), Status::Ok);
        let unread: crate::domain::Notification =
            unread.into_json().expect("unread notification json");
        assert!(unread.read_at.is_none());

        let archived = client
            .patch(format!("/api/notifications/{notification_id}/archive"))
            .dispatch();
        assert_eq!(archived.status(), Status::Ok);
        let archived: crate::domain::Notification =
            archived.into_json().expect("archived notification json");
        assert!(archived.archived_at.is_some());

        let active = list_notifications(&client, "?limit=10");
        assert!(active.notifications.is_empty());
        assert_eq!(active.unread_count, 0);

        let archived = list_notifications(&client, "?include_archived=true&limit=10");
        assert_eq!(archived.notifications.len(), 1);
        assert_eq!(archived.notifications[0].id, notification_id);
    }

    #[test]
    fn missing_notification_mutations_return_not_found() {
        let client = Client::tracked(app::rocket_with_database_url("sqlite::memory:"))
            .expect("valid rocket instance");

        for route in [
            "/api/notifications/missing/read",
            "/api/notifications/missing/unread",
            "/api/notifications/missing/archive",
        ] {
            let response = client.patch(route).dispatch();
            assert_eq!(response.status(), Status::NotFound);
        }
    }
}
