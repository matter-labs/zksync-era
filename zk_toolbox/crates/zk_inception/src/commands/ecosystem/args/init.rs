use std::path::PathBuf;

use clap::Parser;
use common::{forge::ForgeScriptArgs, Prompt, PromptConfirm};
use serde::{Deserialize, Serialize};
use types::L1Network;
use url::Url;

use crate::{
    commands::chain::args::genesis::GenesisArgs,
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_DEPLOY_ECOSYSTEM_PROMPT, MSG_DEPLOY_ERC20_PROMPT, MSG_DEPLOY_PAYMASTER_PROMPT,
        MSG_DEV_ARG_HELP, MSG_GENESIS_ARGS_HELP, MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR,
        MSG_L1_RPC_URL_PROMPT, MSG_OBSERVABILITY_HELP, MSG_OBSERVABILITY_PROMPT,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemArgs {
    /// Deploy ecosystem contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_ecosystem: Option<bool>,
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
}

impl EcosystemArgs {
    pub fn fill_values_with_prompt(self, l1_network: L1Network, dev: bool) -> EcosystemArgsFinal {
        let deploy_ecosystem = self.deploy_ecosystem.unwrap_or_else(|| {
            if dev {
                true
            } else {
                PromptConfirm::new(MSG_DEPLOY_ECOSYSTEM_PROMPT)
                    .default(true)
                    .ask()
            }
        });

        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_L1_RPC_URL_PROMPT);
            if dev {
                return LOCAL_RPC_URL.to_string();
            }
            if l1_network == L1Network::Localhost {
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
    #[clap(flatten, next_help_heading = MSG_GENESIS_ARGS_HELP)]
    #[serde(flatten)]
    pub genesis_args: GenesisArgs,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(long, short = 'o', help = MSG_OBSERVABILITY_HELP, default_missing_value = "true", num_args = 0..=1)]
    pub observability: Option<bool>,
}

impl EcosystemInitArgs {
    pub fn fill_values_with_prompt(self, l1_network: L1Network) -> EcosystemInitArgsFinal {
        let (deploy_paymaster, deploy_erc20) = if self.dev {
            (true, true)
        } else {
            let deploy_paymaster = self.deploy_paymaster.unwrap_or_else(|| {
                PromptConfirm::new(MSG_DEPLOY_PAYMASTER_PROMPT)
                    .default(true)
                    .ask()
            });
            let deploy_erc20 = self.deploy_erc20.unwrap_or_else(|| {
                PromptConfirm::new(MSG_DEPLOY_ERC20_PROMPT)
                    .default(true)
                    .ask()
            });
            (deploy_paymaster, deploy_erc20)
        };
        let ecosystem = self.ecosystem.fill_values_with_prompt(l1_network, self.dev);
        let observability = if self.dev {
            true
        } else {
            self.observability.unwrap_or_else(|| {
                PromptConfirm::new(MSG_OBSERVABILITY_PROMPT)
                    .default(true)
                    .ask()
            })
        };

        EcosystemInitArgsFinal {
            deploy_paymaster,
            deploy_erc20,
            ecosystem,
            forge_args: self.forge_args.clone(),
            dev: self.dev,
            observability,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EcosystemInitArgsFinal {
    pub deploy_paymaster: bool,
    pub deploy_erc20: bool,
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub dev: bool,
    pub observability: bool,
}
