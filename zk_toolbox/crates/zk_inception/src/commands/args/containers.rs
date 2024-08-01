use clap::Parser;

use crate::messages::MSG_RUN_OBSERVABILITY_HELP;

#[derive(Debug, Parser)]
pub struct ContainersArgs {
    #[clap(long, short = 'o', help = MSG_RUN_OBSERVABILITY_HELP)]
    pub run_observability: bool,
}
