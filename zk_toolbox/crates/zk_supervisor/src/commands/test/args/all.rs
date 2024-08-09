use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::MSG_BUILD_DEPENDENSCIES_HELP;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct AllArgs {
    #[clap(short, long, help = MSG_BUILD_DEPENDENSCIES_HELP)]
    pub no_deps: bool,
}
