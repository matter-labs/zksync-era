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
        script_params::{DEPLOY_GATEWAY_CTM, GATEWAY_GOVERNANCE_TX_PATH1, GATEWAY_PREPARATION},
    },
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zksync_basic_types::H256;
use zksync_config::configs::gateway::GatewayConfig;
use zksync_types::{Address, L1ChainId};

use super::{
    admin_call_builder::{AdminCall, AdminCallBuilder},
    gateway_migration::MigrationDirection,
    grant_gateway_whitelist::{display_admin_script_output, get_default_foundry_path},
};
use crate::{
    accept_ownership::{
        grant_gateway_whitelist, notify_server_migration_from_gateway,
        notify_server_migration_to_gateway, AdminScriptMode, AdminScriptOutput,
    },
    commands::chain::admin_call_builder::encode_admin_multicall,
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct NotifyServerCalldataArgs {
    pub bridgehub_addr: Address,
    pub l2_chain_id: u64,
    pub l1_rpc_url: String,
}

pub async fn get_notify_server_calls(
    shell: &Shell,
    forge_args: ForgeScriptArgs,
    forge_path: &Path,
    args: NotifyServerCalldataArgs,
    direction: MigrationDirection,
) -> anyhow::Result<AdminScriptOutput> {
    let admin_call_output = match direction {
        MigrationDirection::FromGateway => {
            notify_server_migration_from_gateway(
                shell,
                &forge_args,
                forge_path,
                AdminScriptMode::OnlySave,
                args.l2_chain_id,
                args.bridgehub_addr,
                args.l1_rpc_url,
            )
            .await
        }
        MigrationDirection::ToGateway => {
            notify_server_migration_to_gateway(
                shell,
                &forge_args,
                forge_path,
                AdminScriptMode::OnlySave,
                args.l2_chain_id,
                args.bridgehub_addr,
                args.l1_rpc_url,
            )
            .await
        }
    }?;

    Ok(admin_call_output)
}

pub async fn run(
    shell: &Shell,
    args: NotifyServerCalldataArgs,
    direction: MigrationDirection,
) -> anyhow::Result<()> {
    let result = get_notify_server_calls(
        shell,
        // We do not care about forge args that much here, since
        // we only need to obtain the calldata
        Default::default(),
        &get_default_foundry_path()?,
        args,
        direction,
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
