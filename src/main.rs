use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt};

use luna::{
    error::Result,
    init::{InitOptions, run_init},
    orchestrator,
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
        }) => {
            let created = run_init(InitOptions {
                target_dir: dir,
                force,
                owner,
                project_number,
                create_project,
                project_title,
                non_interactive,
            })
            .await?;
            for path in created {
                println!("{}", path.display());
            }
            Ok(())
        }
        None => {
            let workflow_path = cli.workflow.unwrap_or_else(|| PathBuf::from("WORKFLOW.md"));
            load_dotenv_file(&workflow_path)?;
            orchestrator::run(workflow_path).await
        }
    }
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
