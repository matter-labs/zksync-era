use anyhow::Context;
use clap::Parser;
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::{
    config::global_config, ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger,
};
use zkstack_cli_config::{ChainConfig, GatewayChainConfigPatch, ZkStackConfig, ZkStackConfigTrait};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::U256;
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

use super::{
    constants::DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
    gateway_common::extract_and_wait_for_priority_ops,
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
}

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
    let context =
        get_migrate_to_gateway_context(&chain_config, &gateway_chain_config, false).await?;
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

    let receipt = {
        let mut receipt = None;
        let max_attempts = 5;
        for attempt in 1..=max_attempts {
            match send_tx(
                chain_admin,
                calldata.clone(),
                value,
                context.l1_rpc_url.clone(),
                chain_config
                    .get_wallets_config()?
                    .governor
                    .private_key_h256()
                    .unwrap(),
                "migrating to gateway",
            )
            .await
            {
                Ok(r) if r.status == Some(1.into()) => {
                    receipt = Some(r);
                    break;
                }
                Ok(_) if attempt < max_attempts => {
                    logger::warn(format!(
                        "Transaction reverted, retrying ({}/{})...",
                        attempt + 1,
                        max_attempts
                    ));
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                }
                Ok(_) => anyhow::bail!("Migration failed after {} attempts", max_attempts),
                Err(e) => return Err(e),
            }
        }
        receipt.unwrap()
    };

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

pub(crate) async fn get_migrate_to_gateway_context(
    chain_config: &ChainConfig,
    gateway_chain_config: &ChainConfig,
    skip_pre_migration_checks: bool,
) -> anyhow::Result<MigrateToGatewayContext> {
    let gateway_gateway_config = gateway_chain_config
        .get_gateway_config()
        .context("Gateway config not present")?;

    let l1_url = chain_config.get_secrets_config().await?.l1_rpc_url()?;

    let genesis_config = chain_config.get_genesis_config().await?;

    let chain_contracts_config = chain_config.get_contracts_config().unwrap();

    let general_config = gateway_chain_config.get_general_config().await?;
    let gw_rpc_url = general_config.l2_http_url()?;

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
        l1_rpc_url: l1_url.clone(),
        l1_bridgehub_addr: chain_contracts_config
            .ecosystem_contracts
            .bridgehub_proxy_addr,
        max_l1_gas_price: DEFAULT_MAX_L1_GAS_PRICE_FOR_PRIORITY_TXS,
        l2_chain_id: chain_config.chain_id.as_u64(),
        gateway_chain_id: gateway_chain_config.chain_id.as_u64(),
        gateway_diamond_cut: gateway_gateway_config.diamond_cut_data.0.clone(),
        gateway_rpc_url: gw_rpc_url.clone(),
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
