use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use common::{config::global_config, Prompt};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use crate::messages::{
    MSG_CHAIN_NOT_FOUND_ERR, MSG_OVERRIDE_CONFIG_PATH_HELP, MSG_OVERRRIDE_CONFIG_PATH_PROMPT,
};

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
    let path = args.get_config_path().into();
    let ecosystem = EcosystemConfig::from_file(shell)?;
    let chain = ecosystem
        .load_chain(global_config().chain_name.clone())
        .context(MSG_CHAIN_NOT_FOUND_ERR)?;
    override_config(path, &chain)?;
    Ok(())
}

pub fn override_config(path: PathBuf, chain: &ChainConfig) -> anyhow::Result<()> {
    Ok(())
}
