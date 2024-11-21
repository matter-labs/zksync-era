use clap::Parser;
use serde::{Deserialize, Serialize};

use crate::messages::{
    MSG_SERVER_ADDITIONAL_ARGS_HELP, MSG_SERVER_COMPONENTS_HELP, MSG_SERVER_GENESIS_HELP,
    MSG_SERVER_URING_HELP,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct RunServerArgs {
    #[arg(long, help = MSG_SERVER_COMPONENTS_HELP)]
    pub components: Option<Vec<String>>,
    #[arg(long, help = MSG_SERVER_GENESIS_HELP)]
    pub genesis: bool,
    #[arg(
        long, short,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        hide = false,
        help = MSG_SERVER_ADDITIONAL_ARGS_HELP
    )]
    additional_args: Vec<String>,
    #[clap(help = MSG_SERVER_URING_HELP, long, default_missing_value = "true")]
    pub uring: bool,
}
