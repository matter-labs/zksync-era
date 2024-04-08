use clap::{command, Parser, Subcommand};

use crate::commands::{get_proof_progress, status};

pub const VERSION_STRING: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(name="prover-cli", version=VERSION_STRING, about, long_about = None)]
struct ProverCLI {
    #[command(subcommand)]
    command: ProverCommand,
}

#[derive(Subcommand)]
enum ProverCommand {
    Status,
    GetProofProgress,
}

pub async fn start() -> eyre::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::Status => status::run().await?,
        ProverCommand::GetProofProgress => get_proof_progress::run().await?,
    };

    Ok(())
}
