use std::path::PathBuf;

use clap::Parser;
use common::Prompt;
use config::{ChainConfig, EcosystemConfig};
use serde::{Deserialize, Serialize};
use types::L1Network;
use url::Url;

use crate::{
    commands::{
        chain::args::{
            genesis::{GenesisArgs, GenesisArgsFinal},
            init::InitArgsFinal,
        },
        ecosystem::init::prompt_ecosystem_contracts_path,
    },
    defaults::LOCAL_RPC_URL,
    messages::{
        msg_ecosystem_no_found_preexisting_contract, MSG_ECOSYSTEM_CONTRACTS_PATH_HELP,
        MSG_GENESIS_ARGS_HELP, MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR,
        MSG_L1_RPC_URL_PROMPT, MSG_NO_PORT_REALLOCATION_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitConfigsArgs {
    #[clap(flatten, next_help_heading = MSG_GENESIS_ARGS_HELP)]
    #[serde(flatten)]
    pub genesis_args: GenesisArgs,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long, help = MSG_NO_PORT_REALLOCATION_HELP)]
    pub no_port_reallocation: bool,
    #[clap(long, help = MSG_ECOSYSTEM_CONTRACTS_PATH_HELP)]
    pub ecosystem_contracts_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitConfigsArgsFinal {
    pub genesis_args: GenesisArgsFinal,
    pub l1_rpc_url: String,
    pub no_port_reallocation: bool,
    pub ecosystem_contracts_path: PathBuf,
}

impl InitConfigsArgs {
    pub fn fill_values_with_prompt(
        self,
        ecosystem: Option<EcosystemConfig>,
        chain: &ChainConfig,
    ) -> anyhow::Result<InitConfigsArgsFinal> {
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            if chain.l1_network == L1Network::Localhost {
                prompt = prompt.default(LOCAL_RPC_URL);
            }
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| MSG_L1_RPC_URL_INVALID_ERR.to_string())
                })
                .ask()
        });

        let ecosystem_contracts_path =
            get_ecosystem_contracts_path(self.ecosystem_contracts_path, ecosystem, chain)?;

        Ok(InitConfigsArgsFinal {
            genesis_args: self.genesis_args.fill_values_with_prompt(chain),
            l1_rpc_url,
            no_port_reallocation: self.no_port_reallocation,
            ecosystem_contracts_path,
        })
    }
}

pub fn get_ecosystem_contracts_path(
    ecosystem_contracts_path: Option<String>,
    ecosystem: Option<EcosystemConfig>,
    chain: &ChainConfig,
) -> anyhow::Result<PathBuf> {
    let ecosystem_contracts_path = ecosystem_contracts_path.map_or_else(
        || prompt_ecosystem_contracts_path(),
        |path| Some(PathBuf::from(path)),
    );

    let ecosystem_preexisting_configs_path = ecosystem.map_or_else(
        || chain.get_preexisting_ecosystem_contracts_path(),
        |e| e.get_preexisting_ecosystem_contracts_path(),
    );

    if ecosystem_contracts_path.is_none() && !ecosystem_preexisting_configs_path.exists() {
        anyhow::bail!(msg_ecosystem_no_found_preexisting_contract(
            &chain.l1_network.to_string()
        ))
    }

    Ok(ecosystem_contracts_path.unwrap_or_else(|| ecosystem_preexisting_configs_path))
}

impl InitConfigsArgsFinal {
    pub fn from_chain_init_args(init_args: &InitArgsFinal) -> InitConfigsArgsFinal {
        InitConfigsArgsFinal {
            genesis_args: init_args.genesis_args.clone(),
            l1_rpc_url: init_args.l1_rpc_url.clone(),
            no_port_reallocation: init_args.no_port_reallocation,
            ecosystem_contracts_path: init_args.ecosystem_contracts_path.clone(),
        }
    }
}
