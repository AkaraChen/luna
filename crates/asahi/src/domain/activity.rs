use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Activity {
    pub id: String,
    pub issue_id: Option<String>,
    pub kind: String,
    pub actor_id: Option<String>,
    pub title: String,
    pub body: Option<String>,
    pub created_at: DateTime<Utc>,
}
