use clap::Parser;

use crate::messages::{MSG_SERVER_ADDITIONAL_ARGS_HELP, MSG_SERVER_COMPONENTS_HELP};

#[derive(Debug, Parser)]
pub struct RunExternalNodeArgs {
    #[arg(long)]
    pub reinit: bool,
    #[arg(long, help = MSG_SERVER_COMPONENTS_HELP)]
    pub components: Option<Vec<String>>,
    #[arg(last = true, help = MSG_SERVER_ADDITIONAL_ARGS_HELP)]
    pub additional_args: Vec<String>,
}
