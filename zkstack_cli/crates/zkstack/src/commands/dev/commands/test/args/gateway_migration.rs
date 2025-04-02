use clap::Parser;

use crate::commands::dev::messages::MSG_NO_DEPS_HELP;

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
pub struct Direction {
    #[clap(long, default_missing_value = "true")]
    pub from_gateway: bool,
    #[clap(long, default_missing_value = "true")]
    pub to_gateway: bool,
}

#[derive(Debug, Parser)]
pub struct GatewayMigrationArgs {
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(flatten)]
    pub direction: Direction,
    #[clap(short, long)]
    pub gateway_chain: Option<String>,
}
