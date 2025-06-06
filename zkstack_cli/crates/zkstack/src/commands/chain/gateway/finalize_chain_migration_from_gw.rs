use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{
    ethereum::get_zk_client, logger, wallets::Wallet, zks_provider::ZKSProvider,
};
use zksync_types::L2_BRIDGEHUB_ADDRESS;

use super::{
    gateway_common::{
        get_gateway_migration_state, get_migration_transaction, GatewayMigrationProgressState,
        MigrationDirection,
    },
    messages::message_for_gateway_migration_progress_state,
    migrate_from_gateway::finish_migrate_chain_from_gateway,
};
use crate::commands::chain::utils::get_default_foundry_path;

lazy_static! {
    static ref GATEWAY_UTILS_INTERFACE: BaseContract = BaseContract::from(
        parse_abi(&[
            "function finishMigrateChainFromGateway(address bridgehubAddr, uint256 migratingChainId, uint256 gatewayChainId, uint256 l2BatchNumber, uint256 l2MessageIndex, uint16 l2TxNumberInBatch, bytes memory message, bytes32[] memory merkleProof) public",
        ])
        .unwrap(),
    );
}

#[derive(Parser, Debug)]
pub struct FinalizeChainMigrationFromGatewayArgs {
    #[clap(long)]
    pub l1_rpc_url: String,
    #[clap(long)]
    pub l1_bridgehub_addr: Address,
    #[clap(long)]
    pub l2_chain_id: u64,
    #[clap(long)]
    pub gateway_chain_id: u64,
    #[clap(long)]
    pub gateway_rpc_url: String,
    #[clap(long)]
    pub private_key: String,
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
pub async fn run(
    shell: &Shell,
    params: FinalizeChainMigrationFromGatewayArgs,
) -> anyhow::Result<()> {
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
            MigrationDirection::FromGateway,
        )
        .await?;

        match state {
            GatewayMigrationProgressState::PendingManualFinalization => {
                logger::info("The chain migration to Gateway has been finalized on the Gateway side. Finalizing the migration...");
            }
            _ => {
                anyhow::bail!(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::FromGateway,
                ));
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
