use clap::Parser;

use crate::messages::MSG_OBSERVABILITY_HELP;

#[derive(Debug, Parser)]
pub struct ContainersArgs {
    #[clap(long, short = 'o', help = MSG_OBSERVABILITY_HELP)]
    pub observability: bool,
}
