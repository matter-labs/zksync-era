use clap::Parser;

use crate::messages::MSG_UPDATE_ONLY_CONFIG_HELP;

#[derive(Debug, Parser)]
pub struct UpdateArgs {
    #[clap(long, short = 'c', help = MSG_UPDATE_ONLY_CONFIG_HELP)]
    pub only_config: bool,
}
