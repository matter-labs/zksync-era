use anyhow::Context;
use clap::Parser;
use serde::Deserialize;
use zkstack_cli_common::forge::ForgeScriptArgs;
use zkstack_cli_types::L1Network;
use zksync_basic_types::H160;
use zksync_web3_decl::jsonrpsee::core::Serialize;

use crate::{
    commands::ecosystem::args::init::{EcosystemArgs, EcosystemArgsFinal, EcosystemInitArgsFinal},
    messages::{MSG_BRIDGEHUB, MSG_DEV_ARG_HELP, MSG_ZKSYNC_OS},
};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct RegisterCTMArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long)]
    pub update_submodules: Option<bool>,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
}

impl RegisterCTMArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<RegisterCTMArgsFinal> {
        let RegisterCTMArgs {
            ecosystem,
            forge_args,
            update_submodules,
            dev,
        } = self;

        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, dev).await?;

        Ok(RegisterCTMArgsFinal {
            ecosystem,
            forge_args,
            update_submodules,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterCTMArgsFinal {
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub update_submodules: Option<bool>,
}

impl From<EcosystemInitArgsFinal> for RegisterCTMArgsFinal {
    fn from(args: EcosystemInitArgsFinal) -> Self {
        RegisterCTMArgsFinal {
            ecosystem: args.ecosystem,
            forge_args: args.forge_args,
            update_submodules: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitNewCTMArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long)]
    pub update_submodules: Option<bool>,
    #[clap(long, default_value_t = false)]
    pub skip_contract_compilation_override: bool,
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: Option<bool>,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: Option<String>,
    #[clap(long, help = MSG_ZKSYNC_OS)]
    pub zksync_os: bool,
}

impl InitNewCTMArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<InitNewCTMArgsFinal> {
        let InitNewCTMArgs {
            ecosystem,
            forge_args,
            update_submodules,
            skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test,
            bridgehub,
            zksync_os,
        } = self;

        // Fill ecosystem args
        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, true).await?;

        // Parse bridgehub address
        let bridgehub_address = if let Some(ref addr_str) = bridgehub {
            Some(
                addr_str
                    .parse::<H160>()
                    .with_context(|| format!("Invalid bridgehub address format: {}", addr_str))?,
            )
        } else {
            None
        };

        Ok(InitNewCTMArgsFinal {
            ecosystem,
            forge_args,
            update_submodules,
            skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test: support_l2_legacy_shared_bridge_test
                .unwrap_or(false),
            bridgehub_address,
            zksync_os,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitNewCTMArgsFinal {
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub update_submodules: Option<bool>,
    pub skip_contract_compilation_override: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub bridgehub_address: Option<H160>,
    pub zksync_os: bool,
}

impl From<EcosystemInitArgsFinal> for InitNewCTMArgsFinal {
    fn from(args: EcosystemInitArgsFinal) -> Self {
        InitNewCTMArgsFinal {
            ecosystem: args.ecosystem,
            forge_args: args.forge_args,
            update_submodules: None,
            skip_contract_compilation_override: args.skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test: args.support_l2_legacy_shared_bridge_test,
            bridgehub_address: args.bridgehub_address,
            zksync_os: args.zksync_os,
        }
    }
}
