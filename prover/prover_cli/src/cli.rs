use clap::{command, Parser, Subcommand};

use crate::commands::{self, get_file_info, status};

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
    #[command(subcommand)]
    Status(commands::StatusCommand),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Status(status_cmd) => status::run(status_cmd).await?,
    };

    Ok(())
}
