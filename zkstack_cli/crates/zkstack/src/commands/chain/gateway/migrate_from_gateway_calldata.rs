use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{parse_abi, Address},
    contract::BaseContract,
    utils::hex,
};
use lazy_static::lazy_static;
use xshell::Shell;
use zkstack_cli_common::{ethereum::get_ethers_provider, logger};
use zkstack_cli_config::{traits::ReadConfig, ContractsConfig, ZkStackConfig, ZkStackConfigTrait};

use super::{
    gateway_common::{
        get_gateway_migration_state, GatewayMigrationProgressState, MigrationDirection,
    },
    messages::{message_for_gateway_migration_progress_state, USE_SET_DA_VALIDATOR_COMMAND_INFO},
};
use crate::{
    abi::{BridgehubAbi, ZkChainAbi},
    admin_functions::{start_migrate_chain_from_gateway, AdminScriptMode},
    commands::chain::utils::display_admin_script_output,
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
    #[clap(long, default_missing_value = "true")]
    pub no_cross_check: bool,
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(shell: &Shell, params: MigrateFromGatewayCalldataArgs) -> anyhow::Result<()> {
    let forge_args = Default::default();
    let contracts_foundry_path = ZkStackConfig::from_file(shell)?.path_to_foundry_scripts();

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
            GatewayMigrationProgressState::ServerReady => {
                logger::info(
                    "The server is ready to start the migration. Preparing the calldata...",
                );
                logger::warn("Important! It may take a while for Gateway to detect the migration transaction. If you are sure you've already sent it, no need to resend it");
                // It is the expected case, it will be handled later in the file
            }
            GatewayMigrationProgressState::AwaitingFinalization => {
                anyhow::bail!("The transaction to migrate chain on top of Gateway has been processed, but the GW chain has not yet finalized it");
            }
            GatewayMigrationProgressState::PendingManualFinalization => {
                anyhow::bail!("The chain migration to Gateway has been finalized on the Gateway side. Please use the corresponding command to finalize its migration to L1");
            }
            GatewayMigrationProgressState::Finished => {
                let l1_provider = get_ethers_provider(&params.l1_rpc_url)?;
                let bridgehub = BridgehubAbi::new(params.l1_bridgehub_addr, l1_provider.clone());
                let zk_chain_address = bridgehub.get_zk_chain(params.l2_chain_id.into()).await?;
                if zk_chain_address == Address::zero() {
                    anyhow::bail!("Chain does not exist");
                }

                let zk_chain = ZkChainAbi::new(zk_chain_address, l1_provider);
                let (l1_da_validator, l2_da_validator) = zk_chain.get_da_validator_pair().await?;

                // We always output the original message, but we provide additional helper log in case
                // the DA validator is not yet set
                let basic_message = message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::FromGateway,
                );
                logger::info(&basic_message);

                if l1_da_validator == Address::zero() || l2_da_validator == Address::zero() {
                    logger::warn("The DA validators are not yet set on the diamond proxy.");
                    logger::info(USE_SET_DA_VALIDATOR_COMMAND_INFO);
                }

                return Ok(());
            }
            GatewayMigrationProgressState::NotStarted
            | GatewayMigrationProgressState::NotificationSent
            | GatewayMigrationProgressState::NotificationReceived(_) => {
                anyhow::bail!(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::FromGateway,
                ));
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
                .ctm
                .diamond_cut_data,
        )
        .context("Failed to decode diamond cut data")?
        .into(),
        params.refund_recipient,
        params.l1_rpc_url,
    )
    .await?;

    display_admin_script_output(output);

    logger::warn("Note, that the above calldata ONLY includes calldata to start migration from ZK Gateway to L1. Once the migration finishes, the DA validator pair will be reset and so you will have to set again. ");
    logger::info(USE_SET_DA_VALIDATOR_COMMAND_INFO);

    Ok(())
}
