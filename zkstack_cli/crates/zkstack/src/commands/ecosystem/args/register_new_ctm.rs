use clap::Parser;
use serde::Deserialize;
use zkstack_cli_common::forge::ForgeScriptArgs;
use zkstack_cli_types::L1Network;
use zksync_types::Address;

use crate::{
    commands::ecosystem::args::common::CommonEcosystemArgs,
    messages::{MSG_BRIDGEHUB, MSG_CTM, MSG_DEV_ARG_HELP},
};

#[derive(Debug, Clone, zksync_web3_decl::jsonrpsee::core::Serialize, Deserialize, Parser)]
pub struct RegisterCTMArgs {
    #[command(flatten)]
    pub common: CommonEcosystemArgs,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,
    #[clap(long, help = MSG_DEV_ARG_HELP)]
    pub dev: bool,
    #[clap(long, default_missing_value = "false", num_args = 0..=1)]
    pub only_save_calldata: bool,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: Option<Address>,
    #[clap(long, help = MSG_CTM)]
    pub ctm: Option<Address>,
}

impl RegisterCTMArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<RegisterCTMArgsFinal> {
        let RegisterCTMArgs {
            common,
            forge_args,
            dev,
            only_save_calldata,
            bridgehub,
            ctm,
        } = self;

        let common = common.fill_values_with_prompt(l1_network, dev).await?;

        Ok(RegisterCTMArgsFinal {
            l1_rpc_url: common.l1_rpc_url,
            forge_args,
            zksync_os: common.zksync_os,
            only_save_calldata,
            bridgehub,
            ctm,
        })
    }
}

#[derive(Debug, zksync_web3_decl::jsonrpsee::core::Serialize, Deserialize)]
pub struct RegisterCTMArgsFinal {
    pub forge_args: ForgeScriptArgs,
    pub only_save_calldata: bool,
    pub bridgehub: Option<Address>,
    pub ctm: Option<Address>,
    pub zksync_os: bool,
    pub l1_rpc_url: String,
}
