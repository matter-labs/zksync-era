use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::{abigen, BaseContract},
    providers::{Http, Middleware, Provider},
    types::Bytes,
    utils::hex,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_yaml::with;
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config,
    forge::{Forge, ForgeScriptArgs},
    logger,
    spinner::Spinner,
    wallets::Wallet,
    zks_provider::{FinalizeWithdrawalParams, ZKSProvider},
};
use zkstack_cli_config::{
    forge_interface::script_params::GATEWAY_UTILS_SCRIPT_PATH,
    traits::{ReadConfig, SaveConfig},
    ContractsConfig, EcosystemConfig,
};
use zksync_basic_types::{H256, U256, U64};
use zksync_types::{L2ChainId, L2_BRIDGEHUB_ADDRESS};
use zksync_web3_decl::{
    client::{Client, L2},
    namespaces::EthNamespaceClient,
};

use super::{
    gateway_migration::MigrationDirection,
    gateway_migration_calldata::{
        get_gateway_migration_state, get_migration_transaction, GatewayMigrationProgressState,
    },
    migrate_from_gateway::finish_migrate_chain_from_gateway,
    utils::{display_admin_script_output, get_default_foundry_path},
};
use crate::{
    accept_ownership::{set_da_validator_pair, start_migrate_chain_from_gateway, AdminScriptMode},
    commands::chain::{
        admin_call_builder::AdminCallBuilder,
        gateway_migration::{
            await_for_tx_to_complete, extract_and_wait_for_priority_ops, extract_priority_ops,
            send_tx,
        },
        init::get_l1_da_validator,
        utils::{get_ethers_provider, get_zk_client},
    },
    messages::{MSG_CHAIN_NOT_INITIALIZED, MSG_DA_PAIR_REGISTRATION_SPINNER},
    utils::forge::{check_the_balance, fill_forge_private_key, WalletOwner},
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateFromGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,
}

lazy_static! {
    static ref GATEWAY_UTILS_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrateChainFromGateway(address bridgehubAddr, uint256 migratingChainId, uint256 gatewayChainId, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes memory message, bytes32[] memory merkleProof) public",
        ])
        .unwrap(),
    );
}

#[derive(Parser, Debug)]
#[command()]
pub(crate) struct FinalizeChainMigrationFromGatewayScriptArgs {
    pub l1_rpc_url: String,
    pub l1_bridgehub_addr: Address,
    pub l2_chain_id: u64,
    pub gateway_chain_id: u64,

    pub gateway_rpc_url: String,

    pub private_key: String,

    /// RPC URL of the chain being migrated (L2).
    pub l2_rpc_url: Option<String>,

    /// Whether to force providing the full migration calldata even if the chain
    /// isn't strictly ready for final calls.
    pub no_cross_check: Option<bool>,
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(
    shell: &Shell,
    params: FinalizeChainMigrationFromGatewayScriptArgs,
) -> anyhow::Result<()> {
    let contracts_foundry_path = get_default_foundry_path(shell)?;

    let should_cross_check = !params.no_cross_check.unwrap_or_default();

    if should_cross_check {
        let state = get_gateway_migration_state(
            params.l1_rpc_url.clone(),
            params.l1_bridgehub_addr,
            params.l2_chain_id,
            params
                .l2_rpc_url
                .clone()
                .context("L2 RPC URL must be provided for cross checking")?,
            params.gateway_rpc_url.clone(),
            MigrationDirection::ToGateway,
        )
        .await?;

        match state {
            GatewayMigrationProgressState::NotStarted => {
                logger::warn("Notification has not yet been sent. Please use the command to send notification first.");
                return Ok(());
            }
            GatewayMigrationProgressState::NotificationSent => {
                logger::info("Notification has been sent, but the server has not yet picked it up. Please wait");
                return Ok(());
            }
            GatewayMigrationProgressState::NotificationReceived => {
                logger::info("The server has received the notification about the migration, but it needs to finish all outstanding transactions. Please wait");
                return Ok(());
            }
            GatewayMigrationProgressState::ServerReady => {
                logger::info(
                    "The server is ready to start the migration. Please use the command to prepare the calldata",
                );
                return Ok(());
            }
            GatewayMigrationProgressState::AwaitingFinalization => {
                logger::info("The transaction to migrate chain on top of Gateway has been processed, but the GW chain has not yet finalized it");
                return Ok(());
            }
            GatewayMigrationProgressState::PendingManualFinalization => {
                logger::info("The chain migration to Gateway has been finalized on the Gateway side. Finalizing the migration...");
            }
            GatewayMigrationProgressState::Finished => {
                logger::info("The migration in this direction has been already finished");
                return Ok(());
            }
        }
    }

    let migraiton_tx = get_migration_transaction(
        &params.gateway_rpc_url,
        L2_BRIDGEHUB_ADDRESS,
        params.l2_chain_id,
    )
    .await?
    .context("Failed to find the transaction where the migration from GW to L1 happened")?;

    let gateway_zk_client = get_zk_client(&params.gateway_rpc_url, params.l2_chain_id)?;

    let withdrawal_params = gateway_zk_client
        .get_finalize_withdrawal_params(migraiton_tx, 0)
        .await?;

    finish_migrate_chain_from_gateway(
        shell,
        Default::default(),
        &get_default_foundry_path(shell)?,
        Wallet::new(
            params
                .private_key
                .parse()
                .context("Failed to parse private key")?,
        ),
        params.l1_bridgehub_addr,
        params.l2_chain_id,
        params.gateway_chain_id,
        withdrawal_params,
        params.l1_rpc_url,
    )
    .await?;

    Ok(())
}
