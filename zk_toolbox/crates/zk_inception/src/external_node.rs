use std::path::PathBuf;

use anyhow::Context;
use config::{
    external_node::ENConfig, traits::FileConfigWithDefaultName, ChainConfig, GeneralConfig,
    SecretsConfig,
};
use xshell::Shell;

use crate::messages::MSG_FAILED_TO_RUN_SERVER_ERR;

pub struct RunExternalNode {
    components: Option<Vec<String>>,
    code_path: PathBuf,
    general_config: PathBuf,
    secrets: PathBuf,
    en_config: PathBuf,
}

impl RunExternalNode {
    pub fn new(
        components: Option<Vec<String>>,
        chain_config: &ChainConfig,
    ) -> anyhow::Result<Self> {
        let en_path = chain_config
            .external_node_config_path
            .clone()
            .context("External node is not initialized")?;
        let general_config = GeneralConfig::get_path_with_base_path(&en_path);
        let secrets = SecretsConfig::get_path_with_base_path(&en_path);
        let enconfig = ENConfig::get_path_with_base_path(&en_path);

        Ok(Self {
            components,
            code_path: chain_config.link_to_code.clone(),
            general_config,
            secrets,
            en_config: enconfig,
        })
    }

    pub fn run(&self, shell: &Shell, mut additional_args: Vec<String>) -> anyhow::Result<()> {
        let code_path = self.code_path.to_str().unwrap();
        let config_general_config = &self.general_config.to_str().unwrap();
        let en_config = &self.en_config.to_str().unwrap();
        let secrets = &self.secrets.to_str().unwrap();
        if let Some(components) = self.components() {
            additional_args.push(format!("--components={}", components))
        }

        common::external_node::run(
            shell,
            code_path,
            config_general_config,
            secrets,
            en_config,
            additional_args,
        )
        .context(MSG_FAILED_TO_RUN_SERVER_ERR)
    }

    fn components(&self) -> Option<String> {
        self.components.as_ref().and_then(|components| {
            if components.is_empty() {
                return None;
            }
            Some(components.join(","))
        })
    }
}
