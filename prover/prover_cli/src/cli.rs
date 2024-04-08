use crate::commands::status;

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
    Status,
}

pub async fn start() -> eyre::Result<()> {
    let ProverCLI { command } = ProverCLI::parse();
    match command {
        ProverCommand::Status => status::run().await?,
    };

    Ok(())
}
