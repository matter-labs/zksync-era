use clap::{command, Parser, Subcommand};
use xshell::Shell;

use crate::commands::chain::args::init::{configs::InitConfigsArgs, InitArgs};

// Standard init command
mod init;
pub use init::*;
// Init subcommands
pub mod configs;

#[derive(Subcommand, Debug, Clone)]
pub enum ChainInitSubcommands {
    /// Initialize chain configs
    Configs(InitConfigsArgs),
}

#[derive(Parser, Debug)]
#[command()]
pub struct ChainInitCommand {
    #[command(subcommand)]
    command: Option<ChainInitSubcommands>,
    #[clap(flatten)]
    args: InitArgs,
}

pub(crate) async fn run(args: ChainInitCommand, shell: &Shell) -> anyhow::Result<()> {
    match args.command {
        Some(ChainInitSubcommands::Configs(args)) => configs::run(args, shell).await,
        None => init::run(args.args, shell).await,
    }
}
