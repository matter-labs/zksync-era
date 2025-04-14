use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::Parser;
use ethers::{abi::parse_abi, contract::BaseContract, types::Bytes, utils::hex};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    wallets::Wallet,
};
use zkstack_cli_config::{
    forge_interface::{
        deploy_ecosystem::input::{GenesisInput, InitialDeploymentConfig},
        script_params::GATEWAY_GOVERNANCE_TX_PATH1,
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zksync_basic_types::H256;
use zksync_config::configs::gateway::GatewayConfig;
use zksync_types::{Address, L1ChainId};

use super::{
    admin_call_builder::AdminCallBuilder,
    utils::{display_admin_script_output, get_default_foundry_path},
};
use crate::{
    accept_ownership::{grant_gateway_whitelist, AdminScriptMode, AdminScriptOutput},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct GrantGatewayWhitelistArgs {
    pub gateway_chain_id: u64,

    pub bridgehub_addr: Address,

    /// Who to grant the whitelist to.
    pub grantee: Address,

    pub l1_rpc_url: String,
}

pub async fn run(shell: &Shell, args: GrantGatewayWhitelistArgs) -> anyhow::Result<()> {
    let result = grant_gateway_whitelist(
        shell,
        // We do not care about forge args that much here, since
        // we only need to obtain the calldata
        &Default::default(),
        &get_default_foundry_path()?,
        AdminScriptMode::OnlySave,
        args.gateway_chain_id,
        args.bridgehub_addr,
        args.grantee,
        args.l1_rpc_url,
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
