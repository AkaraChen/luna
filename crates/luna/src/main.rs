use std::{
    io::{self, IsTerminal, Read},
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use luna::{
    error::Result,
    init::{InitOptions, run_init},
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
        Some(Commands::Wiki { workflow, issue, args }) => {
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

fn read_comment_body(args: Vec<String>) -> Result<String> {
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
            "comment body is required; pass text or pipe stdin to `luna comment`".to_string(),
        ));
    }

    Ok(body)
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = fmt().with_env_filter(filter).with_target(false).try_init();
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
