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
}

#[derive(Clone, Debug)]
struct InitContext {
    owner: String,
    project_number: u32,
    project_title: String,
    repo_name_with_owner: Option<String>,
    created_project: bool,
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
    let mut owner = options.owner.clone().unwrap_or_else(|| DEFAULT_OWNER.to_string());
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
            Err(_) => (options.project_number.unwrap_or(DEFAULT_PROJECT_NUMBER), false),
        }
    } else {
        (options.project_number.unwrap_or(DEFAULT_PROJECT_NUMBER), false)
    };

    Ok(InitContext {
        owner,
        project_number,
        project_title,
        repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
        created_project,
    })
}

async fn build_interactive_context(options: &InitOptions) -> Result<InitContext> {
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
        owner,
        project_number,
        project_title,
        repo_name_with_owner: detect_repo_name_with_owner(&options.target_dir).await,
        created_project,
    })
}

async fn detect_current_github_login() -> Option<String> {
    run_gh_capture(
        &[
            "api",
            "user",
            "--jq",
            ".login",
        ],
        Path::new("."),
    )
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
            "project",
            "create",
            "--owner",
            owner,
            "--title",
            title,
            "--format",
            "json",
            "--jq",
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

async fn link_current_repo_to_project(target_dir: &Path, owner: &str, project_number: u32) -> Result<()> {
    let project_number = project_number.to_string();
    run_gh_capture(
        &[
            "project",
            "link",
            project_number.as_str(),
            "--owner",
            owner,
        ],
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

    format!(
        r#"---
tracker:
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
    - Done

polling:
  interval_ms: 30000

workspace:
  root: ./.luna/workspaces

hooks:
  timeout_ms: 60000

agent:
  max_concurrent_agents: 4
  max_turns: 20
  max_retry_backoff_ms: 300000

codex:
  command: codex app-server
  approval_policy: never
  thread_sandbox: workspace-write
  turn_sandbox_policy:
    type: workspaceWrite
---
# Luna Workflow

You are Luna, an autonomous coding agent working on a GitHub Project item.

Project context:
- GitHub Project owner: `{owner}`
- GitHub Project number: `{project_number}`
- GitHub Project title: `{project_title}`
- Open the project in the browser with: `gh project view {project_number} --owner {owner} --web`
- Inspect project items with: `gh project item-list {project_number} --owner {owner} --format json`
- If this item corresponds to a repository issue, inspect it with commands like:
  `gh issue view <number> -R {repo_hint}`
  `gh issue comment <number> -R {repo_hint} --body "..."`
  `gh issue edit <number> -R {repo_hint} ...`

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
- Use `gh project`, `gh issue`, `gh pr`, and git commands when you need to inspect or update GitHub state.
- Validate changes before stopping.
- Move the project item or backing issue to the next workflow-defined handoff state when appropriate.
"#,
        owner = context.owner,
        project_number = context.project_number,
        project_title = context.project_title,
        repo_hint = repo_hint,
    )
}

const ENV_TEMPLATE: &str = r#"# Luna runtime secrets
# `gh` can use these if you don't want to rely on `gh auth login`.
GH_TOKEN=
GITHUB_TOKEN=
"#;

fn print_init_summary(context: &InitContext) {
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
    value.parse::<u32>().map_err(|err| {
        LunaError::InvalidConfig(format!("invalid integer for `{label}`: {err}"))
    })
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
