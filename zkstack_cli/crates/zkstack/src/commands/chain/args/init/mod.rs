use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt};
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use types::L1Network;
use url::Url;

use crate::{
    commands::chain::args::genesis::{GenesisArgs, GenesisArgsFinal},
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_DEPLOY_PAYMASTER_PROMPT, MSG_DEV_ARG_HELP, MSG_L1_RPC_URL_HELP,
        MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT, MSG_NO_PORT_REALLOCATION_HELP,
        MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP,
    },
};

pub mod configs;

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, short, action)]
    pub dont_drop: bool,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
    #[clap(long, help = MSG_NO_PORT_REALLOCATION_HELP)]
    pub no_port_reallocation: bool,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
}

impl InitArgs {
    pub fn get_genesis_args(&self) -> GenesisArgs {
        GenesisArgs {
            server_db_url: self.server_db_url.clone(),
            server_db_name: self.server_db_name.clone(),
            dev: self.dev,
            dont_drop: self.dont_drop,
        }
    }

    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> InitArgsFinal {
        let genesis = self.get_genesis_args();

        let deploy_paymaster = if self.dev {
            true
        } else {
            self.deploy_paymaster.unwrap_or_else(|| {
                common::PromptConfirm::new(MSG_DEPLOY_PAYMASTER_PROMPT)
                    .default(true)
                    .ask()
            })
        };

        let l1_rpc_url = if self.dev {
            LOCAL_RPC_URL.to_string()
        } else {
            self.l1_rpc_url.unwrap_or_else(|| {
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
            })
        };

        InitArgsFinal {
            forge_args: self.forge_args,
            genesis_args: genesis.fill_values_with_prompt(config),
            deploy_paymaster,
            l1_rpc_url,
            no_port_reallocation: self.no_port_reallocation,
            dev: self.dev,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub genesis_args: GenesisArgsFinal,
    pub deploy_paymaster: bool,
    pub l1_rpc_url: String,
    pub no_port_reallocation: bool,
    pub dev: bool,
}
