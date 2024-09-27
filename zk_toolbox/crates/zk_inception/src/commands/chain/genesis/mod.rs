use clap::{command, Parser, Subcommand};
use xshell::Shell;

use crate::commands::chain::args::genesis::GenesisArgs;

// Standard genesis command
mod genesis;
pub use genesis::*;
// Genesis subcommands
pub mod database;
pub mod server;

#[derive(Subcommand, Debug, Clone)]
pub enum GenesisSubcommands {
    /// Initialize databases
    #[command(alias = "db")]
    InitDatabase(GenesisArgs),
    /// Runs server genesis
    Server,
}

#[derive(Parser, Debug)]
#[command()]
pub struct GenesisCommand {
    #[command(subcommand)]
    command: Option<GenesisSubcommands>,
    #[clap(flatten)]
    args: GenesisArgs,
}

pub(crate) async fn run(args: GenesisCommand, shell: &Shell) -> anyhow::Result<()> {
    match args.command {
        Some(GenesisSubcommands::InitDatabase(args)) => database::run(args, shell).await,
        Some(GenesisSubcommands::Server) => server::run(shell).await,
        None => genesis::run(args.args, shell).await,
    }
}
