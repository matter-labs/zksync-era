use clap::Parser;

use crate::messages::MSG_NO_DEPS_HELP;

#[derive(Debug, Parser)]
pub struct UpgradeArgs {
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
}
