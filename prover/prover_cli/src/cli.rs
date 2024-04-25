use clap::{command, Parser, Subcommand};

use crate::commands::{get_file_info, requeue};

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
    Requeue(requeue::Args),
}

pub async fn start() -> anyhow::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::FileInfo(args) => get_file_info::run(args).await?,
        ProverCommand::Requeue(args) => requeue::run(args).await?,
    };

    Ok(())
}
