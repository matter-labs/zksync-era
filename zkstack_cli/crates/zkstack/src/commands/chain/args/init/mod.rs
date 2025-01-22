use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{forge::ForgeScriptArgs, Prompt};
use zkstack_cli_config::ChainConfig;
use zkstack_cli_types::{L1BatchCommitmentMode, L1Network};

use crate::{
    commands::chain::args::{
        genesis::{GenesisArgs, GenesisArgsFinal},
        init::da_configs::ValidiumType,
    },
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_DEPLOY_PAYMASTER_PROMPT, MSG_DEV_ARG_HELP, MSG_L1_RPC_URL_HELP,
        MSG_L1_RPC_URL_INVALID_ERR, MSG_L1_RPC_URL_PROMPT, MSG_NO_PORT_REALLOCATION_HELP,
        MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP,
    },
};

pub mod configs;
pub(crate) mod da_configs;

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
    #[clap(long)]
    pub update_submodules: Option<bool>,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(flatten)]
    pub validium_args: da_configs::ValidiumTypeArgs,
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
                zkstack_cli_common::PromptConfirm::new(MSG_DEPLOY_PAYMASTER_PROMPT)
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

        let validium_config = match config.l1_batch_commit_data_generator_mode {
            L1BatchCommitmentMode::Validium => match self.validium_args.validium_type {
                None => Some(ValidiumType::read()),
                Some(da_configs::ValidiumTypeInternal::NoDA) => Some(ValidiumType::NoDA),
                Some(da_configs::ValidiumTypeInternal::Avail) => panic!(
                    "Avail is not supported via CLI args, use interactive mode" // TODO: Add support for configuration via CLI args
                ),
            },
            _ => None,
        };

        InitArgsFinal {
            forge_args: self.forge_args,
            genesis_args: genesis.fill_values_with_prompt(config),
            deploy_paymaster,
            l1_rpc_url,
            no_port_reallocation: self.no_port_reallocation,
            validium_config,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InitArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub genesis_args: GenesisArgsFinal,
    pub deploy_paymaster: bool,
    pub l1_rpc_url: String,
    pub no_port_reallocation: bool,
    pub validium_config: Option<ValidiumType>,
}
