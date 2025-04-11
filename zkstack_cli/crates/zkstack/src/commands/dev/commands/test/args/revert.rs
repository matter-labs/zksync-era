use clap::Parser;

use crate::commands::dev::messages::{
    MSG_NO_DEPS_HELP, MSG_NO_KILL_HELP, MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP,
};

#[derive(Debug, Parser)]
pub struct RevertArgs {
    #[clap(long, help = MSG_REVERT_TEST_ENABLE_CONSENSUS_HELP)]
    pub enable_consensus: bool,
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(long, help = MSG_NO_KILL_HELP)]
    pub no_kill: bool,
}
