use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::Prompt;
use zkstack_cli_config::ChainConfig;
use zkstack_cli_types::L1Network;

use crate::{
    commands::chain::args::{
        genesis::{GenesisArgs, GenesisArgsFinal},
        init::{da_configs::ValidiumType, InitArgsFinal},
    },
    defaults::LOCAL_RPC_URL,
    messages::{
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
}

#[derive(Debug, Clone)]
pub struct InitConfigsArgsFinal {
    pub genesis_args: Option<GenesisArgsFinal>,
    pub l1_rpc_url: String,
    pub no_port_reallocation: bool,
    pub validium_config: Option<ValidiumType>,
}

impl InitConfigsArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> InitConfigsArgsFinal {
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            if config.l1_network == L1Network::Localhost {
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

        InitConfigsArgsFinal {
            genesis_args: Some(self.genesis_args.fill_values_with_prompt(config)),
            l1_rpc_url,
            no_port_reallocation: self.no_port_reallocation,
            validium_config: Some(ValidiumType::read()),
        }
    }
}

impl InitConfigsArgsFinal {
    pub fn from_chain_init_args(init_args: &InitArgsFinal) -> InitConfigsArgsFinal {
        InitConfigsArgsFinal {
            genesis_args: init_args.genesis_args.clone(),
            l1_rpc_url: init_args.l1_rpc_url.clone(),
            no_port_reallocation: init_args.no_port_reallocation,
            validium_config: init_args.validium_config.clone(),
        }
    }
}
