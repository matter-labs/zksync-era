use std::{path::PathBuf, str::FromStr};

use anyhow::Context;
use clap::Parser;
use xshell::Shell;
use zkstack_cli_common::{logger, Prompt};
use zkstack_cli_config::{override_config, EcosystemConfig};

use crate::commands::dev::messages::{
    msg_overriding_config, MSG_CHAIN_NOT_FOUND_ERR, MSG_OVERRIDE_CONFIG_PATH_HELP,
    MSG_OVERRIDE_SUCCESS, MSG_OVERRRIDE_CONFIG_PATH_PROMPT,
};

#[derive(Debug, Parser)]
pub struct ConfigWriterArgs {
    #[clap(long, short, help = MSG_OVERRIDE_CONFIG_PATH_HELP)]
    pub path: Option<String>,
}

impl ConfigWriterArgs {
    pub fn get_config_path(self) -> String {
        self.path
            .unwrap_or_else(|| Prompt::new(MSG_OVERRRIDE_CONFIG_PATH_PROMPT).ask())
    }
}

pub fn run(shell: &Shell, args: ConfigWriterArgs) -> anyhow::Result<()> {
    let path = PathBuf::from_str(&args.get_config_path())?;
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_current_chain()
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    logger::step(msg_overriding_config(chain.name.clone()));
    override_config(shell, &path, &chain)?;
    logger::outro(MSG_OVERRIDE_SUCCESS);
    Ok(())
}
