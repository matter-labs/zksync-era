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
    traits::{ReadConfig, SaveConfig, SaveConfigWithBasePath},
    ChainConfig, EcosystemConfig,
};
use zksync_basic_types::H256;
use zksync_types::{Address, L1ChainId};

use super::{
    admin_call_builder::AdminCallBuilder,
    utils::{display_admin_script_output, get_default_foundry_path},
};
use crate::{
    accept_ownership::{set_transaction_filterer, AdminScriptMode},
    messages::MSG_CHAIN_NOT_INITIALIZED,
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct SetTransactionFiltererArgs {
    /// Gateway transaction filterer
    pub transaction_filterer: Address,

    /// Bridgehub address
    pub bridgehub_address: Address,

    /// The address of the ZK chain diamond proxy
    pub chain_id: u64,

    pub l1_rpc_url: String,
}

pub async fn run(shell: &Shell, args: SetTransactionFiltererArgs) -> anyhow::Result<()> {
    let result = set_transaction_filterer(
        shell,
        &Default::default(),
        &get_default_foundry_path(shell)?,
        AdminScriptMode::OnlySave,
        args.chain_id,
        args.bridgehub_address,
        args.transaction_filterer,
        args.l1_rpc_url,
    )
    .await?;

    display_admin_script_output(result);

    Ok(())
}
