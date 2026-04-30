use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectRef {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub state: String,
    pub priority: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub priority: Option<i64>,
    pub state: String,
    pub url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

pub fn project_matches_locator(project: &Project, locator: &str) -> bool {
    project.id == locator || project.slug.eq_ignore_ascii_case(locator)
}
