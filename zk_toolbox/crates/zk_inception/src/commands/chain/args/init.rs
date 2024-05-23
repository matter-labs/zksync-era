use clap::Parser;
use common::forge::ForgeScriptArgs;
use serde::{Deserialize, Serialize};

use super::genesis::GenesisArgsFinal;
use crate::{commands::chain::args::genesis::GenesisArgs, configs::ChainConfig};

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
}

impl InitArgs {
    pub fn fill_values_with_prompt(self, config: &ChainConfig) -> InitArgsFinal {
        let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
            common::PromptConfirm::new("Do you want to deploy a test paymaster?")
                .default(true)
                .ask()
        });

        InitArgsFinal {
            forge_args: self.forge_args,
            genesis_args: self.genesis_args.fill_values_with_prompt(config),
            deploy_paymaster,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InitArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub genesis_args: GenesisArgsFinal,
    pub deploy_paymaster: bool,
}
