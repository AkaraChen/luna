use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub issue_id: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
}
