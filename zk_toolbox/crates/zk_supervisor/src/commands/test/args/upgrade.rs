use clap::Parser;

use crate::messages::MSG_BUILD_DEPENDENSCIES_HELP;

#[derive(Debug, Parser)]
pub struct UpgradeArgs {
    #[clap(short, long, help = MSG_BUILD_DEPENDENSCIES_HELP)]
    pub no_deps: bool,
}
