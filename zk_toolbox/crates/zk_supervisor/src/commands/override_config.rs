use clap::Parser;
use common::Prompt;
use xshell::Shell;

use crate::messages::{MSG_OVERRIDE_CONFIG_PATH_HELP, MSG_OVERRRIDE_CONFIG_PATH_PROMPT};

#[derive(Debug, Parser)]
pub struct OverrideConfigArgs {
    #[clap(long, short, help = MSG_OVERRIDE_CONFIG_PATH_HELP)]
    pub path: Option<String>,
}

impl OverrideConfigArgs {
    pub fn get_config_path(self) -> String {
        self.path
            .unwrap_or_else(|| Prompt::new(MSG_OVERRRIDE_CONFIG_PATH_PROMPT).ask())
    }
}

pub fn run(shell: &Shell, args: OverrideConfigArgs) -> anyhow::Result<()> {
    let path = args.get_config_path();
    Ok(())
}
