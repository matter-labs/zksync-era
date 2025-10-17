use std::path::PathBuf;

use clap::Parser;
use serde::Deserialize;
use zkstack_cli_common::forge::ForgeArgs;
use zkstack_cli_types::{L1Network, VMOption};
use zksync_basic_types::Address;
use zksync_web3_decl::jsonrpsee::core::Serialize;

use crate::{commands::ecosystem::args::common::CommonEcosystemArgs, messages::MSG_BRIDGEHUB};

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct InitNewCTMArgs {
    #[clap(flatten)]
    pub common: CommonEcosystemArgs,
    #[clap(long, default_value_t=false, default_missing_value = "true", num_args = 0..=1)]
    pub support_l2_legacy_shared_bridge_test: bool,
    #[clap(long, help = MSG_BRIDGEHUB)]
    pub bridgehub: Option<Address>,
    #[clap(long, default_value_t = true)]
    pub reuse_gov_and_admin: bool,

    #[arg(long, requires = "default_configs_src_path")]
    pub contracts_src_path: Option<PathBuf>,
    #[arg(long, requires = "contracts_src_path")]
    pub default_configs_src_path: Option<PathBuf>,
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeArgs,
}

impl InitNewCTMArgs {
    pub async fn fill_values_with_prompt(
        self,
        l1_network: L1Network,
    ) -> anyhow::Result<InitNewCTMArgsFinal> {
        let InitNewCTMArgs {
            common,
            forge_args,
            support_l2_legacy_shared_bridge_test,
            bridgehub,
            reuse_gov_and_admin,
            contracts_src_path,
            default_configs_src_path,
        } = self;

        // Fill ecosystem args
        let common = common.fill_values_with_prompt(l1_network, true).await?;

        Ok(InitNewCTMArgsFinal {
            l1_rpc_url: common.l1_rpc_url,
            forge_args,
            support_l2_legacy_shared_bridge_test,
            bridgehub_address: bridgehub,
            vm_option: common.vm_option,
            reuse_gov_and_admin,
            contracts_src_path,
            default_configs_src_path,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InitNewCTMArgsFinal {
    pub forge_args: ForgeArgs,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub bridgehub_address: Option<Address>,
    pub vm_option: VMOption,
    pub reuse_gov_and_admin: bool,
    pub contracts_src_path: Option<PathBuf>,
    pub default_configs_src_path: Option<PathBuf>,
    pub l1_rpc_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Parser)]
pub struct SetNewCTMArgs {
    #[clap(long, help = "Path to contracts sources")]
    pub contracts_src_path: Option<PathBuf>,
    #[clap(long, help = "Path to default configs sources")]
    pub default_configs_src_path: Option<PathBuf>,
    #[clap(
        long,
        help = "Whether to apply it to zksync os",
        default_value_t = false,
        default_missing_value = "true"
    )]
    pub zksync_os: bool,
}

impl SetNewCTMArgs {
    pub fn fill_values_with_prompt(self) -> anyhow::Result<SetNewCTMArgsFinal> {
        let contracts_src_path = self.contracts_src_path.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Provide path to contracts sources").ask()
        });
        let default_configs_src_path = self.default_configs_src_path.unwrap_or_else(|| {
            zkstack_cli_common::Prompt::new("Provide path to default configs sources").ask()
        });
        let vm_option = if self.zksync_os {
            VMOption::ZKSyncOsVM
        } else {
            VMOption::EraVM
        };

        Ok(SetNewCTMArgsFinal {
            contracts_src_path,
            default_configs_src_path,
            vm_option,
        })
    }
}

pub struct SetNewCTMArgsFinal {
    pub contracts_src_path: PathBuf,
    pub default_configs_src_path: PathBuf,
    pub vm_option: VMOption,
}
