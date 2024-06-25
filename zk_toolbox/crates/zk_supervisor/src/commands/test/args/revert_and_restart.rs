use clap::Parser;

use crate::messages::MSG_DATABASE_COMMON_PROVER_HELP;

#[derive(Debug, Parser)]
pub struct RevertAndRestartArgs {
    #[clap(long, help = MSG_DATABASE_COMMON_PROVER_HELP)]
    pub enable_consensus: bool,
}
