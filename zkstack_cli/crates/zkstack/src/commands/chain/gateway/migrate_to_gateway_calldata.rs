use std::{path::Path, sync::Arc};

use anyhow::Context;
use clap::Parser;
use ethers::{
    prelude::Http,
    providers::{Middleware, Provider},
};
use xshell::Shell;
use zkstack_cli_common::{ethereum::get_ethers_provider, forge::ForgeScriptArgs, logger};
use zkstack_cli_config::{traits::ReadConfig, GatewayConfig, ZkStackConfig, ZkStackConfigTrait};
use zksync_basic_types::{commitment::L2DACommitmentScheme, Address, H256, U256};
use zksync_system_constants::L2_BRIDGEHUB_ADDRESS;

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
        set_da_validator_pair_via_gateway, AdminScriptMode, AdminScriptOutput,
    },
    commands::chain::{admin_call_builder::AdminCall, utils::display_admin_script_output},
    utils::{
        addresses::{apply_l1_to_l2_alias, precompute_chain_address_on_gateway},
        protocol_version::get_minor_protocol_version,
    },
};

#[derive(Debug)]
pub(crate) struct MigrateToGatewayConfig {
    pub(crate) l1_rpc_url: String,
    pub(crate) l1_bridgehub_addr: Address,
    pub(crate) max_l1_gas_price: u64,
    pub(crate) l2_chain_id: u64,
    pub(crate) gateway_chain_id: u64,
    pub(crate) gateway_diamond_cut: Vec<u8>,
    pub(crate) gateway_rpc_url: String,
    pub(crate) new_sl_da_validator: Address,
    pub(crate) validator: Address,
    pub(crate) min_validator_balance: U256,
    pub(crate) refund_recipient: Option<Address>,
}

#[derive(Debug)]
pub(crate) struct MigrateToGatewayContext {
    pub(crate) l1_rpc_url: String,
    pub(crate) l1_bridgehub_addr: Address,
    pub(crate) max_l1_gas_price: u64,
    pub(crate) l2_chain_id: u64,
    pub(crate) gateway_chain_id: u64,
    pub(crate) gateway_diamond_cut: Vec<u8>,
    pub(crate) gateway_rpc_url: String,
    pub(crate) new_sl_da_validator: Address,
    pub(crate) validator: Address,
    pub(crate) min_validator_balance: U256,
    pub(crate) l1_provider: Arc<Provider<Http>>,
    pub(crate) gw_provider: Arc<Provider<Http>>,
    pub(crate) zk_chain_l1_address: Address,
    pub(crate) l1_zk_chain: ZkChainAbi<Provider<Http>>,
    pub(crate) protocol_version: U256,
    pub(crate) gw_validator_timelock_addr: Address,
    pub(crate) gw_validator_timelock: ValidatorTimelockAbi<Provider<Http>>,
    pub(crate) chain_admin_address: Address,
    pub(crate) zk_chain_gw_address: Address,
    pub(crate) refund_recipient: Address,
}

impl MigrateToGatewayConfig {
    pub async fn into_context(self) -> anyhow::Result<MigrateToGatewayContext> {
        self.into_context_inner(false).await
    }

    pub async fn into_context_unchecked(self) -> anyhow::Result<MigrateToGatewayContext> {
        self.into_context_inner(true).await
    }

    async fn into_context_inner(
        self,
        skip_pre_migration_checks: bool,
    ) -> anyhow::Result<MigrateToGatewayContext> {
        let refund_recipient = self.refund_recipient.unwrap_or(self.validator);

        let l1_provider = get_ethers_provider(&self.l1_rpc_url)?;
        let gw_provider = get_ethers_provider(&self.gateway_rpc_url)?;

        let l1_bridgehub = BridgehubAbi::new(self.l1_bridgehub_addr, l1_provider.clone());
        let gw_bridgehub = BridgehubAbi::new(L2_BRIDGEHUB_ADDRESS, gw_provider.clone());

        let current_settlement_layer = l1_bridgehub
            .settlement_layer(self.l2_chain_id.into())
            .await?;

        let zk_chain_l1_address = l1_bridgehub.get_zk_chain(self.l2_chain_id.into()).await?;

        if zk_chain_l1_address == Address::zero() {
            anyhow::bail!("Chain with id {} does not exist!", self.l2_chain_id);
        }

        // Checking whether the user has already done the migration
        if current_settlement_layer == U256::from(self.gateway_chain_id)
            && !skip_pre_migration_checks
        {
            // TODO(EVM-1001): it may happen that the user has started the migration, but it failed for some reason (e.g. the provided
            // diamond cut was not correct).
            // The recovery of the chain is not handled by the tool right now.
            anyhow::bail!("The chain is already on top of Gateway!");
        }

        let ctm_asset_id = l1_bridgehub
            .ctm_asset_id_from_chain_id(self.l2_chain_id.into())
            .await?;
        let ctm_gw_address = gw_bridgehub.ctm_asset_id_to_address(ctm_asset_id).await?;

        if ctm_gw_address == Address::zero() {
            anyhow::bail!("{} does not have a CTM deployed!", self.gateway_chain_id);
        }

        let l1_zk_chain = ZkChainAbi::new(zk_chain_l1_address, l1_provider.clone());
        let protocol_version = l1_zk_chain.get_protocol_version().await?;

        // Checking that the priority queue is empty
        let priority_queue_size = l1_zk_chain.get_priority_queue_size().await?;
        if !priority_queue_size.is_zero() && !skip_pre_migration_checks {
            anyhow::bail!(
                "{} priority queue has {} items! Please empty it before migrating to Gateway",
                self.l2_chain_id,
                priority_queue_size
            );
        }

        let gw_ctm = ChainTypeManagerAbi::new(ctm_gw_address, gw_provider.clone());
        let gw_ctm_protocol_version = gw_ctm.protocol_version().await?;
        if gw_ctm_protocol_version != protocol_version {
            // The migration would fail anyway since CTM has checks to ensure that the protocol version is the same
            anyhow::bail!("The protocol version of the CTM on Gateway ({gw_ctm_protocol_version}) does not match the protocol version of the chain ({protocol_version})");
        }

        let gw_validator_timelock_addr =
            if get_minor_protocol_version(protocol_version)?.is_pre_interop_fast_blocks() {
                gw_ctm.validator_timelock().await?
            } else {
                gw_ctm.validator_timelock_post_v29().await?
            };
        let gw_validator_timelock =
            ValidatorTimelockAbi::new(gw_validator_timelock_addr, gw_provider.clone());

        let chain_admin_address = l1_zk_chain.get_admin().await?;
        let zk_chain_gw_address = {
            let recorded_zk_chain_gw_address =
                gw_bridgehub.get_zk_chain(self.l2_chain_id.into()).await?;
            if recorded_zk_chain_gw_address == Address::zero() {
                precompute_chain_address_on_gateway(
                    self.l2_chain_id,
                    H256(
                        l1_bridgehub
                            .base_token_asset_id(self.l2_chain_id.into())
                            .await?,
                    ),
                    apply_l1_to_l2_alias(l1_zk_chain.get_admin().await?),
                    protocol_version,
                    self.gateway_diamond_cut.clone(),
                    gw_ctm,
                )
                .await?
            } else {
                recorded_zk_chain_gw_address
            }
        };

        Ok(MigrateToGatewayContext {
            l1_rpc_url: self.l1_rpc_url,
            l1_bridgehub_addr: self.l1_bridgehub_addr,
            max_l1_gas_price: self.max_l1_gas_price,
            l2_chain_id: self.l2_chain_id,
            gateway_chain_id: self.gateway_chain_id,
            gateway_diamond_cut: self.gateway_diamond_cut,
            gateway_rpc_url: self.gateway_rpc_url,
            new_sl_da_validator: self.new_sl_da_validator,
            validator: self.validator,
            min_validator_balance: self.min_validator_balance,
            l1_provider,
            gw_provider,
            zk_chain_l1_address,
            l1_zk_chain,
            protocol_version,
            gw_validator_timelock_addr,
            gw_validator_timelock,
            chain_admin_address,
            zk_chain_gw_address,
            refund_recipient,
        })
    }
}

pub(crate) async fn get_migrate_to_gateway_calls(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    context: &MigrateToGatewayContext,
) -> anyhow::Result<(Address, Vec<AdminCall>)> {
    let mut result = vec![];

    let finalize_migrate_to_gateway_output = finalize_migrate_to_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        crate::admin_functions::AdminScriptMode::OnlySave,
        context.l1_bridgehub_addr,
        context.max_l1_gas_price,
        context.l2_chain_id,
        context.gateway_chain_id,
        context.gateway_diamond_cut.clone().into(),
        context.refund_recipient,
        context.l1_rpc_url.clone(),
    )
    .await?;
    result.extend(finalize_migrate_to_gateway_output.calls);

    // Changing L2 DA validator while migrating to gateway is not recommended; we allow changing only the settlement layer one
    let (_, l2_da_validator_commitment_scheme) =
        context.l1_zk_chain.get_da_validator_pair().await?;
    let l2_da_validator_commitment_scheme =
        L2DACommitmentScheme::try_from(l2_da_validator_commitment_scheme)
            .map_err(|err| anyhow::format_err!("Failed to parse L2 DA commitment schema: {err}"))?;
    if !l2_da_validator_commitment_scheme.is_none() {
        let da_validator_encoding_result = check_permanent_rollup_and_set_da_validator_via_gateway(
            shell,
            forge_args,
            foundry_contracts_path,
            context,
            l2_da_validator_commitment_scheme,
            crate::admin_functions::AdminScriptMode::OnlySave,
        )
        .await?;
        result.extend(da_validator_encoding_result.calls.into_iter());
    }

    let is_validator_enabled =
        if get_minor_protocol_version(context.protocol_version)?.is_pre_interop_fast_blocks() {
            // In previous versions, we need to check if the validator is enabled
            context
                .gw_validator_timelock
                .validators(context.l2_chain_id.into(), context.validator)
                .await?
        } else {
            context
                .gw_validator_timelock
                .has_role_for_chain_id(
                    context.l2_chain_id.into(),
                    context
                        .gw_validator_timelock
                        .committer_role()
                        .call()
                        .await?,
                    context.validator,
                )
                .await?
        };

    // 4. If validator is not yet present, please include.
    if !is_validator_enabled {
        let enable_validator_calls = enable_validator_via_gateway(
            shell,
            forge_args,
            foundry_contracts_path,
            crate::admin_functions::AdminScriptMode::OnlySave,
            context.l1_bridgehub_addr,
            context.max_l1_gas_price.into(),
            context.l2_chain_id,
            context.gateway_chain_id,
            context.validator,
            context.gw_validator_timelock_addr,
            context.refund_recipient,
            context.l1_rpc_url.clone(),
        )
        .await?;
        result.extend(enable_validator_calls.calls);
    }

    let current_validator_balance = context
        .gw_provider
        .get_balance(context.validator, None)
        .await?;
    logger::info(format!(
        "Current balance of {:#?} = {}",
        context.validator, current_validator_balance
    ));
    if current_validator_balance < context.min_validator_balance {
        logger::info(format!(
            "Will send {} of the ZK Gateway base token",
            context.min_validator_balance - current_validator_balance
        ));
        let supply_validator_balance_calls = admin_l1_l2_tx(
            shell,
            forge_args,
            foundry_contracts_path,
            crate::admin_functions::AdminScriptMode::OnlySave,
            context.l1_bridgehub_addr,
            context.max_l1_gas_price,
            context.gateway_chain_id,
            context.validator,
            context.min_validator_balance - current_validator_balance,
            Default::default(),
            context.refund_recipient,
            context.l1_rpc_url.clone(),
        )
        .await?;
        result.extend(supply_validator_balance_calls.calls);
    }

    Ok((context.chain_admin_address, result))
}

pub(crate) async fn check_permanent_rollup_and_set_da_validator_via_gateway(
    shell: &Shell,
    forge_args: &ForgeScriptArgs,
    foundry_contracts_path: &Path,
    context: &MigrateToGatewayContext,
    l2_da_validator_commitment_scheme: L2DACommitmentScheme,
    mode: AdminScriptMode,
) -> anyhow::Result<AdminScriptOutput> {
    // Unfortunately, there is no getter for whether a chain is a permanent rollup, we have to
    // read storage here.
    let is_permanent_rollup_slot = context
        .l1_provider
        .get_storage_at(context.zk_chain_l1_address, H256::from_low_u64_be(57), None)
        .await?;
    if is_permanent_rollup_slot == H256::from_low_u64_be(1) {
        // TODO(EVM-1002): We should really check it on our own here, but it is hard with the current interfaces
        logger::warn("WARNING: Your chain is a permanent rollup! Ensure that the new settlement layer DA provider is compatible with Gateway RollupDAManager!");
    }

    let da_validator_encoding_result = set_da_validator_pair_via_gateway(
        shell,
        forge_args,
        foundry_contracts_path,
        mode,
        context.l1_bridgehub_addr,
        context.max_l1_gas_price.into(),
        context.l2_chain_id,
        context.gateway_chain_id,
        context.new_sl_da_validator,
        l2_da_validator_commitment_scheme,
        context.zk_chain_gw_address,
        context.refund_recipient,
        context.l1_rpc_url.clone(),
    )
    .await?;

    Ok(da_validator_encoding_result)
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
    pub validator: Address,
    #[clap(long)]
    pub min_validator_balance: u128,
    #[clap(long)]
    pub refund_recipient: Option<Address>,

    /// RPC URL of the chain being migrated (L2).
    #[clap(long)]
    pub l2_rpc_url: Option<String>,

    /// Whether to force providing the full migration calldata even if the chain
    /// isn't strictly ready for final calls.
    #[clap(long, default_missing_value = "true")]
    pub no_cross_check: Option<bool>,
}

impl MigrateToGatewayCalldataArgs {
    /// Converts into `MigrateToGatewayConfig` by injecting the provided diamond cut.
    pub(crate) fn into_config(self, gateway_diamond_cut: Vec<u8>) -> MigrateToGatewayConfig {
        MigrateToGatewayConfig {
            l1_rpc_url: self.l1_rpc_url,
            l1_bridgehub_addr: self.l1_bridgehub_addr,
            max_l1_gas_price: self.max_l1_gas_price,
            l2_chain_id: self.l2_chain_id,
            gateway_chain_id: self.gateway_chain_id,
            gateway_diamond_cut,
            gateway_rpc_url: self.gateway_rpc_url,
            new_sl_da_validator: self.new_sl_da_validator,
            validator: self.validator,
            min_validator_balance: self.min_validator_balance.into(),
            refund_recipient: self.refund_recipient,
        }
    }
}

/// Produces the calldata necessary to perform (or continue) a migration to Gateway.
///
pub async fn run(shell: &Shell, params: MigrateToGatewayCalldataArgs) -> anyhow::Result<()> {
    let forge_args = Default::default();
    let contracts_foundry_path = ZkStackConfig::from_file(shell)?.path_to_foundry_scripts();

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
            GatewayMigrationProgressState::PendingManualFinalization
            | GatewayMigrationProgressState::AwaitingFinalization => {
                unreachable!("`GatewayMigrationProgressState::PendingManualFinalization` should not be returned for migration to Gateway")
            }
            GatewayMigrationProgressState::NotStarted
            | GatewayMigrationProgressState::NotificationSent
            | GatewayMigrationProgressState::NotificationReceived(_) => {
                anyhow::bail!(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                ));
            }
            GatewayMigrationProgressState::Finished => {
                logger::info(message_for_gateway_migration_progress_state(
                    state,
                    MigrationDirection::ToGateway,
                ));
            }
        }
    }

    let gateway_config = GatewayConfig::read(shell, &params.gateway_config_path)
        .context("Failed to read the gateway config path")?;

    let config = params.into_config(gateway_config.diamond_cut_data.0);
    let context = config.into_context().await?;
    let (admin_address, calls) =
        get_migrate_to_gateway_calls(shell, &forge_args, &contracts_foundry_path, &context).await?;

    display_admin_script_output(AdminScriptOutput {
        admin_address,
        calls,
    });

    Ok(())
}
