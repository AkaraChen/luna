use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Value as JsonValue, json};
use serde_yaml::{Mapping, Value as YamlValue};

use crate::{
    error::{LunaError, Result},
    model::WorkflowDefinition,
    paths::{absolutize_path, normalize_path},
};

const DEFAULT_POLL_INTERVAL_MS: u64 = 30_000;
const DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_MAX_CONCURRENT_AGENTS: usize = 10;
const DEFAULT_MAX_TURNS: u32 = 20;
const DEFAULT_MAX_RETRY_BACKOFF_MS: u64 = 300_000;
const DEFAULT_CODEX_COMMAND: &str = "codex app-server";
const DEFAULT_TURN_TIMEOUT_MS: u64 = 3_600_000;
const DEFAULT_READ_TIMEOUT_MS: u64 = 5_000;
const DEFAULT_STALL_TIMEOUT_MS: i64 = 300_000;
const DEFAULT_GH_COMMAND: &str = "gh";
const DEFAULT_GITHUB_STATUS_FIELD: &str = "Status";
const DEFAULT_GITHUB_PRIORITY_FIELD: &str = "Priority";

#[derive(Clone, Debug)]
pub enum TrackerConfig {
    GitHubProject(GitHubProjectTrackerConfig),
    Linear(LinearTrackerConfig),
}

impl TrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        match self {
            Self::GitHubProject(config) => config.is_active_state(value),
            Self::Linear(config) => config.is_active_state(value),
        }
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        match self {
            Self::GitHubProject(config) => config.is_terminal_state(value),
            Self::Linear(config) => config.is_terminal_state(value),
        }
    }

    pub fn terminal_states(&self) -> &[String] {
        match self {
            Self::GitHubProject(config) => &config.terminal_states,
            Self::Linear(config) => &config.terminal_states,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GitHubProjectTrackerConfig {
    pub owner: String,
    pub project_number: u32,
    pub status_field: String,
    pub priority_field: String,
    pub gh_command: String,
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
    active_state_lookup: HashSet<String>,
    terminal_state_lookup: HashSet<String>,
}

impl GitHubProjectTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_state_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_state_lookup.contains(&value.to_lowercase())
    }
}

#[derive(Clone, Debug)]
pub struct LinearTrackerConfig {
    pub endpoint: String,
    pub api_key: Option<String>,
    pub project_slug: Option<String>,
    pub assignee: Option<String>,
    pub active_states: Vec<String>,
    pub terminal_states: Vec<String>,
    active_state_lookup: HashSet<String>,
    terminal_state_lookup: HashSet<String>,
}

impl LinearTrackerConfig {
    pub fn is_active_state(&self, value: &str) -> bool {
        self.active_state_lookup.contains(&value.to_lowercase())
    }

    pub fn is_terminal_state(&self, value: &str) -> bool {
        self.terminal_state_lookup.contains(&value.to_lowercase())
    }
}

#[derive(Clone, Debug)]
pub struct PollingConfig {
    pub interval_ms: u64,
}

#[derive(Clone, Debug)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct HooksConfig {
    pub after_create: Option<String>,
    pub before_run: Option<String>,
    pub after_run: Option<String>,
    pub before_remove: Option<String>,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug)]
pub struct AgentConfig {
    pub max_concurrent_agents: usize,
    pub max_turns: u32,
    pub max_retry_backoff_ms: u64,
    pub max_concurrent_agents_by_state: HashMap<String, usize>,
}

#[derive(Clone, Debug)]
pub struct CodexConfig {
    pub command: String,
    pub approval_policy: Option<JsonValue>,
    pub thread_sandbox: Option<String>,
    pub turn_sandbox_policy: Option<JsonValue>,
    pub turn_timeout_ms: u64,
    pub read_timeout_ms: u64,
    pub stall_timeout_ms: i64,
}

#[derive(Clone, Debug)]
pub struct ServiceConfig {
    pub workflow_path: PathBuf,
    pub workflow_dir: PathBuf,
    pub tracker: TrackerConfig,
    pub polling: PollingConfig,
    pub workspace: WorkspaceConfig,
    pub hooks: HooksConfig,
    pub agent: AgentConfig,
    pub codex: CodexConfig,
}

#[derive(Debug, Default, Deserialize)]
struct RawWorkflowConfig {
    tracker: Option<RawTrackerConfig>,
    polling: Option<RawPollingConfig>,
    workspace: Option<RawWorkspaceConfig>,
    hooks: Option<RawHooksConfig>,
    agent: Option<RawAgentConfig>,
    codex: Option<RawCodexConfig>,
    #[serde(flatten)]
    _unknown: BTreeMap<String, YamlValue>,
}

#[derive(Debug, Default, Deserialize)]
struct RawTrackerConfig {
    kind: Option<String>,
    owner: Option<String>,
    project_number: Option<u32>,
    status_field: Option<String>,
    priority_field: Option<String>,
    gh_command: Option<String>,
    endpoint: Option<String>,
    api_key: Option<String>,
    project_slug: Option<String>,
    assignee: Option<String>,
    active_states: Option<Vec<String>>,
    terminal_states: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawPollingConfig {
    interval_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RawWorkspaceConfig {
    root: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawHooksConfig {
    after_create: Option<String>,
    before_run: Option<String>,
    after_run: Option<String>,
    before_remove: Option<String>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAgentConfig {
    max_concurrent_agents: Option<usize>,
    max_turns: Option<u32>,
    max_retry_backoff_ms: Option<u64>,
    max_concurrent_agents_by_state: Option<HashMap<String, i64>>,
}

#[derive(Debug, Default, Deserialize)]
struct RawCodexConfig {
    command: Option<String>,
    permission_profile: Option<String>,
    approval_policy: Option<YamlValue>,
    thread_sandbox: Option<String>,
    turn_sandbox_policy: Option<YamlValue>,
    turn_timeout_ms: Option<u64>,
    read_timeout_ms: Option<u64>,
    stall_timeout_ms: Option<i64>,
}

pub fn resolve_service_config(
    definition: &WorkflowDefinition,
    workflow_path: &Path,
) -> Result<ServiceConfig> {
    let raw = parse_raw_config(&definition.config)?;
    let workflow_path = absolutize_path(workflow_path)?;
    let workflow_dir = workflow_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    let tracker = resolve_tracker_config(raw.tracker.unwrap_or_default())?;

    let polling = PollingConfig {
        interval_ms: raw
            .polling
            .and_then(|cfg| cfg.interval_ms)
            .unwrap_or(DEFAULT_POLL_INTERVAL_MS),
    };

    let workspace = WorkspaceConfig {
        root: resolve_workspace_root(raw.workspace.and_then(|cfg| cfg.root), &workflow_dir)?,
    };

    let hooks = {
        let raw_hooks = raw.hooks.unwrap_or_default();
        let timeout_ms = raw_hooks.timeout_ms.unwrap_or(DEFAULT_HOOK_TIMEOUT_MS);
        if timeout_ms == 0 {
            return Err(LunaError::InvalidConfig(
                "hooks.timeout_ms must be greater than 0".to_string(),
            ));
        }
        HooksConfig {
            after_create: raw_hooks.after_create,
            before_run: raw_hooks.before_run,
            after_run: raw_hooks.after_run,
            before_remove: raw_hooks.before_remove,
            timeout_ms,
        }
    };

    let agent = {
        let raw_agent = raw.agent.unwrap_or_default();
        let max_turns = raw_agent.max_turns.unwrap_or(DEFAULT_MAX_TURNS);
        if max_turns == 0 {
            return Err(LunaError::InvalidConfig(
                "agent.max_turns must be greater than 0".to_string(),
            ));
        }
        AgentConfig {
            max_concurrent_agents: raw_agent
                .max_concurrent_agents
                .unwrap_or(DEFAULT_MAX_CONCURRENT_AGENTS),
            max_turns,
            max_retry_backoff_ms: raw_agent
                .max_retry_backoff_ms
                .unwrap_or(DEFAULT_MAX_RETRY_BACKOFF_MS),
            max_concurrent_agents_by_state: raw_agent
                .max_concurrent_agents_by_state
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(state, value)| {
                    if value > 0 {
                        Some((state.to_lowercase(), value as usize))
                    } else {
                        None
                    }
                })
                .collect(),
        }
    };

    let codex = {
        let raw_codex = raw.codex.unwrap_or_default();
        let command = raw_codex
            .command
            .unwrap_or_else(|| DEFAULT_CODEX_COMMAND.to_string());
        if command.trim().is_empty() {
            return Err(LunaError::InvalidConfig(
                "codex.command must be non-empty".to_string(),
            ));
        }
        let permission_defaults =
            resolve_codex_permission_profile_defaults(raw_codex.permission_profile.as_deref())?;
        CodexConfig {
            command,
            approval_policy: raw_codex
                .approval_policy
                .map(yaml_to_json)
                .transpose()?
                .or(permission_defaults.approval_policy),
            thread_sandbox: raw_codex
                .thread_sandbox
                .or(permission_defaults.thread_sandbox),
            turn_sandbox_policy: raw_codex
                .turn_sandbox_policy
                .map(yaml_to_json)
                .transpose()?
                .or(permission_defaults.turn_sandbox_policy),
            turn_timeout_ms: raw_codex.turn_timeout_ms.unwrap_or(DEFAULT_TURN_TIMEOUT_MS),
            read_timeout_ms: raw_codex.read_timeout_ms.unwrap_or(DEFAULT_READ_TIMEOUT_MS),
            stall_timeout_ms: raw_codex
                .stall_timeout_ms
                .unwrap_or(DEFAULT_STALL_TIMEOUT_MS),
        }
    };

    Ok(ServiceConfig {
        workflow_path,
        workflow_dir,
        tracker,
        polling,
        workspace,
        hooks,
        agent,
        codex,
    })
}

fn resolve_tracker_config(raw: RawTrackerConfig) -> Result<TrackerConfig> {
    let kind = raw.kind.unwrap_or_default();
    match normalize_tracker_kind(&kind).as_deref() {
        Some("github_project") => {
            let owner = raw
                .owner
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    LunaError::InvalidConfig(
                        "tracker.owner is required for github_project".to_string(),
                    )
                })?;
            let project_number = raw.project_number.ok_or_else(|| {
                LunaError::InvalidConfig(
                    "tracker.project_number is required for github_project".to_string(),
                )
            })?;
            let status_field = raw
                .status_field
                .unwrap_or_else(|| DEFAULT_GITHUB_STATUS_FIELD.to_string());
            let priority_field = raw
                .priority_field
                .unwrap_or_else(|| DEFAULT_GITHUB_PRIORITY_FIELD.to_string());
            let gh_command = raw
                .gh_command
                .unwrap_or_else(|| DEFAULT_GH_COMMAND.to_string());
            if gh_command.trim().is_empty() {
                return Err(LunaError::InvalidConfig(
                    "tracker.gh_command must be non-empty".to_string(),
                ));
            }

            let active_states = raw
                .active_states
                .unwrap_or_else(|| vec!["Todo".to_string(), "In Progress".to_string()]);
            let terminal_states = raw
                .terminal_states
                .unwrap_or_else(|| vec!["Done".to_string()]);

            Ok(TrackerConfig::GitHubProject(GitHubProjectTrackerConfig {
                owner,
                project_number,
                status_field,
                priority_field,
                gh_command,
                active_state_lookup: active_states
                    .iter()
                    .map(|value| value.to_lowercase())
                    .collect(),
                terminal_state_lookup: terminal_states
                    .iter()
                    .map(|value| value.to_lowercase())
                    .collect(),
                active_states,
                terminal_states,
            }))
        }
        Some("linear") => {
            let api_key = raw.api_key.filter(|value| !value.trim().is_empty());
            let project_slug = raw.project_slug.filter(|value| !value.trim().is_empty());
            let endpoint = raw
                .endpoint
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "https://api.linear.app/graphql".to_string());
            let assignee = raw.assignee.filter(|value| !value.trim().is_empty());

            let active_states = raw
                .active_states
                .unwrap_or_else(|| vec!["Todo".to_string(), "In Progress".to_string()]);
            let terminal_states = raw
                .terminal_states
                .unwrap_or_else(|| vec!["Closed".to_string(), "Cancelled".to_string(), "Canceled".to_string(), "Duplicate".to_string(), "Done".to_string()]);

            Ok(TrackerConfig::Linear(LinearTrackerConfig {
                endpoint,
                api_key,
                project_slug,
                assignee,
                active_state_lookup: active_states
                    .iter()
                    .map(|value| value.to_lowercase())
                    .collect(),
                terminal_state_lookup: terminal_states
                    .iter()
                    .map(|value| value.to_lowercase())
                    .collect(),
                active_states,
                terminal_states,
            }))
        }
        _ => Err(LunaError::UnsupportedTrackerKind(kind)),
    }
}

fn normalize_tracker_kind(kind: &str) -> Option<String> {
    let normalized = kind.trim().replace('-', "_");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

#[derive(Default)]
struct CodexPermissionDefaults {
    approval_policy: Option<JsonValue>,
    thread_sandbox: Option<String>,
    turn_sandbox_policy: Option<JsonValue>,
}

fn resolve_codex_permission_profile_defaults(
    profile: Option<&str>,
) -> Result<CodexPermissionDefaults> {
    let Some(profile) = profile else {
        return Ok(CodexPermissionDefaults::default());
    };

    match normalize_permission_profile(profile).as_deref() {
        Some("high_trust") => Ok(CodexPermissionDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("danger-full-access".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "dangerFullAccess" })),
        }),
        Some("workspace_write") => Ok(CodexPermissionDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("workspace-write".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "workspaceWrite" })),
        }),
        Some("read_only") => Ok(CodexPermissionDefaults {
            approval_policy: Some(JsonValue::String("never".to_string())),
            thread_sandbox: Some("read-only".to_string()),
            turn_sandbox_policy: Some(json!({ "type": "readOnly" })),
        }),
        _ => Err(LunaError::InvalidConfig(format!(
            "unsupported codex.permission_profile: {profile}"
        ))),
    }
}

fn normalize_permission_profile(profile: &str) -> Option<String> {
    let normalized = profile.trim().replace('-', "_");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn parse_raw_config(config: &Mapping) -> Result<RawWorkflowConfig> {
    let yaml = YamlValue::Mapping(config.clone());
    serde_yaml::from_value(yaml).map_err(Into::into)
}

fn resolve_workspace_root(value: Option<String>, workflow_dir: &Path) -> Result<PathBuf> {
    let value = value.unwrap_or_else(|| {
        std::env::temp_dir()
            .join("symphony_workspaces")
            .to_string_lossy()
            .to_string()
    });

    let expanded = if value == "~" || value.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            LunaError::InvalidConfig(
                "could not resolve home directory for workspace.root".to_string(),
            )
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

fn yaml_to_json(value: YamlValue) -> Result<JsonValue> {
    serde_json::to_value(value).map_err(|err| {
        LunaError::InvalidConfig(format!("failed to convert yaml value to json: {err}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::WorkflowDefinition;
    use serde_yaml::Mapping;

    #[test]
    fn missing_tracker_fails() {
        let definition = WorkflowDefinition {
            config: Mapping::new(),
            prompt_template: "hello".to_string(),
        };
        let error = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md"))
            .expect_err("missing tracker config should fail");
        assert!(matches!(error, LunaError::UnsupportedTrackerKind(_)));
    }

    #[test]
    fn github_project_kind_alias_works() {
        let definition = WorkflowDefinition {
            config: serde_yaml::from_str(
                r#"
tracker:
  kind: github-project
  owner: acme
  project_number: 12
"#,
            )
            .expect("valid yaml"),
            prompt_template: "hello".to_string(),
        };

        let config = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md"))
            .expect("config should parse");
        match config.tracker {
            TrackerConfig::GitHubProject(project) => {
                assert_eq!(project.owner, "acme");
                assert_eq!(project.project_number, 12);
            }
            TrackerConfig::Linear(_) => unreachable!(),
        }
    }

    #[test]
    fn codex_permission_profile_high_trust_maps_to_permissions() {
        let definition = WorkflowDefinition {
            config: serde_yaml::from_str(
                r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
codex:
  permission_profile: high-trust
"#,
            )
            .expect("valid yaml"),
            prompt_template: "hello".to_string(),
        };

        let config = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md"))
            .expect("config should parse");
        assert_eq!(
            config.codex.approval_policy,
            Some(JsonValue::String("never".to_string()))
        );
        assert_eq!(
            config.codex.thread_sandbox.as_deref(),
            Some("danger-full-access")
        );
        assert_eq!(
            config.codex.turn_sandbox_policy,
            Some(json!({ "type": "dangerFullAccess" }))
        );
    }

    #[test]
    fn explicit_codex_permissions_override_profile_defaults() {
        let definition = WorkflowDefinition {
            config: serde_yaml::from_str(
                r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
codex:
  permission_profile: high_trust
  approval_policy: on-request
  thread_sandbox: workspace-write
  turn_sandbox_policy:
    type: workspaceWrite
"#,
            )
            .expect("valid yaml"),
            prompt_template: "hello".to_string(),
        };

        let config = resolve_service_config(&definition, Path::new("/tmp/WORKFLOW.md"))
            .expect("config should parse");
        assert_eq!(
            config.codex.approval_policy,
            Some(JsonValue::String("on-request".to_string()))
        );
        assert_eq!(
            config.codex.thread_sandbox.as_deref(),
            Some("workspace-write")
        );
        assert_eq!(
            config.codex.turn_sandbox_policy,
            Some(json!({ "type": "workspaceWrite" }))
        );
    }

    #[test]
    fn relative_workflow_path_resolves_workspace_root_to_absolute() {
        let definition = WorkflowDefinition {
            config: serde_yaml::from_str(
                r#"
tracker:
  kind: github_project
  owner: acme
  project_number: 12
workspace:
  root: ./.luna/workspaces
"#,
            )
            .expect("valid yaml"),
            prompt_template: "hello".to_string(),
        };

        let cwd = std::env::current_dir().expect("cwd available");
        let config = resolve_service_config(&definition, Path::new("fixtures/WORKFLOW.md"))
            .expect("config should parse");

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
