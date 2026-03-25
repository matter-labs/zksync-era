use std::time::{Duration, Instant};

use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config, ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger,
};
use zkstack_cli_config::{ChainConfig, GatewayChainConfigPatch, ZkStackConfig, ZkStackConfigTrait};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::{Address, U256};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

use super::{
    constants::DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
    gateway_common::{
        extract_and_wait_for_priority_ops, get_gateway_migration_state,
        GatewayMigrationProgressState, MigrationDirection,
    },
    messages::message_for_gateway_migration_progress_state,
    migrate_to_gateway_calldata::{get_migrate_to_gateway_calls, MigrateToGatewayConfig},
};
use crate::{
    abi::BridgehubAbi,
    commands::chain::{
        admin_call_builder::AdminCallBuilder,
        gateway::migrate_to_gateway_calldata::MigrateToGatewayContext, utils::send_tx,
    },
    messages::MSG_CHAIN_NOT_INITIALIZED,
};

#[derive(Debug, Serialize, Deserialize, Parser)]
pub struct MigrateToGatewayArgs {
    /// All ethereum environment related arguments
    #[clap(flatten)]
    #[serde(flatten)]
    pub forge_args: ForgeScriptArgs,

    #[clap(long)]
    pub gateway_chain_name: String,

    /// L1 RPC URL. If not provided, will be read from chain secrets config.
    #[clap(long)]
    pub l1_rpc_url: Option<String>,

    /// Gateway RPC URL. If not provided, will be read from gateway chain's general config.
    #[clap(long)]
    pub gateway_rpc_url: Option<String>,
}

const LOOK_WAITING_TIME_MS: u64 = 2000;
const MIGRATION_READY_TIMEOUT: Duration = Duration::from_secs(10 * 60);

pub async fn run(args: MigrateToGatewayArgs, shell: &Shell) -> anyhow::Result<()> {
    let ecosystem_config = ZkStackConfig::ecosystem(shell)?;

    let chain_name = global_config().chain_name.clone();
    let chain_config = ecosystem_config
        .load_chain(chain_name)
        .context(MSG_CHAIN_NOT_INITIALIZED)?;

    let gateway_chain_config = ecosystem_config
        .load_chain(Some(args.gateway_chain_name.clone()))
        .context("Gateway not present")?;
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    logger::info("Migrating the chain to the Gateway...");

    let l1_rpc_url = match args.l1_rpc_url {
        Some(url) => url,
        None => chain_config.get_secrets_config().await?.l1_rpc_url()?,
    };

    let gateway_rpc_url = match args.gateway_rpc_url {
        Some(url) => url,
        None => {
            let general_config = gateway_chain_config.get_general_config().await?;
            general_config.l2_http_url()?
        }
    };
    let l2_rpc_url = chain_config
        .get_general_config()
        .await?
        .l2_http_url()
        .context("L2 RPC URL must be provided for cross checking")?;
    let chain_contracts_config = chain_config.get_contracts_config()?;

    wait_for_migration_to_gateway_ready(
        l1_rpc_url.clone(),
        chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        chain_config.chain_id.as_u64(),
        l2_rpc_url,
        gateway_rpc_url.clone(),
    )
    .await?;

    let context = get_migrate_to_gateway_context(
        &chain_config,
        &gateway_chain_config,
        false,
        l1_rpc_url,
        gateway_rpc_url,
    )
    .await?;
    let (chain_admin, calls) = get_migrate_to_gateway_calls(
        shell,
        &args.forge_args,
        &chain_config.path_to_foundry_scripts(),
        &context,
    )
    .await?;

    if calls.is_empty() {
        logger::info("Chain already migrated!");
        return Ok(());
    }

    let (calldata, value) = AdminCallBuilder::new(calls).compile_full_calldata();

    let receipt = send_tx(
        chain_admin,
        calldata,
        value,
        context.l1_rpc_url.clone(),
        chain_config
            .get_wallets_config()?
            .governor
            .private_key_h256()
            .unwrap(),
        "migrating to gateway",
    )
    .await?;

    let gateway_provider = get_ethers_provider(&context.gateway_rpc_url)?;
    extract_and_wait_for_priority_ops(
        receipt,
        gateway_chain_config
            .get_contracts_config()?
            .l1
            .diamond_proxy_addr,
        gateway_provider.clone(),
    )
    .await?;

    let mut chain_secrets_config = chain_config.get_secrets_config().await?.patched();
    chain_secrets_config.set_gateway_rpc_url(context.gateway_rpc_url.clone())?;
    chain_secrets_config.save().await?;

    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gateway_provider);

    let mut gateway_chain_config =
        GatewayChainConfigPatch::empty(shell, &chain_config.path_to_gateway_chain_config());
    gateway_chain_config.init(
        &gateway_gateway_config,
        gw_bridgehub
            .get_zk_chain(chain_config.chain_id.as_u64().into())
            .await?,
        context.gateway_chain_id.into(),
    )?;
    gateway_chain_config.save().await?;

    Ok(())
}

async fn wait_for_migration_to_gateway_ready(
    l1_rpc_url: String,
    l1_bridgehub_addr: Address,
    l2_chain_id: u64,
    l2_rpc_url: String,
    gw_rpc_url: String,
) -> anyhow::Result<()> {
    let started_at = Instant::now();

    loop {
        let state = get_gateway_migration_state(
            l1_rpc_url.clone(),
            l1_bridgehub_addr,
            l2_chain_id,
            l2_rpc_url.clone(),
            gw_rpc_url.clone(),
            MigrationDirection::ToGateway,
        )
        .await?;

        match state {
            GatewayMigrationProgressState::ServerReady => {
                logger::info("The server is ready for migration to Gateway");
                return Ok(());
            }
            GatewayMigrationProgressState::Finished => {
                logger::info(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                ));
                return Ok(());
            }
            GatewayMigrationProgressState::NotificationSent
            | GatewayMigrationProgressState::NotificationReceived(_) => {
                if started_at.elapsed() > MIGRATION_READY_TIMEOUT {
                    anyhow::bail!(
                        "Timed out waiting for migration readiness: {}",
                        message_for_gateway_migration_progress_state(
                            state,
                            MigrationDirection::ToGateway,
                        )
                    );
                }

                logger::info(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                ));
                tokio::time::sleep(Duration::from_millis(LOOK_WAITING_TIME_MS)).await;
            }
            GatewayMigrationProgressState::NotStarted
            | GatewayMigrationProgressState::AwaitingFinalization
            | GatewayMigrationProgressState::PendingManualFinalization => {
                anyhow::bail!(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                ));
            }
        }
    }
}

pub(crate) async fn get_migrate_to_gateway_context(
    chain_config: &ChainConfig,
    gateway_chain_config: &ChainConfig,
    skip_pre_migration_checks: bool,
    l1_rpc_url: String,
    gateway_rpc_url: String,
) -> anyhow::Result<MigrateToGatewayContext> {
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let genesis_config = chain_config.get_genesis_config().await?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();

    let is_rollup = matches!(
        genesis_config.l1_batch_commitment_mode()?,
        L1BatchCommitmentMode::Rollup
    );

    let gateway_da_validator_address = if is_rollup {
        gateway_gateway_config.relayed_sl_da_validator
    } else {
        gateway_gateway_config.validium_da_validator
    };
    let chain_secrets_config = chain_config.get_wallets_config().unwrap();

    let config = MigrateToGatewayConfig {
        l1_rpc_url,
        l1_bridgehub_addr: chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        max_l1_gas_price: DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
        l2_chain_id: chain_config.chain_id.as_u64(),
        gateway_chain_id: gateway_chain_config.chain_id.as_u64(),
        gateway_diamond_cut: gateway_gateway_config.diamond_cut_data.0.clone(),
        gateway_rpc_url,
        new_sl_da_validator: gateway_da_validator_address,
        validator: chain_secrets_config.operator.address,
        min_validator_balance: U256::from(10).pow(19.into()),
        refund_recipient: None,
    };

    if skip_pre_migration_checks {
        Ok(config.into_context_unchecked().await?)
    } else {
        Ok(config.into_context().await?)
    }
}
