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

#[cfg(test)]
mod tests {
    use rocket::serde::json::{from_str, to_string};

    use super::*;

    fn node() -> WikiNode {
        WikiNode {
            id: "node-1".to_string(),
            project_id: "project-1".to_string(),
            parent_id: None,
            kind: WikiNodeKind::Page,
            title: "Design Notes".to_string(),
            slug: "design-notes".to_string(),
            content: Some("content".to_string()),
            current_version: None,
            created_at: None,
            updated_at: None,
            deleted_at: None,
        }
    }

    #[test]
    fn wiki_node_kind_parse_accepts_trimmed_case_insensitive_values() {
        assert_eq!(WikiNodeKind::parse(" folder "), Some(WikiNodeKind::Folder));
        assert_eq!(WikiNodeKind::parse("PAGE"), Some(WikiNodeKind::Page));
    }

    #[test]
    fn wiki_node_kind_parse_rejects_unknown_values() {
        assert_eq!(WikiNodeKind::parse("document"), None);
        assert_eq!(WikiNodeKind::parse(""), None);
    }

    #[test]
    fn wiki_node_kind_serde_uses_snake_case() {
        let encoded = to_string(&WikiNodeKind::Folder).expect("serialize");
        assert_eq!(encoded, r#""folder""#);
        let decoded: WikiNodeKind = from_str(r#""page""#).expect("deserialize");
        assert_eq!(decoded, WikiNodeKind::Page);
    }

    #[test]
    fn wiki_node_locator_matches_id_or_slug_case_insensitive() {
        let node = node();
        assert!(wiki_node_matches_locator(&node, "node-1"));
        assert!(wiki_node_matches_locator(&node, "DESIGN-NOTES"));
        assert!(!wiki_node_matches_locator(&node, "Design Notes"));
    }
}
