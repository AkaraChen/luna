use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
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

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum TrackerConfig {
    #[serde(rename = "github_project")]
    GitHubProject(GitHubProjectTrackerConfig),
    #[serde(rename = "linear")]
    Linear(LinearTrackerConfig),
    #[serde(rename = "asahi")]
    Asahi(AsahiTrackerConfig),
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
            Self::Asahi(c) => &c.terminal_states,
        }
    }

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::GitHubProject(c) => c.validate(),
            Self::Linear(c) => c.validate(),
            Self::Asahi(c) => c.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct GitHubProjectTrackerConfig {
    pub owner: String,
    pub project_number: u32,
    #[serde(default = "default_github_status_field")]
    pub status_field: String,
    #[serde(default = "default_github_priority_field")]
    pub priority_field: String,
    #[serde(default = "default_gh_command")]
    pub gh_command: String,
    #[serde(default = "default_github_active_states")]
    pub active_states: Vec<String>,
    #[serde(default = "default_github_terminal_states")]
    pub terminal_states: Vec<String>,
    #[serde(skip)]
    active_lookup: HashSet<String>,
    #[serde(skip)]
    terminal_lookup: HashSet<String>,
}

impl GitHubProjectTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_lookup.contains(&value.to_lowercase())
    }

    pub fn validate(&self) -> Result<()> {
        if self.owner.trim().is_empty() {
            return Err(LunaError::InvalidConfig(
                "tracker.owner is required for github_project".to_string(),
            ));
        }
        if self.gh_command.trim().is_empty() {
            return Err(LunaError::InvalidConfig(
                "tracker.gh_command must be non-empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct LinearTrackerConfig {
    #[serde(default = "default_linear_endpoint")]
    pub endpoint: String,
    pub api_key: Option<String>,
    pub project_slug: Option<String>,
    pub assignee: Option<String>,
    #[serde(default = "default_linear_active_states")]
    pub active_states: Vec<String>,
    #[serde(default = "default_linear_terminal_states")]
    pub terminal_states: Vec<String>,
    #[serde(skip)]
    active_lookup: HashSet<String>,
    #[serde(skip)]
    terminal_lookup: HashSet<String>,
}

impl LinearTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_lookup.contains(&value.to_lowercase())
    }

    pub fn validate(&self) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct AsahiTrackerConfig {
    #[serde(default)]
    pub endpoint: String,
    pub db: Option<String>,
    pub port: Option<u16>,
    #[serde(default = "default_asahi_active_states")]
    pub active_states: Vec<String>,
    #[serde(default = "default_asahi_terminal_states")]
    pub terminal_states: Vec<String>,
    #[serde(skip)]
    active_lookup: HashSet<String>,
    #[serde(skip)]
    terminal_lookup: HashSet<String>,
}

impl AsahiTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_lookup.contains(&value.to_lowercase())
    }

    pub fn validate(&self) -> Result<()> {
        if self.db.is_none() && self.endpoint.trim().is_empty() {
            return Err(LunaError::InvalidConfig(
                "tracker.endpoint is required for asahi".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind")]
pub enum RunnerConfig {
    #[serde(rename = "codex")]
    Codex(CodexRunner),
    #[serde(rename = "acp")]
    Acp(AcpRunner),
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

    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Codex(c) => {
                if c.command.trim().is_empty() {
                    return Err(LunaError::InvalidConfig(
                        "runner.command must be non-empty".to_string(),
                    ));
                }
                Ok(())
            }
            Self::Acp(c) => {
                if c.command.trim().is_empty() {
                    return Err(LunaError::InvalidConfig(
                        "runner.command must be non-empty".to_string(),
                    ));
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct CodexRunner {
    #[serde(default = "default_codex_command")]
    pub command: String,
    pub approval_policy: Option<JsonValue>,
    pub thread_sandbox: Option<String>,
    pub turn_sandbox_policy: Option<JsonValue>,
    #[serde(default = "default_turn_timeout_ms")]
    pub turn_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_stall_timeout_ms")]
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

#[derive(Clone, Debug, Deserialize)]
pub struct AcpRunner {
    #[serde(default = "default_acp_command")]
    pub command: String,
    #[serde(default = "default_turn_timeout_ms")]
    pub turn_timeout_ms: u64,
    #[serde(default = "default_read_timeout_ms")]
    pub read_timeout_ms: u64,
    #[serde(default = "default_stall_timeout_ms")]
    pub stall_timeout_ms: i64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SchedulerConfig {
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_retry_backoff_ms")]
    pub retry_backoff_ms: u64,
    #[serde(default)]
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

impl SchedulerConfig {
    pub fn validate(&self) -> Result<()> {
        if self.max_turns == 0 {
            return Err(LunaError::InvalidConfig(
                "scheduler.max_turns must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PollingConfig {
    #[serde(default = "default_poll_interval_ms")]
    pub interval_ms: u64,
}

impl Default for PollingConfig {
    fn default() -> Self {
        Self {
            interval_ms: DEFAULT_POLL_INTERVAL_MS,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default = "default_workspace_root")]
    pub root: PathBuf,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct HooksConfig {
    pub after_create: Option<String>,
    pub before_run: Option<String>,
    pub after_run: Option<String>,
    pub before_remove: Option<String>,
    #[serde(default = "default_hook_timeout_ms")]
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

impl HooksConfig {
    pub fn validate(&self) -> Result<()> {
        if self.timeout_ms == 0 {
            return Err(LunaError::InvalidConfig(
                "hooks.timeout_ms must be greater than 0".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ServiceConfig {
    #[serde(skip)]
    pub workflow_path: PathBuf,
    #[serde(skip)]
    pub workflow_dir: PathBuf,
    pub tracker: TrackerConfig,
    #[serde(default)]
    pub runner: RunnerConfig,
    #[serde(default)]
    pub scheduler: SchedulerConfig,
    #[serde(default)]
    pub polling: PollingConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub hooks: HooksConfig,
}

impl ServiceConfig {
    pub fn validate(&self) -> Result<()> {
        self.tracker.validate()?;
        self.runner.validate()?;
        self.scheduler.validate()?;
        self.hooks.validate()?;
        Ok(())
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
        tracker.insert(
            YamlValue::String("active_states".to_string()),
            serde_yaml::to_value(default_asahi_active_states()).unwrap_or(YamlValue::Sequence(Vec::new())),
        );
        tracker.insert(
            YamlValue::String("terminal_states".to_string()),
            serde_yaml::to_value(default_asahi_terminal_states()).unwrap_or(YamlValue::Sequence(Vec::new())),
        );
        config_map.insert(
            YamlValue::String("tracker".to_string()),
            YamlValue::Mapping(tracker),
        );
    }

    let mut config: ServiceConfig =
        serde_yaml::from_value(YamlValue::Mapping(config_map))
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
        TrackerConfig::Asahi(t) => {
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
    }

    // Normalize scheduler state keys
    config.scheduler.max_concurrent_by_state = config
        .scheduler
        .max_concurrent_by_state
        .into_iter()
        .map(|(k, v)| (k.to_lowercase(), v))
        .collect();

    // Resolve codex permission profiles
    if let RunnerConfig::Codex(codex) = &mut config.runner {
        if let Some(profile) = codex.approval_policy.as_ref().and_then(|v| v.as_str()) {
            if let Ok(defaults) = resolve_permission_profile_defaults(profile) {
                codex.approval_policy = codex.approval_policy.clone().or(defaults.approval_policy);
                codex.thread_sandbox = codex.thread_sandbox.clone().or(defaults.thread_sandbox);
                codex.turn_sandbox_policy = codex
                    .turn_sandbox_policy
                    .clone()
                    .or(defaults.turn_sandbox_policy);
            }
        }
    }

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

#[derive(Default)]
struct PermissionProfileDefaults {
    approval_policy: Option<JsonValue>,
    thread_sandbox: Option<String>,
    turn_sandbox_policy: Option<JsonValue>,
}

fn resolve_permission_profile_defaults(profile: &str) -> Result<PermissionProfileDefaults> {
    match profile.trim().to_lowercase().replace('-', "_").as_str() {
        "high_trust" => Ok(PermissionProfileDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("danger-full-access".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "dangerFullAccess" })),
        }),
        "workspace_write" => Ok(PermissionProfileDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("workspace-write".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "workspaceWrite" })),
        }),
        "read_only" => Ok(PermissionProfileDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("read-only".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "readOnly" })),
        }),
        _ => Err(LunaError::InvalidConfig(format!(
            "unsupported permission_profile: {profile}"
        ))),
    }
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
fn default_asahi_active_states() -> Vec<String> {
    vec!["Todo".to_string(), "In Progress".to_string()]
}
fn default_asahi_terminal_states() -> Vec<String> {
    vec!["Done".to_string()]
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
    fn codex_permission_profile_high_trust() {
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
}
