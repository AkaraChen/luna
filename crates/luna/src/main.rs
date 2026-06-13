use std::{
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use luna::{
    error::Result,
    init::{InitOptions, run_init},
    job::{JobOptions, JobWorkspaceMode, run_job},
    orchestrator,
    paths::absolutize_path,
    tracker::{
        CommentCommandOptions, MoveCommandOptions, ShowCommandOptions, TrackerTargetOptions,
        run_comment_command, run_move_command, run_show_command,
    },
    wiki::command::{WikiCommandOptions, run_wiki_command},
    workflow::discover_workflow_path,
};

#[derive(Debug, Parser)]
#[command(name = "luna")]
#[command(about = "Long-running coding-agent orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(value_name = "WORKFLOW.md")]
    workflow: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(about = "Initialize a default WORKFLOW.md in a directory")]
    Init {
        #[arg(default_value = ".")]
        dir: PathBuf,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long)]
        project_number: Option<u32>,
        #[arg(long)]
        create_project: bool,
        #[arg(long)]
        project_title: Option<String>,
        #[arg(long)]
        non_interactive: bool,
        #[arg(long, help = "Tracker kind: github_project or asahi")]
        tracker: Option<String>,
    },
    #[command(about = "Post a comment to the current tracker item")]
    Comment {
        #[arg(
            long,
            help = "Path to WORKFLOW.md. Defaults to the nearest WORKFLOW.md or workflow.md in the current directory tree."
        )]
        workflow: Option<PathBuf>,
        #[arg(
            long,
            help = "Explicit issue locator. If omitted, Luna resolves the current issue from LUNA_ISSUE_* env vars or the current workspace."
        )]
        issue: Option<String>,
        #[arg(
            value_name = "BODY",
            help = "Comment body. If omitted, Luna reads from stdin when stdin is piped."
        )]
        body: Vec<String>,
    },
    #[command(about = "Show the current tracker item")]
    Show {
        #[arg(
            long,
            help = "Path to WORKFLOW.md. Defaults to the nearest WORKFLOW.md or workflow.md in the current directory tree."
        )]
        workflow: Option<PathBuf>,
        #[arg(
            long,
            help = "Explicit issue locator. If omitted, Luna resolves the current issue from LUNA_ISSUE_* env vars or the current workspace."
        )]
        issue: Option<String>,
        #[arg(long, help = "Print the issue as JSON instead of human-readable text.")]
        json: bool,
    },
    #[command(about = "Move the current tracker item to a new state")]
    Move {
        #[arg(
            long,
            help = "Path to WORKFLOW.md. Defaults to the nearest WORKFLOW.md or workflow.md in the current directory tree."
        )]
        workflow: Option<PathBuf>,
        #[arg(
            long,
            help = "Explicit issue locator. If omitted, Luna resolves the current issue from LUNA_ISSUE_* env vars or the current workspace."
        )]
        issue: Option<String>,
        #[arg(value_name = "STATE", help = "Target tracker state name.")]
        state: String,
    },
    #[command(about = "Run a one-off Angel Engine job and stream TurnRunEvent JSONL")]
    Job {
        #[arg(
            long,
            help = "Path to WORKFLOW.md. Defaults to the nearest WORKFLOW.md or workflow.md in the current directory tree."
        )]
        workflow: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = JobWorkspaceMode::None)]
        workspace: JobWorkspaceMode,
        #[arg(value_name = "PROMPT")]
        prompt: Vec<String>,
    },
    #[command(about = "Browse the current issue's project wiki via a virtual shell")]
    Wiki {
        #[arg(
            long,
            help = "Path to WORKFLOW.md. Defaults to the nearest WORKFLOW.md or workflow.md in the current directory tree."
        )]
        workflow: Option<PathBuf>,
        #[arg(
            long,
            help = "Explicit issue locator. If omitted, Luna resolves the current issue from LUNA_ISSUE_* env vars or the current workspace."
        )]
        issue: Option<String>,
        #[arg(
            value_name = "SHELL_COMMAND",
            help = "Shell command to run against the wiki filesystem (e.g. ls, tree, cat, find)"
        )]
        args: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logging();
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Init {
            dir,
            force,
            owner,
            project_number,
            create_project,
            project_title,
            non_interactive,
            tracker,
        }) => {
            let created = run_init(InitOptions {
                target_dir: dir,
                force,
                owner,
                project_number,
                create_project,
                project_title,
                non_interactive,
                tracker_kind: tracker,
            })
            .await?;
            for path in created {
                println!("{}", path.display());
            }
            Ok(())
        }
        Some(Commands::Comment {
            workflow,
            issue,
            body,
        }) => {
            let target = resolve_tracker_target(workflow, issue)?;
            load_dotenv_file(&target.workflow_path)?;

            let identifier = run_comment_command(CommentCommandOptions {
                target,
                body: read_comment_body(body)?,
            })
            .await?;
            println!("commented on {identifier}");
            Ok(())
        }
        Some(Commands::Show {
            workflow,
            issue,
            json,
        }) => {
            let target = resolve_tracker_target(workflow, issue)?;
            load_dotenv_file(&target.workflow_path)?;
            println!(
                "{}",
                run_show_command(ShowCommandOptions { target, json }).await?
            );
            Ok(())
        }
        Some(Commands::Move {
            workflow,
            issue,
            state,
        }) => {
            let target = resolve_tracker_target(workflow, issue)?;
            load_dotenv_file(&target.workflow_path)?;
            let identifier = run_move_command(MoveCommandOptions { target, state }).await?;
            println!("moved {identifier}");
            Ok(())
        }
        Some(Commands::Job {
            workflow,
            workspace,
            prompt,
        }) => {
            let cwd = std::env::current_dir()?;
            let workflow_path = match workflow {
                Some(path) => absolutize_path(&path)?,
                None => discover_workflow_path(&cwd)?,
            };
            load_dotenv_file(&workflow_path)?;
            run_job(JobOptions {
                workflow_path,
                prompt: read_job_prompt(prompt)?,
                workspace,
            })
            .await
        }
        Some(Commands::Wiki {
            workflow,
            issue,
            args,
        }) => {
            let target = resolve_tracker_target(workflow, issue)?;
            load_dotenv_file(&target.workflow_path)?;
            let result = run_wiki_command(WikiCommandOptions { target, args }).await?;
            print!("{}", result.stdout);
            if !result.stderr.is_empty() {
                eprint!("{}", result.stderr);
            }
            if result.exit_code != 0 {
                std::process::exit(result.exit_code);
            }
            Ok(())
        }
        None => {
            let workflow_path =
                absolutize_path(&cli.workflow.unwrap_or_else(|| PathBuf::from("WORKFLOW.md")))?;
            load_dotenv_file(&workflow_path)?;
            orchestrator::run(workflow_path).await
        }
    }
}

fn resolve_tracker_target(
    workflow: Option<PathBuf>,
    issue: Option<String>,
) -> Result<TrackerTargetOptions> {
    let cwd = std::env::current_dir()?;
    let workflow_path = match workflow {
        Some(path) => absolutize_path(&path)?,
        None => discover_workflow_path(&cwd)?,
    };

    Ok(TrackerTargetOptions {
        workflow_path,
        issue_locator: issue,
        cwd,
    })
}

fn read_job_prompt(args: Vec<String>) -> Result<String> {
    read_text_arg_or_stdin(
        args,
        "job prompt is required; pass text or pipe stdin to `luna job`",
    )
}

fn read_comment_body(args: Vec<String>) -> Result<String> {
    read_text_arg_or_stdin(
        args,
        "comment body is required; pass text or pipe stdin to `luna comment`",
    )
}

fn read_text_arg_or_stdin(args: Vec<String>, missing_message: &str) -> Result<String> {
    let body = if args.is_empty() {
        if io::stdin().is_terminal() {
            String::new()
        } else {
            let mut stdin = String::new();
            io::stdin().read_to_string(&mut stdin)?;
            stdin
        }
    } else {
        args.join(" ")
    };

    let body = body.trim().to_string();
    if body.is_empty() {
        return Err(luna::error::LunaError::InvalidConfig(
            missing_message.to_string(),
        ));
    }

    Ok(body)
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_writer(io::stderr)
        .try_init();
}

fn load_dotenv_file(workflow_path: &Path) -> Result<()> {
    let workflow_dir = workflow_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let env_path = workflow_dir.join(".env.luna");
    if env_path.exists() {
        dotenvy::from_path_override(env_path).map_err(|err| {
            luna::error::LunaError::InvalidConfig(format!("failed to load .env.luna: {err}"))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::{
        Cli, Commands, JobWorkspaceMode, load_dotenv_file, read_comment_body, read_job_prompt,
        read_text_arg_or_stdin, resolve_tracker_target,
    };

    #[test]
    fn cli_parses_codex_workflow_subcommands() {
        let cli = Cli::try_parse_from([
            "luna",
            "init",
            "repo",
            "--tracker",
            "asahi",
            "--non-interactive",
            "--project-title",
            "Local Backlog",
        ])
        .expect("init cli");
        match cli.command {
            Some(Commands::Init {
                dir,
                tracker,
                non_interactive,
                project_title,
                ..
            }) => {
                assert_eq!(dir, std::path::PathBuf::from("repo"));
                assert_eq!(tracker.as_deref(), Some("asahi"));
                assert!(non_interactive);
                assert_eq!(project_title.as_deref(), Some("Local Backlog"));
            }
            other => panic!("expected init command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "luna",
            "comment",
            "--workflow",
            "WORKFLOW.md",
            "--issue",
            "ASAHI-1",
            "ship",
            "it",
        ])
        .expect("comment cli");
        match cli.command {
            Some(Commands::Comment {
                workflow,
                issue,
                body,
            }) => {
                assert_eq!(workflow.unwrap(), std::path::PathBuf::from("WORKFLOW.md"));
                assert_eq!(issue.as_deref(), Some("ASAHI-1"));
                assert_eq!(body, vec!["ship", "it"]);
            }
            other => panic!("expected comment command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["luna", "show", "--issue", "ASAHI-1", "--json"])
            .expect("show cli");
        match cli.command {
            Some(Commands::Show { issue, json, .. }) => {
                assert_eq!(issue.as_deref(), Some("ASAHI-1"));
                assert!(json);
            }
            other => panic!("expected show command, got {other:?}"),
        }

        let cli =
            Cli::try_parse_from(["luna", "move", "--issue", "ASAHI-1", "Done"]).expect("move cli");
        match cli.command {
            Some(Commands::Move { issue, state, .. }) => {
                assert_eq!(issue.as_deref(), Some("ASAHI-1"));
                assert_eq!(state, "Done");
            }
            other => panic!("expected move command, got {other:?}"),
        }

        let cli = Cli::try_parse_from([
            "luna",
            "job",
            "--workspace",
            "repo",
            "inspect",
            "repository",
        ])
        .expect("job cli");
        match cli.command {
            Some(Commands::Job {
                workspace, prompt, ..
            }) => {
                assert_eq!(workspace, JobWorkspaceMode::Repo);
                assert_eq!(prompt, vec!["inspect", "repository"]);
            }
            other => panic!("expected job command, got {other:?}"),
        }

        let cli = Cli::try_parse_from(["luna", "wiki", "--issue", "ASAHI-1", "grep", "API"])
            .expect("wiki cli");
        match cli.command {
            Some(Commands::Wiki { issue, args, .. }) => {
                assert_eq!(issue.as_deref(), Some("ASAHI-1"));
                assert_eq!(args, vec!["grep", "API"]);
            }
            other => panic!("expected wiki command, got {other:?}"),
        }
    }

    #[test]
    fn cli_without_subcommand_preserves_optional_workflow_path() {
        let cli = Cli::try_parse_from(["luna", "custom-WORKFLOW.md"]).expect("daemon cli");

        assert!(cli.command.is_none());
        assert_eq!(
            cli.workflow.as_deref(),
            Some(std::path::Path::new("custom-WORKFLOW.md"))
        );
    }

    #[test]
    fn cli_text_args_join_trim_and_report_missing_prompt_or_comment() {
        assert_eq!(
            read_job_prompt(vec!["  inspect".to_string(), "repo  ".to_string()]).unwrap(),
            "inspect repo"
        );
        assert_eq!(
            read_comment_body(vec!["  ship".to_string(), "it  ".to_string()]).unwrap(),
            "ship it"
        );
        assert_eq!(
            read_text_arg_or_stdin(vec!["  hello ".to_string()], "missing").unwrap(),
            "hello"
        );

        let job_err = read_job_prompt(vec!["  ".to_string()]).unwrap_err();
        assert!(job_err.to_string().contains("job prompt is required"));
        let comment_err = read_comment_body(vec!["  ".to_string()]).unwrap_err();
        assert!(comment_err.to_string().contains("comment body is required"));
    }

    #[test]
    fn resolve_tracker_target_absolutizes_explicit_workflow_and_preserves_issue() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workflow = temp.path().join("WORKFLOW.md");
        std::fs::write(&workflow, "---\n---\n").expect("workflow");

        let target = resolve_tracker_target(Some(workflow.clone()), Some("ASAHI-1".to_string()))
            .expect("target");

        assert!(target.workflow_path.is_absolute());
        assert!(target.workflow_path.ends_with("WORKFLOW.md"));
        assert_eq!(target.issue_locator.as_deref(), Some("ASAHI-1"));
        assert!(target.cwd.is_absolute());
    }

    #[test]
    fn load_dotenv_file_loads_env_from_workflow_directory_and_ignores_missing_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workflow = temp.path().join("WORKFLOW.md");
        std::fs::write(&workflow, "---\n---\n").expect("workflow");
        let key = format!("LUNA_TEST_ENV_{}", std::process::id());
        std::fs::write(temp.path().join(".env.luna"), format!("{key}=loaded\n")).expect("env");

        load_dotenv_file(&workflow).expect("load dotenv");

        assert_eq!(std::env::var(&key).as_deref(), Ok("loaded"));
        load_dotenv_file(&temp.path().join("nested/WORKFLOW.md")).expect("missing dotenv ignored");
    }
}
