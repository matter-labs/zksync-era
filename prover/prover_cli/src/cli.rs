use crate::commands::{get_file_info, jobs_status};
use clap::{command, Parser, Subcommand};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name="prover-cli", version=VERSION_STRING, about, long_about = None)]
struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
}

#[derive(Subcommand)]
enum ProverCommand {
    FileInfo(get_file_info::Args),
    StatusJobs(jobs_status::Args),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::StatusJobs(args) => jobs_status::run(args).await?,
    };

    Ok(())
}
