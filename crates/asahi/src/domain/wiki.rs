use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WikiNodeKind {
    Folder,
    Page,
}

impl WikiNodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Folder => "folder",
            Self::Page => "page",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "folder" => Some(Self::Folder),
            "page" => Some(Self::Page),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiVersionRef {
    pub id: String,
    pub version: i64,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiNode {
    pub id: String,
    pub project_id: String,
    pub parent_id: Option<String>,
    pub kind: WikiNodeKind,
    pub title: String,
    pub slug: String,
    pub content: Option<String>,
    pub current_version: Option<WikiVersionRef>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiPageVersion {
    pub id: String,
    pub page_id: String,
    pub version: i64,
    pub title: String,
    pub content: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WikiAudit {
    pub id: String,
    pub project_id: String,
    pub node_id: String,
    pub version_id: Option<String>,
    pub action: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub summary: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
}

pub fn wiki_node_matches_locator(node: &WikiNode, locator: &str) -> bool {
    node.id == locator || node.slug.eq_ignore_ascii_case(locator)
}
