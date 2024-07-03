use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::MSG_TESTS_EXTERNAL_NODE_HELP;

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct IntegrationArgs {
    #[clap(short, long, help = MSG_TESTS_EXTERNAL_NODE_HELP)]
    pub external_node: bool,
}
