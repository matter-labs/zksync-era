use clap::{Parser, Subcommand};
use run::RunServerArgs;

use crate::commands::args::WaitArgs;

pub mod run;

#[derive(Debug, Parser)]
#[command(args_conflicts_with_subcommands = true, flatten_help = true)]
pub struct ServerArgs {
    #[command(subcommand)]
    command: Option<ServerCommand>,
    #[command(flatten)]
    run: RunServerArgs,
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// Builds server
    Build,
    /// Runs server
    Run(RunServerArgs),
    /// Waits for server to start
    Wait(WaitArgs),
}

impl From<ServerArgs> for ServerCommand {
    fn from(args: ServerArgs) -> Self {
        args.command.unwrap_or(ServerCommand::Run(args.run))
    }
}
