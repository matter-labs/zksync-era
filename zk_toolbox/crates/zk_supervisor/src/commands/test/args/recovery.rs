use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::MSG_TESTS_RECOVERY_SNAPSHOT_HELP;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RecoveryArgs {
    #[clap(short, long, help = MSG_TESTS_RECOVERY_SNAPSHOT_HELP)]
    pub snapshot: bool,
}
