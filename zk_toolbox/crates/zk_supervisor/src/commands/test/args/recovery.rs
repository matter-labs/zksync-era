use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::{MSG_BUILD_DEPENDENSCIES_HELP, MSG_TESTS_RECOVERY_SNAPSHOT_HELP};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RecoveryArgs {
    #[clap(short, long, help = MSG_TESTS_RECOVERY_SNAPSHOT_HELP)]
    pub snapshot: bool,
    #[clap(short, long, help = MSG_BUILD_DEPENDENSCIES_HELP)]
    pub no_deps: bool,
}
