use std::path::PathBuf;

use clap::Parser;
use serde::{Deserialize, Serialize};
use url::Url;
use zkstack_cli_common::{forge::ForgeScriptArgs, PromptConfirm};
use zkstack_cli_types::{L1Network, VMOption};

use crate::{
    commands::{
        chain::args::{genesis::GenesisArgs, init::da_configs::ValidiumTypeArgs},
        ecosystem::args::common::CommonEcosystemArgs,
    },
    messages::{
        MSG_BRIDGEHUB, MSG_DEPLOY_ECOSYSTEM_PROMPT, MSG_DEPLOY_ERC20_PROMPT, MSG_DEV_ARG_HELP,
        MSG_NO_PORT_REALLOCATION_HELP, MSG_OBSERVABILITY_HELP, MSG_OBSERVABILITY_PROMPT,
        MSG_SERVER_COMMAND_HELP, MSG_SERVER_DB_NAME_HELP, MSG_SERVER_DB_URL_HELP,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct EcosystemInitArgs {
    #[command(flatten)]
    pub common: CommonEcosystemArgs,

    /// Deploy ecosystem contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_ecosystem: Option<bool>,
    /// Deploy ERC20 contracts
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_erc20: Option<bool>,
    #[clap(long)]
    pub ecosystem_contracts_path: Option<PathBuf>,
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
    #[clap(flatten)]
    pub validium_args: ValidiumTypeArgs,
    #[clap(long,default_value_t = false,  default_missing_value = "true", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: bool,
    #[clap(long, default_value_t =false,default_missing_value = "true",  num_args = 0..=1)]
    pub make_permanent_rollup: bool,
    #[clap(long, help = MSG_SERVER_COMMAND_HELP)]
    pub server_command: Option<String>,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub no_genesis: bool,
}

impl EcosystemInitArgs {
    pub fn get_genesis_args(&self) -> Option<GenesisArgs> {
        if self.no_genesis || self.common.vm_option().is_zksync_os() {
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
            forge_args,
            dev,
            ecosystem_only,
            observability,
            no_port_reallocation,
            validium_args,
            support_l2_legacy_shared_bridge_test,
            make_permanent_rollup,
            deploy_paymaster,
            ecosystem_contracts_path,
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
        let common = self.common.fill_values_with_prompt(l1_network, dev).await?;
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
            forge_args,
            dev,
            ecosystem_only,
            no_port_reallocation,
            validium_args,
            support_l2_legacy_shared_bridge_test,
            deploy_ecosystem,
            deploy_paymaster,
            make_permanent_rollup,
            genesis_args,
            vm_option: common.vm_option,
            ecosystem_contracts_path,
            l1_rpc_url: common.l1_rpc_url,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EcosystemInitArgsFinal {
    pub deploy_erc20: bool,
    pub ecosystem_contracts_path: Option<PathBuf>,
    pub l1_rpc_url: String,
    pub forge_args: ForgeScriptArgs,
    pub dev: bool,
    pub observability: bool,
    pub ecosystem_only: bool,
    pub no_port_reallocation: bool,
    pub validium_args: ValidiumTypeArgs,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub deploy_ecosystem: bool,
    pub deploy_paymaster: Option<bool>,
    pub make_permanent_rollup: bool,
    pub genesis_args: Option<GenesisArgs>,
    pub vm_option: VMOption,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitCoreContractsArgs {
    #[clap(flatten)]
    pub common: CommonEcosystemArgs,
    #[clap(long, default_missing_value = "true", num_args = 0..=1)]
    pub deploy_erc20: Option<bool>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(long, default_value_t = true, default_missing_value = "true", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: bool,
}

impl InitCoreContractsArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<InitCoreContractsArgsFinal> {
        let InitCoreContractsArgs {
            common,
            deploy_erc20,
            forge_args,
            dev,
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

        let common = common.fill_values_with_prompt(l1_network, dev).await?;

        Ok(InitCoreContractsArgsFinal {
            vm_option: common.vm_option,
            deploy_erc20,
            l1_rpc_url: common.l1_rpc_url,
            forge_args,
            support_l2_legacy_shared_bridge_test,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitCoreContractsArgsFinal {
    pub vm_option: VMOption,
    pub deploy_erc20: bool,
    pub forge_args: ForgeScriptArgs,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub l1_rpc_url: String,
}

impl From<EcosystemInitArgsFinal> for InitCoreContractsArgsFinal {
    fn from(args: EcosystemInitArgsFinal) -> Self {
        InitCoreContractsArgsFinal {
            l1_rpc_url: args.l1_rpc_url,
            vm_option: args.vm_option,
            deploy_erc20: args.deploy_erc20,
            forge_args: args.forge_args,
            support_l2_legacy_shared_bridge_test: args.support_l2_legacy_shared_bridge_test,
        }
    }
}
