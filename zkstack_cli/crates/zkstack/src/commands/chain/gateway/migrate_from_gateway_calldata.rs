use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    utils::hex,
};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::logger;
use zkstack_cli_config::{traits::ReadConfig, ContractsConfig};

use super::gateway_common::{
    get_gateway_migration_state, GatewayMigrationProgressState, MigrationDirection,
};
use crate::{
    admin_functions::{start_migrate_chain_from_gateway, AdminScriptMode},
    commands::chain::utils::{display_admin_script_output, get_default_foundry_path},
};

lazy_static! {
    static ref GATEWAY_UTILS_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrateChainFromGateway(address bridgehubAddr, uint256 migratingChainId, uint256 gatewayChainId, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes memory message, bytes32[] memory merkleProof) public",
        ])
        .unwrap(),
    );
}

#[derive(Parser, Debug)]
pub struct MigrateFromGatewayCalldataArgs {
    #[clap(long)]
    pub l1_rpc_url: String,
    #[clap(long)]
    pub l1_bridgehub_addr: Address,
    #[clap(long)]
    pub max_l1_gas_price: u64,
    #[clap(long)]
    pub l2_chain_id: u64,
    #[clap(long)]
    pub gateway_chain_id: u64,
    #[clap(long)]
    pub ecosystem_contracts_config_path: String,
    #[clap(long)]
    pub gateway_rpc_url: String,
    #[clap(long)]
    pub refund_recipient: Address,

    /// RPC URL of the chain being migrated (L2).
    #[clap(long)]
    pub l2_rpc_url: Option<String>,

    /// Whether to force providing the full migration calldata even if the chain
    /// isn't strictly ready for final calls.
    #[clap(long, default_missing_value = "false")]
    pub no_cross_check: bool,
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(shell: &Shell, params: MigrateFromGatewayCalldataArgs) -> anyhow::Result<()> {
    let forge_args = Default::default();
    let contracts_foundry_path = get_default_foundry_path(shell)?;

    if !params.no_cross_check {
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
                    "The server is ready to start the migration. Preparing the calldata...",
                );
                logger::warn("Important! It may take awhile for Gateway to detect the migration transaction. If you are sure you've already sent it, no need to resend it");
                // It is the expected case, it will be handled later in the file
            }
            GatewayMigrationProgressState::AwaitingFinalization => {
                logger::info("The transaction to migrate chain on top of Gateway has been processed, but the GW chain has not yet finalized it");
                return Ok(());
            }
            GatewayMigrationProgressState::PendingManualFinalization => {
                logger::info("The chain migration to Gateway has been finalized on the Gateway side. Please use the script to finalize its migration to L1");
                return Ok(());
            }
            GatewayMigrationProgressState::Finished => {
                logger::info("The migration in this direction has been already finished");
                return Ok(());
            }
        }
    }

    let ecosystem_contracts_config =
        ContractsConfig::read(shell, &params.ecosystem_contracts_config_path)
            .context("Failed to read the gateway config path")?;

    let output = start_migrate_chain_from_gateway(
        shell,
        &forge_args,
        &contracts_foundry_path,
        AdminScriptMode::OnlySave,
        params.l1_bridgehub_addr,
        params.max_l1_gas_price,
        params.l2_chain_id,
        params.gateway_chain_id,
        hex::decode(
            &ecosystem_contracts_config
                .ecosystem_contracts
                .diamond_cut_data,
        )
        .context("Failed to decode diamond cut data")?
        .into(),
        params.refund_recipient,
        params.l1_rpc_url,
    )
    .await?;

    // TODO(EVM-1000): The output below only contains the data needed to start migration from the Gateway.
    // However after the migration is finalized, the chain admin will have to reset the validator DA pair on L1.
    // This calldata is not yet present here, but it should be.

    display_admin_script_output(output);

    Ok(())
}
