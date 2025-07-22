use clap::Parser;

use crate::commands::dev::messages::{MSG_NO_DEPS_HELP, MSG_NO_KILL_HELP};

#[derive(Debug, Parser)]
pub struct RevertArgs {
    #[clap(short, long, help = MSG_NO_DEPS_HELP)]
    pub no_deps: bool,
    #[clap(long, help = MSG_NO_KILL_HELP)]
    pub no_kill: bool,
}
