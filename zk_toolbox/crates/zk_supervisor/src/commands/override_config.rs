use clap::Parser;
use xshell::Shell;

use crate::messages::MSG_OVERRIDE_CONFIG_PATH_HELP;

#[derive(Debug, Parser)]
pub struct OverrideConfigArgs {
    #[clap(long, short, help = MSG_OVERRIDE_CONFIG_PATH_HELP)]
    pub path: Option<String>,
}

pub fn run(shell: &Shell, args: OverrideConfigArgs) -> anyhow::Result<()> {
    Ok(())
}
