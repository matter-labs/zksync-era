//! This module provides primitives focusing on the VM instantiation and execution for different use cases.
//! It is rather generic and low-level, so it's not supposed to be a part of public API.
//!
//! Instead, we expect people to write wrappers in the `execution_sandbox` module with a more high-level API
//! that would, in its turn, be used by the actual API method handlers.
//!
//! This module is intended to be blocking.

use std::time::{Duration, Instant};

use anyhow::Context as _;
use tokio::runtime::Handle;
use zksync_dal::{Connection, Core, CoreDal, DalError};
use zksync_multivm::{
    interface::{L1BatchEnv, L2BlockEnv, OneshotEnv, StoredL2BlockEnv, SystemEnv},
    utils::get_eth_call_gas_limit,
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_state::PostgresStorage;
use zksync_system_constants::{
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, ZKPORTER_IS_AVAILABLE,
};
use zksync_types::{
    api,
    block::{unpack_block_info, L2BlockHasher},
    fee_model::BatchFeeInput,
    AccountTreeId, L1BatchNumber, L2BlockNumber, ProtocolVersionId, StorageKey, H256, U256,
};
use zksync_utils::{h256_to_u256, time::seconds_since_epoch};

use super::{
    vm_metrics::{SandboxStage, SANDBOX_METRICS},
    BlockArgs, TxSetupArgs,
};

pub(super) async fn prepare_env_and_storage(
    mut connection: Connection<'static, Core>,
    setup_args: TxSetupArgs,
    block_args: &BlockArgs,
) -> anyhow::Result<(OneshotEnv, PostgresStorage<'static>)> {
    let initialization_stage = SANDBOX_METRICS.sandbox[&SandboxStage::Initialization].start();

    let resolve_started_at = Instant::now();
    let resolved_block_info = block_args
        .resolve_block_info(&mut connection)
        .await
        .with_context(|| format!("cannot resolve block numbers for {block_args:?}"))?;
    let resolve_time = resolve_started_at.elapsed();
    // We don't want to emit too many logs.
    if resolve_time > Duration::from_millis(10) {
        tracing::debug!("Resolved block numbers (took {resolve_time:?})");
    }

    if block_args.resolves_to_latest_sealed_l2_block() {
        setup_args
            .caches
            .schedule_values_update(resolved_block_info.state_l2_block_number);
    }

    let (next_block, current_block) = load_l2_block_info(
        &mut connection,
        block_args.is_pending_l2_block(),
        &resolved_block_info,
    )
    .await?;

    let storage = PostgresStorage::new_async(
        Handle::current(),
        connection,
        resolved_block_info.state_l2_block_number,
        false,
    )
    .await
    .context("cannot create `PostgresStorage`")?
    .with_caches(setup_args.caches.clone());

    let (system, l1_batch) = prepare_env(setup_args, &resolved_block_info, next_block);

    let env = OneshotEnv {
        system,
        l1_batch,
        current_block,
    };
    initialization_stage.observe();
    Ok((env, storage))
}

async fn load_l2_block_info(
    connection: &mut Connection<'_, Core>,
    is_pending_block: bool,
    resolved_block_info: &ResolvedBlockInfo,
) -> anyhow::Result<(L2BlockEnv, Option<StoredL2BlockEnv>)> {
    let mut current_block = None;
    let next_block = read_stored_l2_block(connection, resolved_block_info.state_l2_block_number)
        .await
        .context("failed reading L2 block info")?;

    let next_block = if is_pending_block {
        L2BlockEnv {
            number: next_block.number + 1,
            timestamp: resolved_block_info.l1_batch_timestamp,
            prev_block_hash: resolved_block_info.state_l2_block_hash,
            // For simplicity, we assume each L2 block create one virtual block.
            // This may be wrong only during transition period.
            max_virtual_blocks_to_create: 1,
        }
    } else if next_block.number == 0 {
        // Special case:
        // - For environments, where genesis block was created before virtual block upgrade it doesn't matter what we put here.
        // - Otherwise, we need to put actual values here. We cannot create next L2 block with block_number=0 and `max_virtual_blocks_to_create=0`
        //   because of SystemContext requirements. But, due to intrinsics of SystemContext, block.number still will be resolved to 0.
        L2BlockEnv {
            number: 1,
            timestamp: 0,
            prev_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
            max_virtual_blocks_to_create: 1,
        }
    } else {
        // We need to reset L2 block info in storage to process transaction in the current block context.
        // Actual resetting will be done after `storage_view` is created.
        let prev_block_number = resolved_block_info.state_l2_block_number - 1;
        let prev_l2_block = read_stored_l2_block(connection, prev_block_number)
            .await
            .context("failed reading previous L2 block info")?;

        let mut prev_block_hash = connection
            .blocks_web3_dal()
            .get_l2_block_hash(prev_block_number)
            .await
            .map_err(DalError::generalize)?;
        if prev_block_hash.is_none() {
            // We might need to load the previous block hash from the snapshot recovery metadata
            let snapshot_recovery = connection
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await
                .map_err(DalError::generalize)?;
            prev_block_hash = snapshot_recovery.and_then(|recovery| {
                (recovery.l2_block_number == prev_block_number).then_some(recovery.l2_block_hash)
            });
        }

        current_block = Some(prev_l2_block);
        L2BlockEnv {
            number: next_block.number,
            timestamp: next_block.timestamp,
            prev_block_hash: prev_block_hash.with_context(|| {
                format!("missing hash for previous L2 block #{prev_block_number}")
            })?,
            max_virtual_blocks_to_create: 1,
        }
    };

    Ok((next_block, current_block))
}

fn prepare_env(
    setup_args: TxSetupArgs,
    resolved_block_info: &ResolvedBlockInfo,
    next_block: L2BlockEnv,
) -> (SystemEnv, L1BatchEnv) {
    let TxSetupArgs {
        execution_mode,
        operator_account,
        fee_input,
        base_system_contracts,
        validation_computational_gas_limit,
        chain_id,
        enforced_base_fee,
        ..
    } = setup_args;

    // In case we are executing in a past block, we'll use the historical fee data.
    let fee_input = resolved_block_info
        .historical_fee_input
        .unwrap_or(fee_input);
    let system_env = SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: resolved_block_info.protocol_version,
        base_system_smart_contracts: base_system_contracts
            .get_by_protocol_version(resolved_block_info.protocol_version),
        bootloader_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
        execution_mode,
        default_validation_computational_gas_limit: validation_computational_gas_limit,
        chain_id,
    };
    let l1_batch_env = L1BatchEnv {
        previous_batch_hash: None,
        number: resolved_block_info.vm_l1_batch_number,
        timestamp: resolved_block_info.l1_batch_timestamp,
        fee_input,
        fee_account: *operator_account.address(),
        enforced_base_fee,
        first_l2_block: next_block,
    };
    (system_env, l1_batch_env)
}

async fn read_stored_l2_block(
    connection: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
) -> anyhow::Result<StoredL2BlockEnv> {
    let l2_block_info_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    );
    let l2_block_info = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(l2_block_info_key.hashed_key(), l2_block_number)
        .await?;
    let (l2_block_number_from_state, timestamp) = unpack_block_info(h256_to_u256(l2_block_info));

    let l2_block_txs_rolling_hash_key = StorageKey::new(
        AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
        SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
    );
    let txs_rolling_hash = connection
        .storage_web3_dal()
        .get_historical_value_unchecked(l2_block_txs_rolling_hash_key.hashed_key(), l2_block_number)
        .await?;

    Ok(StoredL2BlockEnv {
        number: l2_block_number_from_state as u32,
        timestamp,
        txs_rolling_hash,
    })
}

#[derive(Debug)]
pub(crate) struct ResolvedBlockInfo {
    state_l2_block_number: L2BlockNumber,
    state_l2_block_hash: H256,
    vm_l1_batch_number: L1BatchNumber,
    l1_batch_timestamp: u64,
    pub(crate) protocol_version: ProtocolVersionId,
    historical_fee_input: Option<BatchFeeInput>,
}

impl BlockArgs {
    fn is_pending_l2_block(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
        )
    }

    fn is_estimate_like(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
                | api::BlockId::Number(api::BlockNumber::Latest)
                | api::BlockId::Number(api::BlockNumber::Committed)
        )
    }

    pub(crate) async fn default_eth_call_gas(
        &self,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<U256> {
        let protocol_version = self
            .resolve_block_info(connection)
            .await
            .context("failed to resolve block info")?
            .protocol_version;
        Ok(get_eth_call_gas_limit(protocol_version.into()).into())
    }

    async fn resolve_block_info(
        &self,
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<ResolvedBlockInfo> {
        let (state_l2_block_number, vm_l1_batch_number, l1_batch_timestamp);

        let l2_block_header = if self.is_pending_l2_block() {
            vm_l1_batch_number = connection
                .blocks_dal()
                .get_sealed_l1_batch_number()
                .await?
                .context("no L1 batches in storage")?;
            let sealed_l2_block_header = connection
                .blocks_dal()
                .get_last_sealed_l2_block_header()
                .await?
                .context("no L2 blocks in storage")?;

            state_l2_block_number = sealed_l2_block_header.number;
            // Timestamp of the next L1 batch must be greater than the timestamp of the last L2 block.
            l1_batch_timestamp = seconds_since_epoch().max(sealed_l2_block_header.timestamp + 1);
            sealed_l2_block_header
        } else {
            vm_l1_batch_number = connection
                .storage_web3_dal()
                .resolve_l1_batch_number_of_l2_block(self.resolved_block_number)
                .await
                .context("failed resolving L1 batch for L2 block")?
                .expected_l1_batch();
            l1_batch_timestamp = self
                .l1_batch_timestamp_s
                .context("L1 batch timestamp is `None` for non-pending block args")?;
            state_l2_block_number = self.resolved_block_number;

            connection
                .blocks_dal()
                .get_l2_block_header(self.resolved_block_number)
                .await?
                .context("resolved L2 block disappeared from storage")?
        };

        let historical_fee_input = if !self.is_estimate_like() {
            let l2_block_header = connection
                .blocks_dal()
                .get_l2_block_header(self.resolved_block_number)
                .await?
                .context("resolved L2 block is not in storage")?;
            Some(l2_block_header.batch_fee_input)
        } else {
            None
        };

        // Blocks without version specified are considered to be of `Version9`.
        // TODO: remove `unwrap_or` when protocol version ID will be assigned for each block.
        let protocol_version = l2_block_header
            .protocol_version
            .unwrap_or(ProtocolVersionId::last_potentially_undefined());

        Ok(ResolvedBlockInfo {
            state_l2_block_number,
            state_l2_block_hash: l2_block_header.hash,
            vm_l1_batch_number,
            l1_batch_timestamp,
            protocol_version,
            historical_fee_input,
        })
    }
}
