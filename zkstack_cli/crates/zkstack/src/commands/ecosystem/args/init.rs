use std::path::PathBuf;

use clap::Parser;
use ethers::providers::Middleware;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{
    config::global_config, ethereum::get_ethers_provider, forge::ForgeScriptArgs, Prompt,
    PromptConfirm,
};
use zkstack_cli_types::L1Network;

use crate::{
    commands::chain::args::{genesis::GenesisArgs, init::da_configs::ValidiumTypeArgs},
    defaults::LOCAL_RPC_URL,
    messages::{
        MSG_BRIDGEHUB, MSG_DEPLOY_ECOSYSTEM_PROMPT, MSG_DEPLOY_ERC20_PROMPT, MSG_DEV_ARG_HELP,
        MSG_L1_RPC_URL_HELP, MSG_L1_RPC_URL_INVALID_ERR, MSG_NO_PORT_REALLOCATION_HELP,
        MSG_OBSERVABILITY_HELP, MSG_OBSERVABILITY_PROMPT, MSG_RPC_URL_PROMPT,
        MSG_SERVER_COMMAND_HELP, MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP,
    },
};

/// Check if L1 RPC is healthy by calling eth_chainId
async fn check_l1_rpc_health(l1_rpc_url: &str) -> anyhow::Result<()> {
    let l1_provider = get_ethers_provider(l1_rpc_url)?;
    let l1_chain_id = l1_provider.get_chainid().await?.as_u64();

    println!("✅ L1 RPC health check passed - chain ID: {}", l1_chain_id);
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemArgs {
    /// Path to ecosystem contracts
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
    #[clap(long, help = MSG_L1_RPC_URL_HELP)]
    pub l1_rpc_url: Option<String>,
}

impl EcosystemArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
        dev: bool,
    ) -> anyhow::Result<EcosystemArgsFinal> {
        let l1_rpc_url = self.l1_rpc_url.unwrap_or_else(|| {
            let mut prompt = Prompt::new(MSG_RPC_URL_PROMPT);
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

        // Check L1 RPC health after getting the URL
        println!("🔍 Checking L1 RPC health...");
        check_l1_rpc_health(&l1_rpc_url).await?;

        Ok(EcosystemArgsFinal {
            ecosystem_contracts_path: self.ecosystem_contracts_path,
            l1_rpc_url,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemArgsFinal {
    pub ecosystem_contracts_path: Option<PathBuf>,
    pub l1_rpc_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemInitArgs {
    /// Deploy ecosystem contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_ecosystem: Option<bool>,
    /// Deploy ERC20 contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_erc20: Option<bool>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    /// Deploy Paymaster contract
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_paymaster: Option<bool>,
    #[clap(long, help = MSG_SERVER_DB_URL_HELP)]
    pub server_db_url: Option<Url>,
    #[clap(long, help = MSG_SERVER_DB_NAME_HELP)]
    pub server_db_name: Option<String>,
    #[clap(long, short, action)]
    pub dont_drop: bool,
    /// Initialize ecosystem only and skip chain initialization (chain can be initialized later with `chain init` subcommand)
    #[clap(long, default_value_t = false)]
    pub ecosystem_only: bool,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(
        long, short = 'o', help = MSG_OBSERVABILITY_HELP, default_missing_value = "true", num_args = 0..=1
    )]
    pub observability: Option<bool>,
    #[clap(long, help = MSG_NO_PORT_REALLOCATION_HELP)]
    pub no_port_reallocation: bool,
    #[clap(long)]
    pub update_submodules: Option<bool>,
    #[clap(flatten)]
    pub validium_args: ValidiumTypeArgs,
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: Option<bool>,
    #[clap(long, default_value = "false", num_args = 0..=1)]
    pub make_permanent_rollup: Option<bool>,
    #[clap(long, default_value_t = false)]
    pub skip_contract_compilation_override: bool,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub no_genesis: bool,
}

impl EcosystemInitArgs {
    pub fn get_genesis_args(&self) -> Option<GenesisArgs> {
        if self.no_genesis || global_config().zksync_os {
            None
        } else {
            Some(GenesisArgs {
                server_db_url: self.server_db_url.clone(),
                server_db_name: self.server_db_name.clone(),
                dev: self.dev,
                dont_drop: self.dont_drop,
                server_command: self.server_command.clone(),
            })
        }
    }

    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<EcosystemInitArgsFinal> {
        let genesis_args = self.get_genesis_args();
        let EcosystemInitArgs {
            deploy_ecosystem,
            deploy_erc20,
            ecosystem,
            forge_args,
            dev,
            ecosystem_only,
            observability,
            no_port_reallocation,
            skip_contract_compilation_override,
            validium_args,
            support_l2_legacy_shared_bridge_test,
            make_permanent_rollup,
            update_submodules,
            deploy_paymaster,
            ..
        } = self;

        let deploy_erc20 = if dev {
            true
        } else {
            deploy_erc20.unwrap_or_else(|| {
                PromptConfirm::new(MSG_DEPLOY_ERC20_PROMPT)
                    .default(true)
                    .ask()
            })
        };
        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, dev).await?;
        let observability = if dev {
            true
        } else {
            observability.unwrap_or_else(|| {
                PromptConfirm::new(MSG_OBSERVABILITY_PROMPT)
                    .default(true)
                    .ask()
            })
        };

        let deploy_ecosystem = deploy_ecosystem.unwrap_or_else(|| {
            if dev {
                true
            } else {
                PromptConfirm::new(MSG_DEPLOY_ECOSYSTEM_PROMPT)
                    .default(true)
                    .ask()
            }
        });

        Ok(EcosystemInitArgsFinal {
            deploy_erc20,
            observability,
            ecosystem,
            forge_args,
            dev,
            ecosystem_only,
            no_port_reallocation,
            skip_contract_compilation_override,
            validium_args,
            support_l2_legacy_shared_bridge_test: support_l2_legacy_shared_bridge_test
                .unwrap_or_default(),
            deploy_ecosystem,
            deploy_paymaster,
            make_permanent_rollup,
            update_submodules,
            genesis_args,
            zksync_os: global_config().zksync_os,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemInitArgsFinal {
    pub deploy_erc20: bool,
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub dev: bool,
    pub observability: bool,
    pub ecosystem_only: bool,
    pub no_port_reallocation: bool,
    pub skip_contract_compilation_override: bool,
    pub validium_args: ValidiumTypeArgs,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub deploy_ecosystem: bool,
    pub deploy_paymaster: Option<bool>,
    pub make_permanent_rollup: Option<bool>,
    pub update_submodules: Option<bool>,
    pub genesis_args: Option<GenesisArgs>,
    pub zksync_os: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitCoreContractsArgs {
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_erc20: Option<bool>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(long)]
    pub update_submodules: Option<bool>,
    #[clap(long, default_value_t = false)]
    pub skip_contract_compilation_override: bool,
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: Option<bool>,
}

impl InitCoreContractsArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<InitCoreContractsArgsFinal> {
        let InitCoreContractsArgs {
            deploy_erc20,
            ecosystem,
            forge_args,
            dev,
            update_submodules,
            skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test,
        } = self;

        let deploy_erc20 = if self.dev {
            true
        } else {
            deploy_erc20.unwrap_or_else(|| {
                PromptConfirm::new(MSG_DEPLOY_ERC20_PROMPT)
                    .default(true)
                    .ask()
            })
        };

        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, dev).await?;

        Ok(InitCoreContractsArgsFinal {
            deploy_erc20,
            ecosystem,
            forge_args,
            update_submodules,
            skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test: support_l2_legacy_shared_bridge_test
                .unwrap_or(false),
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitCoreContractsArgsFinal {
    pub deploy_erc20: bool,
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub update_submodules: Option<bool>,
    pub skip_contract_compilation_override: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
}

impl From<EcosystemInitArgsFinal> for InitCoreContractsArgsFinal {
    fn from(args: EcosystemInitArgsFinal) -> Self {
        InitCoreContractsArgsFinal {
            deploy_erc20: args.deploy_erc20,
            ecosystem: args.ecosystem,
            forge_args: args.forge_args,
            update_submodules: None,
            skip_contract_compilation_override: args.skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test: args.support_l2_legacy_shared_bridge_test,
        }
    }
}
