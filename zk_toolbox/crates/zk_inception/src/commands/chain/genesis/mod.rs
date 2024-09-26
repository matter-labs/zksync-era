use clap::Subcommand;

use crate::commands::chain::args::genesis::GenesisArgs;

pub mod database;
pub mod server;

#[derive(Subcommand, Debug, Clone)]
pub enum GenesisSubcommands {
    /// Initialize database
    #[command(alias = "db")]
    InitDatabase(GenesisArgs),
    /// Runs server genesis
    Server,
}
