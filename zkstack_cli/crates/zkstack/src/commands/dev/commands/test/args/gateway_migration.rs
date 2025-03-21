use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::commands::dev::messages::MSG_NO_DEPS_HELP;

#[derive(Debug, Clone, clap::ValueEnum, Serialize, Deserialize)]
pub enum MigrationDirection {
    From,
    To,
}

#[derive(Debug, Parser)]
pub struct GatewayMigrationArgs {
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(short, long)]
    pub direction: MigrationDirection,
    #[clap(short, long)]
    pub gateway_chain: Option<String>,
}
