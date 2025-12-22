use clap::Parser;

use crate::commands::dev::messages::MSG_NO_DEPS_HELP;

#[derive(Debug, Parser)]
pub struct TokenBalanceMigrationArgs {
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(short, long)]
    pub gateway_chain: Option<String>,
}
