use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::{
    MSG_SERVER_ADDITIONAL_ARGS_HELP, MSG_SERVER_BUILD_HELP, MSG_SERVER_COMPONENTS_HELP,
    MSG_SERVER_GENESIS_HELP, MSG_SERVER_URING_HELP,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RunServerArgs {
    #[clap(long, help = MSG_SERVER_COMPONENTS_HELP)]
    pub components: Option<Vec<String>>,
    #[clap(long, help = MSG_SERVER_GENESIS_HELP)]
    pub genesis: bool,
    #[clap(long, short)]
    #[arg(trailing_var_arg = true, allow_hyphen_values = true, hide = false, help = MSG_SERVER_ADDITIONAL_ARGS_HELP)]
    additional_args: Vec<String>,
    #[clap(long, help = MSG_SERVER_BUILD_HELP)]
    pub build: bool,
    #[clap(help=MSG_SERVER_URING_HELP, long, default_missing_value = "true", num_args = 0..=1)]
    pub uring: bool,
}
