use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use garde::Validate;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

use crate::{
    error::{LunaError, Result},
    model::WorkflowDefinition,
    paths::{absolutize_path, normalize_path},
};

// ─── Defaults ───────────────────────────────────────────────────────────────

const DEFAULT_POLL_INTERVAL_MS: u64 = 30_000;
const DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_MAX_CONCURRENT: usize = 10;
const DEFAULT_MAX_TURNS: u32 = 20;
const DEFAULT_MAX_RETRY_BACKOFF_MS: u64 = 300_000;
const DEFAULT_TURN_TIMEOUT_MS: u64 = 3_600_000;
const DEFAULT_READ_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_STALL_TIMEOUT_MS: i64 = 300_000;
const DEFAULT_CODEX_COMMAND: &str = "codex app-server";
const DEFAULT_ACP_COMMAND: &str = "kimi acp";
const DEFAULT_GH_COMMAND: &str = "gh";

// ─── Public Config Types ────────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(tag = "kind")]
pub enum TrackerConfig {
    #[serde(rename = "github_project")]
    GitHubProject(#[garde(dive)] GitHubProjectTrackerConfig),
    #[serde(rename = "linear")]
    Linear(#[garde(dive)] LinearTrackerConfig),
    #[serde(rename = "asahi")]
    Asahi(#[garde(dive)] AsahiTrackerConfig),
}

impl TrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        match self {
            Self::GitHubProject(c) => c.is_active_state(value),
            Self::Linear(c) => c.is_active_state(value),
            Self::Asahi(c) => c.is_active_state(value),
        }
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        match self {
            Self::GitHubProject(c) => c.is_terminal_state(value),
            Self::Linear(c) => c.is_terminal_state(value),
            Self::Asahi(c) => c.is_terminal_state(value),
        }
    }

    pub fn terminal_states(&self) -> &[String] {
        match self {
            Self::GitHubProject(c) => &c.terminal_states,
            Self::Linear(c) => &c.terminal_states,
            Self::Asahi(_) => {
                static STATES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
                STATES.get_or_init(|| vec!["Done".to_string()])
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct GitHubProjectTrackerConfig {
    #[garde(custom(not_blank))]
    pub owner: String,
    #[garde(range(min = 1))]
    pub project_number: u32,
    #[serde(default = "default_github_status_field")]
    #[garde(custom(not_blank))]
    pub status_field: String,
    #[serde(default = "default_github_priority_field")]
    #[garde(custom(not_blank))]
    pub priority_field: String,
    #[serde(default = "default_gh_command")]
    #[garde(custom(not_blank))]
    pub gh_command: String,
    #[serde(default = "default_github_active_states")]
    #[garde(length(min = 1), inner(custom(not_blank)))]
    pub active_states: Vec<String>,
    #[serde(default = "default_github_terminal_states")]
    #[garde(length(min = 1), inner(custom(not_blank)))]
    pub terminal_states: Vec<String>,
    #[serde(skip)]
    #[garde(skip)]
    active_lookup: HashSet<String>,
    #[serde(skip)]
    #[garde(skip)]
    terminal_lookup: HashSet<String>,
}

impl GitHubProjectTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_lookup.contains(&value.to_lowercase())
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LinearTrackerConfig {
    #[serde(default = "default_linear_endpoint")]
    #[garde(custom(not_blank))]
    pub endpoint: String,
    #[garde(inner(custom(not_blank)))]
    pub api_key: Option<String>,
    #[garde(inner(custom(not_blank)))]
    pub project_slug: Option<String>,
    #[garde(inner(custom(not_blank)))]
    pub assignee: Option<String>,
    #[serde(default = "default_linear_active_states")]
    #[garde(length(min = 1), inner(custom(not_blank)))]
    pub active_states: Vec<String>,
    #[serde(default = "default_linear_terminal_states")]
    #[garde(length(min = 1), inner(custom(not_blank)))]
    pub terminal_states: Vec<String>,
    #[serde(skip)]
    #[garde(skip)]
    active_lookup: HashSet<String>,
    #[serde(skip)]
    #[garde(skip)]
    terminal_lookup: HashSet<String>,
}

impl LinearTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_lookup.contains(&value.to_lowercase())
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AsahiTrackerConfig {
    #[serde(default)]
    #[garde(custom(asahi_endpoint_or_db(&self.db)))]
    pub endpoint: String,
    #[garde(inner(custom(not_blank)))]
    pub db: Option<String>,
    #[garde(range(min = 1))]
    pub port: Option<u16>,
}

impl AsahiTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        matches!(value.to_lowercase().as_str(), "todo" | "in progress")
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        value.to_lowercase() == "done"
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(tag = "kind")]
pub enum RunnerConfig {
    #[serde(rename = "codex")]
    Codex(#[garde(dive)] CodexRunner),
    #[serde(rename = "acp")]
    Acp(#[garde(dive)] AcpRunner),
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self::Codex(CodexRunner::default())
    }
}

impl RunnerConfig {
    pub fn command(&self) -> &str {
        match self {
            Self::Codex(c) => &c.command,
            Self::Acp(c) => &c.command,
        }
    }

    pub fn turn_timeout_ms(&self) -> u64 {
        match self {
            Self::Codex(c) => c.turn_timeout_ms,
            Self::Acp(c) => c.turn_timeout_ms,
        }
    }

    pub fn read_timeout_ms(&self) -> u64 {
        match self {
            Self::Codex(c) => c.read_timeout_ms,
            Self::Acp(c) => c.read_timeout_ms,
        }
    }

    pub fn stall_timeout_ms(&self) -> i64 {
        match self {
            Self::Codex(c) => c.stall_timeout_ms,
            Self::Acp(c) => c.stall_timeout_ms,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct CodexRunner {
    #[serde(default = "default_codex_command")]
    #[garde(custom(not_blank))]
    pub command: String,
    #[garde(skip)]
    pub approval_policy: Option<JsonValue>,
    #[garde(inner(custom(not_blank)))]
    pub thread_sandbox: Option<String>,
    #[garde(skip)]
    pub turn_sandbox_policy: Option<JsonValue>,
    #[serde(default = "default_turn_timeout_ms")]
    #[garde(range(min = 1))]
    pub turn_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms")]
    #[garde(range(min = 1))]
    pub read_timeout_ms: u64,
    #[serde(default = "default_stall_timeout_ms")]
    #[garde(range(min = 0))]
    pub stall_timeout_ms: i64,
}

impl Default for CodexRunner {
    fn default() -> Self {
        Self {
            command: DEFAULT_CODEX_COMMAND.to_string(),
            approval_policy: None,
            thread_sandbox: None,
            turn_sandbox_policy: None,
            turn_timeout_ms: DEFAULT_TURN_TIMEOUT_MS,
            read_timeout_ms: DEFAULT_READ_TIMEOUT_MS,
            stall_timeout_ms: DEFAULT_STALL_TIMEOUT_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AcpRunner {
    #[serde(default = "default_acp_command")]
    #[garde(custom(not_blank))]
    pub command: String,
    #[serde(default = "default_turn_timeout_ms")]
    #[garde(range(min = 1))]
    pub turn_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms")]
    #[garde(range(min = 1))]
    pub read_timeout_ms: u64,
    #[serde(default = "default_stall_timeout_ms")]
    #[garde(range(min = 0))]
    pub stall_timeout_ms: i64,
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct SchedulerConfig {
    #[serde(default = "default_max_concurrent")]
    #[garde(range(min = 1))]
    pub max_concurrent: usize,
    #[serde(default = "default_max_turns")]
    #[garde(range(min = 1))]
    pub max_turns: u32,
    #[serde(default = "default_max_retry_backoff_ms")]
    #[garde(range(min = 1))]
    pub retry_backoff_ms: u64,
    #[serde(default)]
    #[garde(custom(valid_state_limits))]
    pub max_concurrent_by_state: HashMap<String, usize>,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: DEFAULT_MAX_CONCURRENT,
            max_turns: DEFAULT_MAX_TURNS,
            retry_backoff_ms: DEFAULT_MAX_RETRY_BACKOFF_MS,
            max_concurrent_by_state: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PollingConfig {
    #[serde(default = "default_poll_interval_ms")]
    #[garde(range(min = 1))]
    pub interval_ms: u64,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(default = "default_workspace_root")]
    #[garde(skip)]
    pub root: PathBuf,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct HooksConfig {
    #[garde(inner(custom(not_blank)))]
    pub after_create: Option<String>,
    #[garde(inner(custom(not_blank)))]
    pub before_run: Option<String>,
    #[garde(inner(custom(not_blank)))]
    pub after_run: Option<String>,
    #[garde(inner(custom(not_blank)))]
    pub before_remove: Option<String>,
    #[serde(default = "default_hook_timeout_ms")]
    #[garde(range(min = 1))]
    pub timeout_ms: u64,
}

impl Default for HooksConfig {
    fn default() -> Self {
        Self {
            after_create: None,
            before_run: None,
            after_run: None,
            before_remove: None,
            timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ServiceConfig {
    #[serde(skip)]
    #[garde(skip)]
    pub workflow_path: PathBuf,
    #[serde(skip)]
    #[garde(skip)]
    pub workflow_dir: PathBuf,
    #[garde(dive)]
    pub tracker: TrackerConfig,
    #[serde(default)]
    #[garde(dive)]
    pub runner: RunnerConfig,
    #[serde(default)]
    #[garde(dive)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    #[garde(dive)]
    pub polling: PollingConfig,
    #[serde(default)]
    #[garde(dive)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    #[garde(dive)]
    pub hooks: HooksConfig,
}

impl ServiceConfig {
    pub fn validate(&self) -> Result<()> {
        Validate::validate(self)
            .map(|_| ())
            .map_err(|err| LunaError::InvalidConfig(err.to_string()))
    }
}

// ─── Resolution ─────────────────────────────────────────────────────────────

pub fn resolve_service_config(
    definition: &WorkflowDefinition,
    workflow_path: &Path,
) -> Result<ServiceConfig> {
    let mut config_map = definition.config.clone();

    if !config_map.contains_key("tracker") {
        let mut tracker = serde_yaml::Mapping::new();
        tracker.insert(
            YamlValue::String("kind".to_string()),
            YamlValue::String("asahi".to_string()),
        );
        tracker.insert(
            YamlValue::String("db".to_string()),
            YamlValue::String("./asahi.db".to_string()),
        );
        config_map.insert(
            YamlValue::String("tracker".to_string()),
            YamlValue::Mapping(tracker),
        );
    }

    let mut config: ServiceConfig = serde_yaml::from_value(YamlValue::Mapping(config_map))
        .map_err(|err| LunaError::InvalidConfig(format!("config parse error: {err}")))?;

    config.workflow_path = absolutize_path(workflow_path)?;
    config.workflow_dir = config
        .workflow_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    config.workspace.root = resolve_workspace_root(
        config.workspace.root.to_str().map(|s| s.to_string()),
        &config.workflow_dir,
    )?;

    // Build lookup tables for tracker states
    match &mut config.tracker {
        TrackerConfig::GitHubProject(t) => {
            t.active_lookup = t
                .active_states
                .iter()
                .map(|s: &String| s.to_lowercase())
                .collect();
            t.terminal_lookup = t
                .terminal_states
                .iter()
                .map(|s: &String| s.to_lowercase())
                .collect();
        }
        TrackerConfig::Linear(t) => {
            t.active_lookup = t
                .active_states
                .iter()
                .map(|s: &String| s.to_lowercase())
                .collect();
            t.terminal_lookup = t
                .terminal_states
                .iter()
                .map(|s: &String| s.to_lowercase())
                .collect();
        }
        TrackerConfig::Asahi(_) => {
            // Asahi tracker states are hard-coded and not configurable
        }
    }

    // Normalize scheduler state keys
    config.scheduler.max_concurrent_by_state = config
        .scheduler
        .max_concurrent_by_state
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();

    config.validate()?;
    Ok(config)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn resolve_workspace_root(value: Option<String>, workflow_dir: &Path) -> Result<PathBuf> {
    let value = value.unwrap_or_else(|| {
        std::env::temp_dir()
            .join("luna_workspaces")
            .to_string_lossy()
            .to_string()
    });

    let expanded = if value == "~" || value.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            LunaError::InvalidConfig("could not resolve home directory".to_string())
        })?;
        if value == "~" {
            home.to_string_lossy().to_string()
        } else {
            home.join(value.trim_start_matches("~/"))
                .to_string_lossy()
                .to_string()
        }
    } else {
        value
    };

    let root = PathBuf::from(expanded);
    let absolute = if root.is_absolute() {
        root
    } else {
        workflow_dir.join(root)
    };
    Ok(normalize_path(&absolute))
}

fn not_blank(value: &str, _: &()) -> garde::Result {
    if value.trim().is_empty() {
        return Err(garde::Error::new("must be non-empty"));
    }
    Ok(())
}

fn asahi_endpoint_or_db<'a>(
    db: &'a Option<String>,
) -> impl FnOnce(&str, &()) -> garde::Result + 'a {
    move |endpoint, _| {
        if db.is_none() && endpoint.trim().is_empty() {
            return Err(garde::Error::new("endpoint is required when db is unset"));
        }
        Ok(())
    }
}

fn valid_state_limits(value: &HashMap<String, usize>, _: &()) -> garde::Result {
    for (state, limit) in value {
        if state.trim().is_empty() {
            return Err(garde::Error::new("state names must be non-empty"));
        }
        if *limit == 0 {
            return Err(garde::Error::new(format!(
                "{state} limit must be greater than 0"
            )));
        }
    }
    Ok(())
}

// ─── Defaults ───────────────────────────────────────────────────────────────

fn default_poll_interval_ms() -> u64 {
    DEFAULT_POLL_INTERVAL_MS
}
fn default_hook_timeout_ms() -> u64 {
    DEFAULT_HOOK_TIMEOUT_MS
}
fn default_workspace_root() -> PathBuf {
    PathBuf::from(".")
}
fn default_max_concurrent() -> usize {
    DEFAULT_MAX_CONCURRENT
}
fn default_max_turns() -> u32 {
    DEFAULT_MAX_TURNS
}
fn default_max_retry_backoff_ms() -> u64 {
    DEFAULT_MAX_RETRY_BACKOFF_MS
}
fn default_turn_timeout_ms() -> u64 {
    DEFAULT_TURN_TIMEOUT_MS
}
fn default_read_timeout_ms() -> u64 {
    DEFAULT_READ_TIMEOUT_MS
}
fn default_stall_timeout_ms() -> i64 {
    DEFAULT_STALL_TIMEOUT_MS
}
fn default_codex_command() -> String {
    DEFAULT_CODEX_COMMAND.to_string()
}
fn default_acp_command() -> String {
    DEFAULT_ACP_COMMAND.to_string()
}
fn default_gh_command() -> String {
    DEFAULT_GH_COMMAND.to_string()
}
fn default_github_status_field() -> String {
    "Status".to_string()
}
fn default_github_priority_field() -> String {
    "Priority".to_string()
}
fn default_github_active_states() -> Vec<String> {
    vec!["Todo".to_string(), "In Progress".to_string()]
}
fn default_github_terminal_states() -> Vec<String> {
    vec!["Done".to_string()]
}
fn default_linear_endpoint() -> String {
    "https://api.linear.app/graphql".to_string()
}
fn default_linear_active_states() -> Vec<String> {
    vec!["Todo".to_string(), "In Progress".to_string()]
}
fn default_linear_terminal_states() -> Vec<String> {
    vec![
        "Closed".to_string(),
        "Cancelled".to_string(),
        "Canceled".to_string(),
        "Duplicate".to_string(),
        "Done".to_string(),
    ]
}


// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn github_project_tracker_parses() {
        let yaml = serde_yaml::from_str(
            r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
runner:
  kind: codex
  command: codex app-server
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap();
        match config.tracker {
            TrackerConfig::GitHubProject(t) => {
                assert_eq!(t.owner, "acme");
                assert_eq!(t.project_number, 12);
            }
            other => panic!("expected github_project, got {:?}", other),
        }
    }

    #[test]
    fn linear_tracker_parses() {
        let yaml = serde_yaml::from_str(
            r#"
tracker:
  kind: linear
  project_slug: my-project
  api_key: lin_api_xxx
runner:
  kind: codex
  command: codex app-server
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap();
        match config.tracker {
            TrackerConfig::Linear(t) => {
                assert_eq!(t.project_slug, Some("my-project".to_string()));
            }
            other => panic!("expected linear, got {:?}", other),
        }
    }

    #[test]
    fn acp_runner_parses() {
        let yaml = serde_yaml::from_str(
            r#"
runner:
  kind: acp
  command: kimi acp
tracker:
  kind: github_project
  owner: acme
  project_number: 12
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap();
        match config.runner {
            RunnerConfig::Acp(r) => {
                assert_eq!(r.command, "kimi acp");
            }
            other => panic!("expected acp, got {:?}", other),
        }
    }

    #[test]
    fn codex_explicit_high_trust_policy() {
        let yaml = serde_yaml::from_str(
            r#"
runner:
  kind: codex
  command: codex app-server
  approval_policy: never
  thread_sandbox: danger-full-access
  turn_sandbox_policy:
    type: dangerFullAccess
tracker:
  kind: github_project
  owner: acme
  project_number: 12
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap();
        match config.runner {
            RunnerConfig::Codex(c) => {
                assert_eq!(
                    c.approval_policy,
                    Some(JsonValue::String("never".to_string()))
                );
                assert_eq!(c.thread_sandbox.as_deref(), Some("danger-full-access"));
                assert_eq!(
                    c.turn_sandbox_policy,
                    Some(json!({ "type": "dangerFullAccess" }))
                );
            }
            other => panic!("expected codex, got {:?}", other),
        }
    }

    #[test]
    fn rejects_legacy_agent_and_codex_sections() {
        let yaml = serde_yaml::from_str(
            r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
agent:
  max_concurrent_agents: 4
codex:
  command: codex app-server
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let err = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_permission_profile_on_runner() {
        let yaml = serde_yaml::from_str(
            r#"
runner:
  kind: codex
  command: codex app-server
  permission_profile: high_trust
tracker:
  kind: github_project
  owner: acme
  project_number: 12
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let err = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap_err();

        assert!(err.to_string().contains("permission_profile"));
    }

    #[test]
    fn relative_workflow_path_resolves_workspace_root() {
        let yaml = serde_yaml::from_str(
            r#"
workspace:
  root: ./.luna/workspaces
tracker:
  kind: github_project
  owner: acme
  project_number: 12
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let cwd = std::env::current_dir().expect("cwd available");
        let config = resolve_service_config(&def, Path::new("fixtures/WORKFLOW.md")).unwrap();

        assert_eq!(
            config.workflow_path,
            normalize_path(&cwd.join("fixtures/WORKFLOW.md"))
        );
        assert_eq!(config.workflow_dir, normalize_path(&cwd.join("fixtures")));
        assert_eq!(
            config.workspace.root,
            normalize_path(&cwd.join("fixtures/.luna/workspaces"))
        );
    }

    #[test]
    fn default_asahi_tracker_does_not_treat_backlog_as_active() {
        let yaml = serde_yaml::from_str(
            r#"
tracker:
  kind: asahi
  db: ./asahi.db
"#,
        )
        .unwrap();
        let def = WorkflowDefinition {
            config: yaml,
            prompt_template: "hello".to_string(),
        };
        let config = resolve_service_config(&def, Path::new("/tmp/WORKFLOW.md")).unwrap();

        assert!(
            !config.tracker.is_active_state("Backlog"),
            "Backlog should not be an active state"
        );
        assert!(
            !config.tracker.is_terminal_state("Backlog"),
            "Backlog should not be a terminal state"
        );
        assert!(config.tracker.is_active_state("Todo"));
        assert!(config.tracker.is_active_state("In Progress"));
        assert!(config.tracker.is_terminal_state("Done"));
    }
}
