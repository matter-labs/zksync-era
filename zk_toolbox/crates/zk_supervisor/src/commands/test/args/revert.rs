use clap::Parser;

use crate::messages::{
    MSG_NO_DEPS_HELP, MSG_NO_KILL_HELP, MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP,
    MSG_TESTS_EXTERNAL_NODE_HELP,
};

#[derive(Debug, Parser)]
pub struct RevertArgs {
    #[clap(long, help = MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP)]
    pub enable_consensus: bool,
    #[clap(short, long, help = MSG_TESTS_EXTERNAL_NODE_HELP)]
    pub external_node: bool,
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(short, long, help = MSG_NO_KILL_HELP)]
    pub auto_kill: bool,
}
