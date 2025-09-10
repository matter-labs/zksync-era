use std::path::PathBuf;

use anyhow::Context;
use xshell::Shell;
use zkstack_cli_config::{
    ChainConfig, ZkStackConfigTrait, CONSENSUS_CONFIG_FILE, EN_CONFIG_FILE, GENERAL_FILE,
    SECRETS_FILE,
};

use crate::messages::MSG_FAILED_TO_RUN_SERVER_ERR;

pub struct RunExternalNode {
    components: Option<Vec<String>>,
    code_path: PathBuf,
    general_config: PathBuf,
    secrets: PathBuf,
    en_config: PathBuf,
    consensus_config: PathBuf,
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
        let general_config = en_path.join(GENERAL_FILE);
        let secrets = en_path.join(SECRETS_FILE);
        let en_config = en_path.join(EN_CONFIG_FILE);
        let consensus_config = en_path.join(CONSENSUS_CONFIG_FILE);

        Ok(Self {
            components,
            code_path: chain_config.link_to_code().clone(),
            general_config,
            secrets,
            en_config,
            consensus_config,
        })
    }

    pub fn run(
        &self,
        shell: &Shell,
        enable_consensus: bool,
        additional_args: Vec<String>,
    ) -> anyhow::Result<()> {
        let code_path = self.code_path.to_str().unwrap();
        let config_general_config = &self.general_config.to_str().unwrap();
        let en_config = &self.en_config.to_str().unwrap();
        let secrets = &self.secrets.to_str().unwrap();
        let consensus_config = &self.consensus_config.to_str().unwrap();

        let mut passed_args = vec![];
        if let Some(components) = self.components() {
            passed_args.push(format!("--components={}", components))
        }
        if enable_consensus {
            passed_args.push("--enable-consensus".to_string());
            passed_args.push(format!("--consensus-path={consensus_config}"))
        }
        // Need to insert the additional args at the end, since they may include positional ones
        passed_args.extend(additional_args);

        zkstack_cli_common::external_node::run(
            shell,
            code_path,
            config_general_config,
            secrets,
            en_config,
            passed_args,
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
