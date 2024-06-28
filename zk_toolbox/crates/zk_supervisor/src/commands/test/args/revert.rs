use clap::Parser;

use crate::messages::MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP;

#[derive(Debug, Parser)]
pub struct RevertArgs {
    #[clap(long, help = MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP)]
    pub enable_consensus: bool,
}
