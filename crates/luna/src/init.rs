use std::{
    fs,
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
};

use tokio::process::Command;

use crate::error::{LunaError, Result};

#[derive(Clone, Debug, Default)]
pub struct InitOptions {
    pub target_dir: PathBuf,
    pub force: bool,
    pub owner: Option<String>,
    pub project_number: Option<u32>,
    pub create_project: bool,
    pub project_title: Option<String>,
    pub non_interactive: bool,
    pub tracker_kind: Option<String>,
}

#[derive(Clone, Debug)]
struct InitContext {
    tracker_kind: String,
    owner: String,
    project_number: u32,
    project_title: String,
    repo_name_with_owner: Option<String>,
    created_project: bool,
    asahi_port: Option<u16>,
    asahi_db: Option<String>,
}

const DEFAULT_OWNER: &str = "your-github-owner";
const DEFAULT_PROJECT_NUMBER: u32 = 1;
const DEFAULT_PROJECT_TITLE: &str = "Luna Project";

pub async fn run_init(options: InitOptions) -> Result<Vec<PathBuf>> {
    fs::create_dir_all(&options.target_dir)?;

    let context = if options.non_interactive || !io::stdin().is_terminal() {
        build_non_interactive_context(&options).await?
    } else {
        build_interactive_context(&options).await?
    };

    let workflow_path = options.target_dir.join("WORKFLOW.md");
    let env_path = options.target_dir.join(".env.luna");
    let gitignore_path = options.target_dir.join(".gitignore");

    write_file(
        &workflow_path,
        &render_workflow_template(&context),
        options.force,
    )?;
    write_file(&env_path, ENV_TEMPLATE, options.force)?;
    ensure_gitignore_entry(&gitignore_path, ".env.luna")?;

    print_init_summary(&context);

    Ok(vec![workflow_path, env_path, gitignore_path])
}

async fn build_non_interactive_context(options: &InitOptions) -> Result<InitContext> {
    let tracker_kind = options
        .tracker_kind
        .clone()
        .unwrap_or_else(|| "github_project".to_string());

    if tracker_kind == "asahi" {
        let project_title = options
            .project_title
            .clone()
            .unwrap_or_else(|| default_project_title(&options.target_dir));
        let port = find_available_port().await?;
        return Ok(InitContext {
            tracker_kind,
            owner: DEFAULT_OWNER.to_string(),
            project_number: DEFAULT_PROJECT_NUMBER,
            project_title,
            repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
            created_project: false,
            asahi_port: Some(port),
            asahi_db: Some("./asahi.db".to_string()),
        });
    }

    let mut owner = options
        .owner
        .clone()
        .unwrap_or_else(|| DEFAULT_OWNER.to_string());
    if owner == DEFAULT_OWNER {
        if let Some(detected) = detect_current_github_login().await {
            owner = detected;
        }
    }

    let project_title = options
        .project_title
        .clone()
        .unwrap_or_else(|| default_project_title(&options.target_dir));

    let (project_number, created_project) = if options.create_project {
        match create_github_project(&owner, &project_title).await {
            Ok(number) => (number, true),
            Err(_) => (
                options.project_number.unwrap_or(DEFAULT_PROJECT_NUMBER),
                false,
            ),
        }
    } else {
        (
            options.project_number.unwrap_or(DEFAULT_PROJECT_NUMBER),
            false,
        )
    };

    Ok(InitContext {
        tracker_kind,
        owner,
        project_number,
        project_title,
        repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
        created_project,
        asahi_port: None,
        asahi_db: None,
    })
}

async fn build_interactive_context(options: &InitOptions) -> Result<InitContext> {
    let tracker_default = options
        .tracker_kind
        .clone()
        .unwrap_or_else(|| "github_project".to_string());
    let tracker_kind = if options.tracker_kind.is_some() {
        tracker_default
    } else {
        let choices = vec!["github_project", "asahi"];
        prompt_choice("Tracker kind", &choices, 0)?
    };

    if tracker_kind == "asahi" {
        let project_title_default = options
            .project_title
            .clone()
            .unwrap_or_else(|| default_project_title(&options.target_dir));
        let project_title = prompt_string("Project title", &project_title_default)?;
        let port = find_available_port().await?;
        return Ok(InitContext {
            tracker_kind,
            owner: DEFAULT_OWNER.to_string(),
            project_number: DEFAULT_PROJECT_NUMBER,
            project_title,
            repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
            created_project: false,
            asahi_port: Some(port),
            asahi_db: Some("./asahi.db".to_string()),
        });
    }

    println!("Luna will generate a GitHub Project based workflow.");
    println!("If you are not logged in, run `gh auth login` first.");

    let owner_default = options
        .owner
        .clone()
        .unwrap_or_else(|| DEFAULT_OWNER.to_string());
    let owner_default = if owner_default == DEFAULT_OWNER {
        detect_current_github_login()
            .await
            .unwrap_or_else(|| DEFAULT_OWNER.to_string())
    } else {
        owner_default
    };
    let owner = prompt_string("GitHub project owner", &owner_default)?;

    let should_create_project = if options.create_project {
        true
    } else if options.project_number.is_some() {
        prompt_confirm("Create a new GitHub Project now?", false)?
    } else {
        prompt_confirm("Create a new GitHub Project now?", true)?
    };

    let project_title_default = options
        .project_title
        .clone()
        .unwrap_or_else(|| default_project_title(&options.target_dir));

    let (project_number, project_title, created_project) = if should_create_project {
        let title = prompt_string("GitHub project title", &project_title_default)?;
        let number = create_github_project(&owner, &title).await?;
        println!(
            "Created GitHub Project `{}` with number {} for owner `{}`.",
            title, number, owner
        );
        if prompt_confirm(
            "Link the current repository to that project with `gh project link`?",
            true,
        )? {
            link_current_repo_to_project(&options.target_dir, &owner, number).await?;
        }
        (number, title, true)
    } else {
        let number = prompt_u32(
            "Existing GitHub project number",
            options.project_number.unwrap_or(DEFAULT_PROJECT_NUMBER),
        )?;
        (number, project_title_default, false)
    };

    Ok(InitContext {
        tracker_kind,
        owner,
        project_number,
        project_title,
        repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
        created_project,
        asahi_port: None,
        asahi_db: None,
    })
}

async fn detect_current_github_login() -> Option<String> {
    run_gh_capture(&["api", "user", "--jq", ".login"], Path::new("."))
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn detect_repo_name_with_owner(target_dir: &Path) -> Option<String> {
    run_gh_capture(
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "--jq",
            ".nameWithOwner",
        ],
        target_dir,
    )
    .await
    .ok()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

async fn create_github_project(owner: &str, title: &str) -> Result<u32> {
    let output = run_gh_capture(
        &[
            "project", "create", "--owner", owner, "--title", title, "--format", "json", "--jq",
            ".number",
        ],
        Path::new("."),
    )
    .await?;

    output.trim().parse::<u32>().map_err(|err| {
        LunaError::InvalidConfig(format!(
            "failed to parse project number from `gh project create`: {err}"
        ))
    })
}

async fn link_current_repo_to_project(
    target_dir: &Path,
    owner: &str,
    project_number: u32,
) -> Result<()> {
    let project_number = project_number.to_string();
    run_gh_capture(
        &["project", "link", project_number.as_str(), "--owner", owner],
        target_dir,
    )
    .await?;
    Ok(())
}

async fn run_gh_capture(args: &[&str], cwd: &Path) -> Result<String> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;

    if !output.status.success() {
        return Err(LunaError::InvalidConfig(format!(
            "`gh {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn render_workflow_template(context: &InitContext) -> String {
    let repo_hint = context
        .repo_name_with_owner
        .as_deref()
        .unwrap_or("owner/repo");

    let tracker_front_matter = if context.tracker_kind == "asahi" {
        let port = context.asahi_port.unwrap_or(8080);
        let db = context.asahi_db.as_deref().unwrap_or("./asahi.db");
        format!(
            r#"tracker:
  kind: asahi
  db: {db}
  port: {port}"#
        )
    } else {
        format!(
            r#"tracker:
  kind: github_project
  owner: {owner}
  project_number: {project_number}
  status_field: Status
  priority_field: Priority
  gh_command: gh
  active_states:
    - Todo
    - In Progress
  terminal_states:
    - Done"#,
            owner = context.owner,
            project_number = context.project_number,
        )
    };

    format!(
        r#"---
{tracker_front_matter}

polling:
  interval_ms: 30000

workspace:
  root: ./.luna/workspaces

hooks:
  timeout_ms: 60000

scheduler:
  max_concurrent: 4
  max_turns: 20
  retry_backoff_ms: 300000

runner:
  kind: codex
  command: codex app-server
  approval_policy: never
  thread_sandbox: danger-full-access
  turn_sandbox_policy:
    type: dangerFullAccess
---
# Luna Workflow

You are Luna, an autonomous coding agent working on a tracker item.

Project context:
{project_context}

Issue: {{{{ issue.identifier }}}} - {{{{ issue.title }}}}
URL: {{{{ issue.url or "" }}}}
State: {{{{ issue.state }}}}
Priority: {{{{ issue.priority if issue.priority is not none else "unprioritized" }}}}

Description:
{{{{ issue.description or "(no description provided)" }}}}

Blocked by:
{{% if issue.blocked_by %}}
{{% for blocker in issue.blocked_by %}}
- {{{{ blocker.identifier or blocker.id or "unknown" }}}} (state: {{{{ blocker.state or "unknown" }}}})
{{% endfor %}}
{{% else %}}
- none
{{% endif %}}

Attempt:
{{{{ attempt if attempt is not none else "first run" }}}}

Execution rules:
- Work only inside the current workspace.
- The repository checkout already lives in the current workspace; run commands from the current working directory and do not construct nested `.luna/workspaces/...` paths yourself.
- Do not guess Luna CLI usage. Check the real interface with `luna --help`, and inspect subcommand details with commands like `luna comment --help` whenever you need exact flags or behavior.
- At the start of every run, sync the workspace with the latest upstream code before making changes. Prefer `git pull --ff-only`; if the workspace is detached or has no upstream tracking branch, fetch the latest remote state and update from the correct base branch before continuing.
- Inspect the current tracker item with `luna show` before editing code.
- Use `luna comment` to post meaningful progress updates, blockers, and the final handoff summary so the workflow stays tracker-agnostic.
- Use `luna move "<state>"` when you need to advance the tracker state through the workflow.
- When the implementation is ready, open or update a PR with `gh pr create`, `gh pr view`, `gh pr edit`, and `gh pr comment`.
- After a PR exists, check review status and CI with `gh pr view`, `gh pr checks`, or `gh run watch`.
- Once the required review is satisfied and CI is green, merge the PR with `gh pr merge` instead of stopping at a local code change.
- Use `luna`, `gh pr`, and git commands whenever you need to inspect or update project state.
- Validate changes before stopping.
- Move the project item or backing issue to the next workflow-defined handoff state when appropriate.
"#,
        tracker_front_matter = tracker_front_matter,
        project_context = if context.tracker_kind == "asahi" {
            format!(
                "- Tracker: Asahi (local)\n- Project title: `{project_title}`\n- Database: `{db}`\n- Port: `{port}`\n- Start asahi manually with: `ROCKET_PORT={port} asahi` (or let luna embed it automatically)\n- Browse the project wiki with `luna wiki <command>` — it runs inside a virtual bash sandbox with the full wiki mounted as a filesystem, so most standard Unix commands work (ls, tree, cat, grep, find, wc, head, tail, sort, uniq, sed, awk, jq, etc.), including pipes and redirections. Examples:\n  - `luna wiki ls` or `luna wiki ls -la`\n  - `luna wiki tree`\n  - `luna wiki cat <page>.md`\n  - `luna wiki grep -r \"TODO\" .`\n  - `luna wiki cat design.md | grep \"API\"`\n  - `luna wiki find . -name \"*.md\" | wc -l`",
                project_title = context.project_title,
                db = context.asahi_db.as_deref().unwrap_or("./asahi.db"),
                port = context.asahi_port.unwrap_or(8080),
            )
        } else {
            format!(
                "- GitHub Project owner: `{owner}`\n- GitHub Project number: `{project_number}`\n- GitHub Project title: `{project_title}`\n- Open the project in the browser with: `gh project view {project_number} --owner {owner} --web`\n- Inspect project items with: `gh project item-list {project_number} --owner {owner} --format json`\n- If this item corresponds to a repository issue, inspect it with commands like:\n  `luna show`\n  `luna comment \"...\"`\n  `luna move \"In Progress\"`\n- Open, inspect, and update pull requests with commands like:\n  `gh pr create -R {repo_hint} --fill`\n  `gh pr view -R {repo_hint} --json number,url,reviewDecision,statusCheckRollup`\n  `gh pr comment <number> -R {repo_hint} --body \"...\"`\n  `gh pr checks <number> -R {repo_hint} --watch`\n  `gh pr merge <number> -R {repo_hint} --squash --delete-branch`",
                owner = context.owner,
                project_number = context.project_number,
                project_title = context.project_title,
                repo_hint = repo_hint,
            )
        },
    )
}

const ENV_TEMPLATE: &str = r#"# Luna runtime secrets
# `gh` can use these if you don't want to rely on `gh auth login`.
GH_TOKEN=
GITHUB_TOKEN=
"#;

fn print_init_summary(context: &InitContext) {
    if context.tracker_kind == "asahi" {
        println!(
            "Configured Luna for Asahi tracker `{}` (db: {}, port: {}).",
            context.project_title,
            context.asahi_db.as_deref().unwrap_or("./asahi.db"),
            context.asahi_port.unwrap_or(0),
        );
        println!("Luna will embed asahi automatically when running.");
    } else {
        println!(
            "Configured Luna for GitHub Project `{}` (owner `{}`, number {}).",
            context.project_title, context.owner, context.project_number
        );
        if context.created_project {
            println!(
                "Project created. Open it with: gh project view {} --owner {} --web",
                context.project_number, context.owner
            );
        } else {
            println!(
                "Project not created by Luna. If needed, create one with: gh project create --owner {} --title \"{}\"",
                context.owner, context.project_title
            );
        }
    }
}

fn prompt_string(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush()?;
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    let value = buffer.trim();
    if value.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(value.to_string())
    }
}

fn prompt_confirm(label: &str, default: bool) -> Result<bool> {
    let suffix = if default { "Y/n" } else { "y/N" };
    print!("{label} [{suffix}]: ");
    io::stdout().flush()?;
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    let value = buffer.trim().to_lowercase();
    if value.is_empty() {
        Ok(default)
    } else {
        Ok(matches!(value.as_str(), "y" | "yes"))
    }
}

fn prompt_u32(label: &str, default: u32) -> Result<u32> {
    let value = prompt_string(label, &default.to_string())?;
    value
        .parse::<u32>()
        .map_err(|err| LunaError::InvalidConfig(format!("invalid integer for `{label}`: {err}")))
}

fn prompt_choice(label: &str, choices: &[&str], default_index: usize) -> Result<String> {
    println!("{label}:");
    for (i, choice) in choices.iter().enumerate() {
        let marker = if i == default_index { " *" } else { "" };
        println!("  {}) {}{}", i + 1, choice, marker);
    }
    print!("[default: {}]: ", default_index + 1);
    io::stdout().flush()?;
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer)?;
    let value = buffer.trim();
    if value.is_empty() {
        return Ok(choices[default_index].to_string());
    }
    if let Ok(index) = value.parse::<usize>() {
        if index > 0 && index <= choices.len() {
            return Ok(choices[index - 1].to_string());
        }
    }
    Err(LunaError::InvalidConfig(format!(
        "invalid choice for `{label}`: {value}"
    )))
}

async fn find_available_port() -> Result<u16> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    drop(listener);
    Ok(addr.port())
}

fn default_project_title(target_dir: &Path) -> String {
    target_dir
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty() && *name != ".")
        .map(|name| format!("{name} backlog"))
        .unwrap_or_else(|| DEFAULT_PROJECT_TITLE.to_string())
}

fn write_file(path: &Path, contents: &str, force: bool) -> Result<()> {
    if path.exists() && !force {
        return Err(LunaError::InvalidConfig(format!(
            "refusing to overwrite existing file without --force: {}",
            path.display()
        )));
    }
    fs::write(path, contents)?;
    Ok(())
}

fn ensure_gitignore_entry(path: &Path, entry: &str) -> Result<()> {
    if !path.exists() {
        fs::write(path, format!("/target\n{entry}\n"))?;
        return Ok(());
    }

    let current = fs::read_to_string(path)?;
    if current.lines().any(|line| line.trim() == entry) {
        return Ok(());
    }

    let mut next = current;
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(entry);
    next.push('\n');
    fs::write(path, next)?;
    Ok(())
}
