use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::{MSG_NO_DEPS_HELP, MSG_NO_KILL_HELP, MSG_TESTS_RECOVERY_SNAPSHOT_HELP};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RecoveryArgs {
    #[clap(short, long, help = MSG_TESTS_RECOVERY_SNAPSHOT_HELP)]
    pub snapshot: bool,
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(short, long, help = MSG_NO_KILL_HELP)]
    pub no_kill: bool,
}
