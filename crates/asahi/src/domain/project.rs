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

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project {
            id: "project-1".to_string(),
            slug: "asahi-web".to_string(),
            name: "Asahi Web".to_string(),
            description: None,
            priority: Some(1),
            state: "Todo".to_string(),
            url: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn project_locator_matches_id_or_slug_case_insensitive() {
        let project = project();
        assert!(project_matches_locator(&project, "project-1"));
        assert!(project_matches_locator(&project, "ASAHI-WEB"));
    }

    #[test]
    fn project_locator_does_not_match_display_name() {
        let project = project();
        assert!(!project_matches_locator(&project, "Asahi Web"));
    }
}
