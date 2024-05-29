use std::path::PathBuf;

use anyhow::Context;
use common::cmd::Cmd;
use xshell::{cmd, Shell};

use crate::{
    configs::ChainConfig,
    consts::{CONTRACTS_FILE, GENERAL_FILE, GENESIS_FILE, SECRETS_FILE, WALLETS_FILE},
};

pub struct RunServer {
    components: Option<Vec<String>>,
    code_path: PathBuf,
    wallets: PathBuf,
    contracts: PathBuf,
    general_config: PathBuf,
    genesis: PathBuf,
    secrets: PathBuf,
}

pub enum ServerMode {
    Normal,
    Genesis,
}

impl RunServer {
    pub fn new(components: Option<Vec<String>>, chain_config: &ChainConfig) -> Self {
        let wallets = chain_config.configs.join(WALLETS_FILE);
        let general_config = chain_config.configs.join(GENERAL_FILE);
        let genesis = chain_config.configs.join(GENESIS_FILE);
        let contracts = chain_config.configs.join(CONTRACTS_FILE);
        let secrets = chain_config.configs.join(SECRETS_FILE);

        Self {
            components,
            code_path: chain_config.link_to_code.clone(),
            wallets,
            contracts,
            general_config,
            genesis,
            secrets,
        }
    }

    pub fn run(&self, shell: &Shell, server_mode: ServerMode) -> anyhow::Result<()> {
        shell.change_dir(&self.code_path);
        let config_genesis = &self.genesis.to_str().unwrap();
        let config_wallets = &self.wallets.to_str().unwrap();
        let config_general_config = &self.general_config.to_str().unwrap();
        let config_contracts = &self.contracts.to_str().unwrap();
        let secrets = &self.secrets.to_str().unwrap();
        let mut additional_args = vec![];
        if let Some(components) = self.components() {
            additional_args.push(format!("--components={}", components))
        }
        if let ServerMode::Genesis = server_mode {
            additional_args.push("--genesis".to_string());
        }

        let mut cmd = Cmd::new(
            cmd!(
                shell,
                "cargo run --release --bin zksync_server --
                --genesis-path {config_genesis}
                --wallets-path {config_wallets}
                --config-path {config_general_config}
                --secrets-path {secrets}
                --contracts-config-path {config_contracts}
                "
            )
            .args(additional_args)
            .env_remove("RUSTUP_TOOLCHAIN"),
        );

        // If we are running server in normal mode
        // we need to get the output to the console
        if let ServerMode::Normal = server_mode {
            cmd = cmd.with_force_run();
        }

        cmd.run().context("Failed to run server")?;
        Ok(())
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
