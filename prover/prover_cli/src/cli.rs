use clap::{command, Parser, Subcommand};

use crate::commands::{self, delete, get_file_info};

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
    Delete(delete::Args),
    #[command(subcommand)]
    Status(commands::StatusCommand),
    #[command(subcommand)]
    Restart(commands::RestartCommand),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Delete(args) => delete::run(args).await?,
        ProverCommand::Status(cmd) => cmd.run().await?,
        ProverCommand::Restart(cmd) => cmd.run().await?,
    };

    Ok(())
}
