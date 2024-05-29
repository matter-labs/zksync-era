use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt, PromptConfirm};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::commands::chain::args::genesis::GenesisArgs;
use crate::defaults::LOCAL_RPC_URL;
use crate::types::L1Network;

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemArgs {
    /// Deploy ecosystem contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_ecosystem: Option<bool>,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = "L1 RPC URL")]
    pub l1_rpc_url: Option<String>,
}

impl EcosystemArgs {
    pub fn fill_values_with_prompt(self, l1_network: L1Network) -> EcosystemArgsFinal {
        let deploy_ecosystem = self.deploy_ecosystem.unwrap_or_else(|| {
            PromptConfirm::new("Do you want to deploy ecosystem contracts? (Not needed if you already have an existing one)")
                .default(true)
                .ask()
        });

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new("What is the RPC URL of the L1 network?");
            if l1_network == L1Network::Localhost {
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
        EcosystemArgsFinal {
            deploy_ecosystem,
            ecosystem_contracts_path: self.ecosystem_contracts_path,
            l1_rpc_url,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemArgsFinal {
    pub deploy_ecosystem: bool,
    pub ecosystem_contracts_path: Option<PathBuf>,
    pub l1_rpc_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemInitArgs {
    /// Deploy Paymaster contract
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
    /// Deploy ERC20 contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_erc20: Option<bool>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(flatten, next_help_heading = "Genesis options")]
    #[serde(flatten)]
    pub genesis_args: GenesisArgs,
}

impl EcosystemInitArgs {
    pub fn fill_values_with_prompt(self, l1_network: L1Network) -> EcosystemInitArgsFinal {
        let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
            PromptConfirm::new("Do you want to deploy paymaster?")
                .default(true)
                .ask()
        });
        let deploy_erc20 = self.deploy_erc20.unwrap_or_else(|| {
            PromptConfirm::new("Do you want to deploy some test ERC20s?")
                .default(true)
                .ask()
        });
        let ecosystem = self.ecosystem.fill_values_with_prompt(l1_network);

        EcosystemInitArgsFinal {
            deploy_paymaster,
            deploy_erc20,
            ecosystem,
            forge_args: self.forge_args.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemInitArgsFinal {
    pub deploy_paymaster: bool,
    pub deploy_erc20: bool,
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
}
