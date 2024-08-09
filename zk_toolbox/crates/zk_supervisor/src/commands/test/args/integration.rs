use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::{MSG_BUILD_DEPENDENSCIES_HELP, MSG_TESTS_EXTERNAL_NODE_HELP};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct IntegrationArgs {
    #[clap(short, long, help = MSG_TESTS_EXTERNAL_NODE_HELP)]
    pub external_node: bool,
    #[clap(short, long, help = MSG_BUILD_DEPENDENSCIES_HELP)]
    pub no_deps: bool,
}
