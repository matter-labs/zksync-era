use anyhow::Context;
use clap::Parser;
use serde::Deserialize;
use zkstack_cli_common::config::global_config;
use zkstack_cli_common::forge::ForgeScriptArgs;
use zkstack_cli_types::L1Network;
use zksync_basic_types::H160;
use zksync_web3_decl::jsonrpsee::core::Serialize;

use crate::{
    commands::ecosystem::args::init::{EcosystemArgs, EcosystemArgsFinal},
    messages::{MSG_BRIDGEHUB, MSG_CTM, MSG_DEV_ARG_HELP, MSG_ZKSYNC_OS},
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
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub only_save_calldata: bool,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: Option<String>,
    #[clap(long, help = MSG_CTM)]
    pub ctm: String,
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
            only_save_calldata,
            bridgehub,
            ctm,
        } = self;

        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, dev).await?;

        // Parse bridgehub address
        let bridgehub_address = bridgehub
            .map(|a| {
                a.parse::<H160>()
                    .with_context(|| format!("Invalid bridgehub address format: {}", a))
            })
            .transpose()?;
        // Parse ctm address
        let ctm_address = ctm
            .parse::<H160>()
            .with_context(|| format!("Invalid ctm address format: {}", ctm))?;

        Ok(RegisterCTMArgsFinal {
            ecosystem,
            forge_args,
            update_submodules,
            only_save_calldata,
            bridgehub_address,
            ctm_address,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegisterCTMArgsFinal {
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    pub update_submodules: Option<bool>,
    pub only_save_calldata: bool,
    pub bridgehub_address: Option<H160>,
    pub ctm_address: H160,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitNewCTMArgs {
    #[clap(flatten)]
    #[serde(flatten)]
    pub ecosystem: EcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    // #[clap(long)]
    // pub update_submodules: Option<bool>,
    // #[clap(long, default_value_t = false)]
    // pub skip_contract_compilation_override: bool,
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: Option<bool>,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: Option<String>,
    #[clap(long, default_missing_value = "true")]
    pub reuse_gov_and_admin: bool,
}

impl InitNewCTMArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<InitNewCTMArgsFinal> {
        let InitNewCTMArgs {
            ecosystem,
            forge_args,
            // update_submodules,
            // skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test,
            // update_submodules,
            bridgehub,
            // zksync_os,
            reuse_gov_and_admin,
        } = self;

        // Fill ecosystem args
        let ecosystem = ecosystem.fill_values_with_prompt(l1_network, true).await?;

        Ok(InitNewCTMArgsFinal {
            ecosystem,
            forge_args,
            // update_submodules,
            // skip_contract_compilation_override,
            support_l2_legacy_shared_bridge_test: support_l2_legacy_shared_bridge_test
                .unwrap_or(false),
            bridgehub_address: bridgehub
                .map(|a| {
                    a.parse::<H160>()
                        .with_context(|| format!("Invalid bridgehub address format: {}", a))
                })
                .transpose()?,
            zksync_os: global_config().zksync_os,
            reuse_gov_and_admin,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitNewCTMArgsFinal {
    pub ecosystem: EcosystemArgsFinal,
    pub forge_args: ForgeScriptArgs,
    // pub update_submodules: Option<bool>,
    // pub skip_contract_compilation_override: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub bridgehub_address: Option<H160>,
    pub zksync_os: bool,
    pub reuse_gov_and_admin: bool,
}
