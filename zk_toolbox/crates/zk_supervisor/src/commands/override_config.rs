use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use common::{config::global_config, logger, yaml::merge_yaml, Prompt};
use config::{ChainConfig, EcosystemConfig};
use xshell::Shell;

use crate::messages::{
    msg_overriding_config, MSG_CHAIN_NOT_FOUND_ERR, MSG_OVERRIDE_CONFIG_PATH_HELP,
    MSG_OVERRIDE_SUCCESS, MSG_OVERRRIDE_CONFIG_PATH_PROMPT,
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
    logger::step(msg_overriding_config(chain.name.clone()));
    override_config(shell, path, &chain)?;
    logger::outro(MSG_OVERRIDE_SUCCESS);
    Ok(())
}

pub fn override_config(shell: &Shell, path: PathBuf, chain: &ChainConfig) -> anyhow::Result<()> {
    let chain_config_path = chain.path_to_general_config();
    let override_config = serde_yaml::from_str(&shell.read_file(path)?)?;
    let mut chain_config = serde_yaml::from_str(&shell.read_file(chain_config_path.clone())?)?;
    merge_yaml(&mut chain_config, override_config, true)?;
    shell.write_file(chain_config_path, serde_yaml::to_string(&chain_config)?)?;
    Ok(())
}
