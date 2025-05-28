// src/commands/chain/migrate_to_gateway_calldata.rs

use std::path::Path;

use anyhow::Context;
use clap::Parser;
use ethers::{
    abi::{encode, Token},
    providers::{Middleware, Provider},
};
use xshell::Shell;
use zkstack_cli_common::{ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{traits::ReadConfig, GatewayConfig};
use zksync_basic_types::{Address, H256, U256};
use zksync_system_constants::{L2_BRIDGEHUB_ADDRESS, L2_CHAIN_ASSET_HANDLER_ADDRESS};

use super::{
    gateway_common::{
        get_gateway_migration_state, GatewayMigrationProgressState, MigrationDirection,
    },
    messages::message_for_gateway_migration_progress_state,
};
use crate::{
    abi::{BridgehubAbi, ChainTypeManagerAbi, ValidatorTimelockAbi, ZkChainAbi},
    admin_functions::{
        admin_l1_l2_tx, enable_validator_via_gateway, finalize_migrate_to_gateway,
        set_da_validator_pair_via_gateway, AdminScriptOutput,
    },
    commands::chain::{
        admin_call_builder::AdminCall,
        utils::{display_admin_script_output, get_default_foundry_path},
    },
    utils::addresses::apply_l1_to_l2_alias,
};

// The most reliable way to precompute the address is to simulate `createNewChain` function
async fn precompute_chain_address_on_gateway(
    l2_chain_id: u64,
    base_token_asset_id: H256,
    new_l2_admin: Address,
    protocol_version: U256,
    gateway_diamond_cut: Vec<u8>,
    gw_ctm: ChainTypeManagerAbi<Provider<ethers::providers::Http>>,
) -> anyhow::Result<Address> {
    let ctm_data = encode(&[
        Token::FixedBytes(base_token_asset_id.0.into()),
        Token::Address(new_l2_admin),
        Token::Uint(protocol_version),
        Token::Bytes(gateway_diamond_cut),
    ]);

    let result = gw_ctm
        .forwarded_bridge_mint(l2_chain_id.into(), ctm_data.into())
        .from(L2_CHAIN_ASSET_HANDLER_ADDRESS)
        .await?;

    Ok(result)
}

#[derive(Parser, Debug)]
pub(crate) struct MigrateToGatewayParams {
    pub(crate) l1_rpc_url: String,
    pub(crate) l1_bridgehub_addr: Address,
    pub(crate) max_l1_gas_price: u64,
    pub(crate) l2_chain_id: u64,
    pub(crate) gateway_chain_id: u64,
    pub(crate) gateway_diamond_cut: Vec<u8>,
    pub(crate) gateway_rpc_url: String,
    pub(crate) new_sl_da_validator: Address,
    pub(crate) validator_1: Address,
    pub(crate) validator_2: Address,
    pub(crate) min_validator_balance: U256,
    pub(crate) refund_recipient: Option<Address>,
}

pub(crate) async fn get_migrate_to_gateway_calls(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    params: MigrateToGatewayParams,
) -> anyhow::Result<(Address, Vec<AdminCall>)> {
    let refund_recipient = params.refund_recipient.unwrap_or(params.validator_1);
    let mut result = vec![];

    let l1_provider = get_ethers_provider(&params.l1_rpc_url)?;
    let gw_provider = get_ethers_provider(&params.gateway_rpc_url)?;

    let l1_bridgehub = BridgehubAbi::new(params.l1_bridgehub_addr, l1_provider.clone());
    let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_provider.clone());

    let current_settlement_layer = l1_bridgehub
        .settlement_layer(params.l2_chain_id.into())
        .await?;

    let zk_chain_l1_address = l1_bridgehub.get_zk_chain(params.l2_chain_id.into()).await?;

    if zk_chain_l1_address == Address::zero() {
        anyhow::bail!("Chain with id {} does not exist!", params.l2_chain_id);
    }

    // Checking whether the user has already done the migration
    if current_settlement_layer == U256::from(params.gateway_chain_id) {
        // TODO(EVM-1001): it may happen that the user has started the migration, but it failed for some reason (e.g. the provided
        // diamond cut was not correct).
        // The recovery of the chain is not handled by the tool right now.
        anyhow::bail!("The chain is already on top of Gateway!");
    }

    let ctm_asset_id = l1_bridgehub
        .ctm_asset_id_from_chain_id(params.l2_chain_id.into())
        .await?;
    let ctm_gw_address = gw_bridgehub.ctm_asset_id_to_address(ctm_asset_id).await?;

    if ctm_gw_address == Address::zero() {
        anyhow::bail!("{} does not have a CTM deployed!", params.gateway_chain_id);
    }

    let gw_ctm = ChainTypeManagerAbi::new(ctm_gw_address, gw_provider.clone());
    let gw_validator_timelock_addr = gw_ctm.validator_timelock().await?;
    let _gw_validator_timelock =
        ValidatorTimelockAbi::new(gw_validator_timelock_addr, gw_provider.clone());

    let l1_zk_chain = ZkChainAbi::new(zk_chain_l1_address, l1_provider.clone());
    let chain_admin_address = l1_zk_chain.get_admin().await?;
    let zk_chain_gw_address = {
        let recorded_zk_chain_gw_address =
            gw_bridgehub.get_zk_chain(params.l2_chain_id.into()).await?;
        if recorded_zk_chain_gw_address == Address::zero() {
            precompute_chain_address_on_gateway(
                params.l2_chain_id,
                H256(
                    l1_bridgehub
                        .base_token_asset_id(params.l2_chain_id.into())
                        .await?,
                ),
                apply_l1_to_l2_alias(l1_zk_chain.get_admin().await?),
                l1_zk_chain.get_protocol_version().await?,
                params.gateway_diamond_cut.clone(),
                gw_ctm,
            )
            .await?
        } else {
            recorded_zk_chain_gw_address
        }
    };

    let finalize_migrate_to_gateway_output = finalize_migrate_to_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::admin_functions::AdminScriptMode::OnlySave,
        params.l1_bridgehub_addr,
        params.max_l1_gas_price,
        params.l2_chain_id,
        params.gateway_chain_id,
        params.gateway_diamond_cut.into(),
        refund_recipient,
        params.l1_rpc_url.clone(),
    )
    .await?;

    result.extend(finalize_migrate_to_gateway_output.calls);

    // Changing L2 DA validator while migrating to gateway is not recommended; we allow changing only the settlement layer one
    let (_, l2_da_validator) = l1_zk_chain.get_da_validator_pair().await?;

    // Unfortunately, there is no getter for whether a chain is a permanent rollup, we have to
    // read storage here.
    let is_permanent_rollup_slot = l1_provider
        .get_storage_at(zk_chain_l1_address, H256::from_low_u64_be(57), None)
        .await?;
    if is_permanent_rollup_slot == H256::from_low_u64_be(1) {
        // TODO(EVM-1002): We should really check it on our own here, but it is hard with the current interfaces
        logger::warn("WARNING: Your chain is a permanent rollup! Ensure that the new settlement layer DA provider is compatible with Gateway RollupDAManager!");
    }

    let da_validator_encoding_result = set_da_validator_pair_via_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::admin_functions::AdminScriptMode::OnlySave,
        params.l1_bridgehub_addr,
        params.max_l1_gas_price.into(),
        params.l2_chain_id,
        params.gateway_chain_id,
        params.new_sl_da_validator,
        l2_da_validator,
        zk_chain_gw_address,
        refund_recipient,
        params.l1_rpc_url.clone(),
    )
    .await?;

    result.extend(da_validator_encoding_result.calls.into_iter());

    // 4. If validators are not yet present, please include.
    for validator in [params.validator_1, params.validator_2] {
        // if !gw_validator_timelock
        //     .validators(params.l2_chain_id.into(), validator)
        //     .await?
        // {
        let enable_validator_calls = enable_validator_via_gateway(
            shell,
            forge_args,
            foundry_contracts_path,
            crate::admin_functions::AdminScriptMode::OnlySave,
            params.l1_bridgehub_addr,
            params.max_l1_gas_price.into(),
            params.l2_chain_id,
            params.gateway_chain_id,
            validator,
            gw_validator_timelock_addr,
            refund_recipient,
            params.l1_rpc_url.clone(),
        )
        .await?;
        result.extend(enable_validator_calls.calls);
        // }

        let current_validator_balance = gw_provider.get_balance(validator, None).await?;
        logger::info(format!(
            "Current balance of {:#?} = {}",
            validator, current_validator_balance
        ));
        if current_validator_balance < params.min_validator_balance {
            logger::info(format!(
                "Will send {} of the ZK Gateway base token",
                params.min_validator_balance - current_validator_balance
            ));
            let supply_validator_balance_calls = admin_l1_l2_tx(
                shell,
                forge_args,
                foundry_contracts_path,
                crate::admin_functions::AdminScriptMode::OnlySave,
                params.l1_bridgehub_addr,
                params.max_l1_gas_price,
                params.gateway_chain_id,
                validator,
                params.min_validator_balance - current_validator_balance,
                Default::default(),
                refund_recipient,
                params.l1_rpc_url.clone(),
            )
            .await?;
            result.extend(supply_validator_balance_calls.calls);
        }
    }

    Ok((chain_admin_address, result))
}

#[derive(Parser, Debug)]
pub struct MigrateToGatewayCalldataArgs {
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
    pub gateway_config_path: String,
    #[clap(long)]
    pub gateway_rpc_url: String,
    #[clap(long)]
    pub new_sl_da_validator: Address,
    #[clap(long)]
    pub validator_1: Address,
    #[clap(long)]
    pub validator_2: Address,
    #[clap(long)]
    pub min_validator_balance: U256,
    #[clap(long)]
    pub refund_recipient: Option<Address>,

    /// RPC URL of the chain being migrated (L2).
    #[clap(long)]
    pub l2_rpc_url: Option<String>,

    /// Whether to force providing the full migration calldata even if the chain
    /// isn't strictly ready for final calls.
    #[clap(long, default_missing_value = "false")]
    pub no_cross_check: Option<bool>,
}

impl MigrateToGatewayCalldataArgs {
    /// Converts into `MigrateToGatewayParams` by injecting the provided diamond cut.
    pub(crate) fn into_migrate_params(
        self,
        gateway_diamond_cut: Vec<u8>,
    ) -> MigrateToGatewayParams {
        MigrateToGatewayParams {
            l1_rpc_url: self.l1_rpc_url,
            l1_bridgehub_addr: self.l1_bridgehub_addr,
            max_l1_gas_price: self.max_l1_gas_price,
            l2_chain_id: self.l2_chain_id,
            gateway_chain_id: self.gateway_chain_id,
            gateway_diamond_cut,
            gateway_rpc_url: self.gateway_rpc_url,
            new_sl_da_validator: self.new_sl_da_validator,
            validator_1: self.validator_1,
            validator_2: self.validator_2,
            min_validator_balance: self.min_validator_balance,
            refund_recipient: self.refund_recipient,
        }
    }
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(shell: &Shell, params: MigrateToGatewayCalldataArgs) -> anyhow::Result<()> {
    let forge_args = Default::default();
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
            GatewayMigrationProgressState::ServerReady => {
                logger::info(
                    "The server is ready to start the migration. Preparing the calldata...",
                );
                // It is the expected case, it will be handled later in the file
            }
            GatewayMigrationProgressState::PendingManualFinalization => {
                unreachable!("`GatewayMigrationProgressState::PendingManualFinalization` should not be returned for migration to Gateway")
            }
            _ => {
                let msg = message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                );
                logger::info(&msg);
            }
        }
    }

    let gateway_config = GatewayConfig::read(shell, &params.gateway_config_path)
        .context("Failed to read the gateway config path")?;

    let (admin_address, calls) = get_migrate_to_gateway_calls(
        shell,
        &forge_args,
        &contracts_foundry_path,
        params.into_migrate_params(gateway_config.diamond_cut_data.0),
    )
    .await?;

    display_admin_script_output(AdminScriptOutput {
        admin_address,
        calls,
    });

    Ok(())
}
