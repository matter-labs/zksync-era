use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::{
    commands::args::WaitArgs,
    messages::{
        MSG_SERVER_ADDITIONAL_ARGS_HELP, MSG_SERVER_COMMAND_HELP, MSG_SERVER_COMPONENTS_HELP,
        MSG_SERVER_GENESIS_HELP, MSG_SERVER_URING_HELP,
    },
};

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

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RunServerArgs {
    #[arg(long, help = MSG_SERVER_COMPONENTS_HELP)]
    pub components: Option<Vec<String>>,
    #[arg(long, help = MSG_SERVER_GENESIS_HELP)]
    pub genesis: bool,
    #[arg(
        long, short,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        hide = false,
        help = MSG_SERVER_ADDITIONAL_ARGS_HELP
    )]
    additional_args: Vec<String>,
    #[clap(help = MSG_SERVER_URING_HELP, long, default_missing_value = "true")]
    pub uring: bool,
    #[clap(help = MSG_SERVER_URING_HELP, long, default_missing_value = "true")]
    pub no_zkos: bool,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
}
