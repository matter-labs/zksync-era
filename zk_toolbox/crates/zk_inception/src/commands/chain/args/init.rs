use clap::Parser;
use common::forge::ForgeScriptArgs;
use common::Prompt;
use config::ChainConfig;
use serde::{Deserialize, Serialize};
use types::L1Network;
use url::Url;

use super::genesis::GenesisArgsFinal;
use crate::commands::chain::args::genesis::GenesisArgs;
use crate::defaults::LOCAL_RPC_URL;

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(flatten, next_help_heading = "Genesis options")]
    #[serde(flatten)]
    pub genesis_args: GenesisArgs,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
    #[clap(long, help = "L1 RPC URL")]
    pub l1_rpc_url: Option<String>,
}

impl InitArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> InitArgsFinal {
        let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
            common::PromptConfirm::new("Do you want to deploy a test paymaster?")
                .default(true)
                .ask()
        });

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new("What is the RPC URL of the L1 network?");
            if config.l1_network == L1Network::Localhost {
                prompt = prompt.default(LOCAL_RPC_URL);
            }
            prompt
                .validate_with(|val: &String| -> Result<(), String> {
                    Url::parse(val)
                        .map(|_| ())
                        .map_err(|_| "Invalid RPC url".to_string())
                })
                .ask()
        });

        InitArgsFinal {
            forge_args: self.forge_args,
            genesis_args: self.genesis_args.fill_values_with_prompt(config),
            deploy_paymaster,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub genesis_args: GenesisArgsFinal,
    pub deploy_paymaster: bool,
    pub l1_rpc_url: String,
}
