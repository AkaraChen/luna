use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LunaError>;

#[derive(Debug, Error)]
pub enum LunaError {
    #[error("missing_workflow_file: {0}")]
    MissingWorkflowFile(PathBuf),
    #[error("workflow_parse_error: {0}")]
    WorkflowParseError(String),
    #[error("workflow_front_matter_not_a_map")]
    WorkflowFrontMatterNotAMap,
    #[error("template_parse_error: {0}")]
    TemplateParseError(String),
    #[error("template_render_error: {0}")]
    TemplateRenderError(String),
    #[error("unsupported_tracker_kind: {0}")]
    UnsupportedTrackerKind(String),
    #[error("missing_tracker_api_key")]
    MissingTrackerApiKey,
    #[error("missing_tracker_project_slug")]
    MissingTrackerProjectSlug,
    #[error("invalid_config: {0}")]
    InvalidConfig(String),
    #[error("tracker error: {0}")]
    Tracker(String),
    #[error("workspace error: {0}")]
    Workspace(String),
    #[error("agent error: {0}")]
    Agent(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("request error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("bashkit error: {0}")]
    Bashkit(#[from] bashkit::Error),
}
