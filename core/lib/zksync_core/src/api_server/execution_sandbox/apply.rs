//! This module provides primitives focusing on the VM instantiation and execution for different use cases.
//! It is rather generic and low-level, so it's not supposed to be a part of public API.
//!
//! Instead, we expect people to write wrappers in the `execution_sandbox` module with a more high-level API
//! that would, in its turn, be used by the actual API method handlers.
//!
//! This module is intended to be blocking.

use std::time::{Duration, Instant};

use anyhow::Context as _;
use multivm::{
    interface::{L1BatchEnv, L2BlockEnv, SystemEnv, VmInterface},
    utils::adjust_pubdata_price_for_tx,
    vm_latest::{constants::BLOCK_GAS_LIMIT, HistoryDisabled},
    VmInstance,
};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_state::{PostgresStorage, ReadStorage, StorageView, WriteStorage};
use zksync_system_constants::{
    SYSTEM_CONTEXT_ADDRESS, SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
    SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION, ZKPORTER_IS_AVAILABLE,
};
use zksync_types::{
    api,
    block::{pack_block_info, unpack_block_info, MiniblockHasher},
    fee_model::BatchFeeInput,
    get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    AccountTreeId, L1BatchNumber, MiniblockNumber, Nonce, ProtocolVersionId, StorageKey,
    Transaction, H256, U256,
};
use zksync_utils::{h256_to_u256, time::seconds_since_epoch, u256_to_h256};

use super::{
    vm_metrics::{self, SandboxStage, SANDBOX_METRICS},
    BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit,
};
use crate::utils::projected_first_l1_batch;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_vm_in_sandbox<T>(
    vm_permit: VmPermit,
    shared_args: TxSharedArgs,
    // If `true`, then the batch's L1/pubdata gas price will be adjusted so that the transaction's gas per pubdata limit is <=
    // to the one in the block. This is often helpful in case we want the transaction validation to work regardless of the
    // current L1 prices for gas or pubdata.
    adjust_pubdata_price: bool,
    execution_args: &TxExecutionArgs,
    connection_pool: &ConnectionPool,
    tx: Transaction,
    block_args: BlockArgs,
    apply: impl FnOnce(
        &mut VmInstance<StorageView<PostgresStorage<'_>>, HistoryDisabled>,
        Transaction,
    ) -> T,
) -> anyhow::Result<T> {
    let stage_started_at = Instant::now();
    let span = tracing::debug_span!("initialization").entered();

    let rt_handle = vm_permit.rt_handle();
    let mut connection = rt_handle
        .block_on(connection_pool.access_storage_tagged("api"))
        .context("failed acquiring DB connection")?;
    let connection_acquire_time = stage_started_at.elapsed();
    // We don't want to emit too many logs.
    if connection_acquire_time > Duration::from_millis(10) {
        tracing::debug!(
            "Obtained connection (took {:?})",
            stage_started_at.elapsed()
        );
    }

    let resolve_started_at = Instant::now();
    let ResolvedBlockInfo {
        state_l2_block_number,
        vm_l1_batch_number,
        l1_batch_timestamp,
        protocol_version,
        historical_fee_input,
    } = rt_handle
        .block_on(block_args.resolve_block_info(&mut connection))
        .with_context(|| format!("failed resolving block numbers for {block_args:?}"))?;
    let resolve_time = resolve_started_at.elapsed();
    // We don't want to emit too many logs.
    if resolve_time > Duration::from_millis(10) {
        tracing::debug!(
            "Resolved block numbers (took {:?})",
            resolve_started_at.elapsed()
        );
    }

    if block_args.resolves_to_latest_sealed_miniblock() {
        shared_args
            .caches
            .schedule_values_update(state_l2_block_number);
    }

    let mut l2_block_info_to_reset = None;
    let current_l2_block_info = rt_handle
        .block_on(StoredL2BlockInfo::new(
            &mut connection,
            state_l2_block_number,
        ))
        .context("failed reading L2 block info")?;
    let next_l2_block_info = if block_args.is_pending_miniblock() {
        L2BlockEnv {
            number: current_l2_block_info.l2_block_number + 1,
            timestamp: l1_batch_timestamp,
            prev_block_hash: current_l2_block_info.l2_block_hash,
            // For simplicity we assume each miniblock create one virtual block.
            // This may be wrong only during transition period.
            max_virtual_blocks_to_create: 1,
        }
    } else if current_l2_block_info.l2_block_number == 0 {
        // Special case:
        // - For environments, where genesis block was created before virtual block upgrade it doesn't matter what we put here.
        // - Otherwise, we need to put actual values here. We cannot create next L2 block with block_number=0 and `max_virtual_blocks_to_create=0`
        //   because of SystemContext requirements. But, due to intrinsics of SystemContext, block.number still will be resolved to 0.
        L2BlockEnv {
            number: 1,
            timestamp: 0,
            prev_block_hash: MiniblockHasher::legacy_hash(MiniblockNumber(0)),
            max_virtual_blocks_to_create: 1,
        }
    } else {
        // We need to reset L2 block info in storage to process transaction in the current block context.
        // Actual resetting will be done after `storage_view` is created.
        let prev_l2_block_info = rt_handle
            .block_on(StoredL2BlockInfo::new(
                &mut connection,
                state_l2_block_number - 1,
            ))
            .context("failed reading previous L2 block info")?;

        l2_block_info_to_reset = Some(prev_l2_block_info);
        L2BlockEnv {
            number: current_l2_block_info.l2_block_number,
            timestamp: current_l2_block_info.l2_block_timestamp,
            prev_block_hash: prev_l2_block_info.l2_block_hash,
            max_virtual_blocks_to_create: 1,
        }
    };

    let storage = PostgresStorage::new(rt_handle.clone(), connection, state_l2_block_number, false)
        .with_caches(shared_args.caches);
    let mut storage_view = StorageView::new(storage);

    let storage_view_setup_started_at = Instant::now();
    if let Some(nonce) = execution_args.enforced_nonce {
        let nonce_key = get_nonce_key(&tx.initiator_account());
        let full_nonce = storage_view.read_value(&nonce_key);
        let (_, deployment_nonce) = decompose_full_nonce(h256_to_u256(full_nonce));
        let enforced_full_nonce = nonces_to_full_nonce(U256::from(nonce.0), deployment_nonce);
        storage_view.set_value(nonce_key, u256_to_h256(enforced_full_nonce));
    }

    let payer = tx.payer();
    let balance_key = storage_key_for_eth_balance(&payer);
    let mut current_balance = h256_to_u256(storage_view.read_value(&balance_key));
    current_balance += execution_args.added_balance;
    storage_view.set_value(balance_key, u256_to_h256(current_balance));

    // Reset L2 block info.
    if let Some(l2_block_info_to_reset) = l2_block_info_to_reset {
        let l2_block_info_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
        );
        let l2_block_info = pack_block_info(
            l2_block_info_to_reset.l2_block_number as u64,
            l2_block_info_to_reset.l2_block_timestamp,
        );
        storage_view.set_value(l2_block_info_key, u256_to_h256(l2_block_info));

        let l2_block_txs_rolling_hash_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
        );
        storage_view.set_value(
            l2_block_txs_rolling_hash_key,
            l2_block_info_to_reset.txs_rolling_hash,
        );
    }

    let storage_view_setup_time = storage_view_setup_started_at.elapsed();
    // We don't want to emit too many logs.
    if storage_view_setup_time > Duration::from_millis(10) {
        tracing::debug!("Prepared the storage view (took {storage_view_setup_time:?})",);
    }

    let TxSharedArgs {
        operator_account,
        fee_input,
        base_system_contracts,
        validation_computational_gas_limit,
        chain_id,
        ..
    } = shared_args;

    // In case we are executing in a past block, we'll
    // use the historical fee data.
    let fee_input = historical_fee_input.unwrap_or(fee_input);
    let fee_input = if adjust_pubdata_price {
        adjust_pubdata_price_for_tx(
            fee_input,
            tx.gas_per_pubdata_byte_limit(),
            protocol_version.into(),
        )
    } else {
        fee_input
    };

    let system_env = SystemEnv {
        zk_porter_available: ZKPORTER_IS_AVAILABLE,
        version: protocol_version,
        base_system_smart_contracts: base_system_contracts
            .get_by_protocol_version(protocol_version),
        gas_limit: BLOCK_GAS_LIMIT,
        execution_mode: execution_args.execution_mode,
        default_validation_computational_gas_limit: validation_computational_gas_limit,
        chain_id,
    };
    let l1_batch_env = L1BatchEnv {
        previous_batch_hash: None,
        number: vm_l1_batch_number,
        timestamp: l1_batch_timestamp,
        fee_input,
        fee_account: *operator_account.address(),
        enforced_base_fee: execution_args.enforced_base_fee,
        first_l2_block: next_l2_block_info,
    };

    let storage_view = storage_view.to_rc_ptr();
    let mut vm = Box::new(VmInstance::new_with_specific_version(
        l1_batch_env,
        system_env,
        storage_view.clone(),
        protocol_version.into_api_vm_version(),
    ));

    SANDBOX_METRICS.sandbox[&SandboxStage::Initialization].observe(stage_started_at.elapsed());
    span.exit();

    let tx_id = format!(
        "{:?}-{}",
        tx.initiator_account(),
        tx.nonce().unwrap_or(Nonce(0))
    );
    let execution_latency = SANDBOX_METRICS.sandbox[&SandboxStage::Execution].start();
    let result = apply(&mut vm, tx);
    let vm_execution_took = execution_latency.observe();

    let memory_metrics = vm.record_vm_memory_metrics();
    vm_metrics::report_vm_memory_metrics(
        &tx_id,
        &memory_metrics,
        vm_execution_took,
        storage_view.as_ref().borrow_mut().metrics(),
    );
    drop(vm_permit); // Ensure that the permit lives until this point

    Ok(result)
}

#[derive(Debug, Clone, Copy)]
struct StoredL2BlockInfo {
    l2_block_number: u32,
    l2_block_timestamp: u64,
    l2_block_hash: H256,
    txs_rolling_hash: H256,
}

impl StoredL2BlockInfo {
    async fn new(
        connection: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
    ) -> anyhow::Result<Self> {
        let l2_block_info_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_L2_BLOCK_INFO_POSITION,
        );
        let l2_block_info = connection
            .storage_web3_dal()
            .get_historical_value_unchecked(&l2_block_info_key, miniblock_number)
            .await
            .context("failed reading L2 block info from VM state")?;
        let (l2_block_number, l2_block_timestamp) = unpack_block_info(h256_to_u256(l2_block_info));

        let l2_block_txs_rolling_hash_key = StorageKey::new(
            AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
            SYSTEM_CONTEXT_CURRENT_TX_ROLLING_HASH_POSITION,
        );
        let txs_rolling_hash = connection
            .storage_web3_dal()
            .get_historical_value_unchecked(&l2_block_txs_rolling_hash_key, miniblock_number)
            .await
            .context("failed reading transaction rolling hash from VM state")?;

        let l2_block_hash = connection
            .blocks_web3_dal()
            .get_miniblock_hash(miniblock_number)
            .await
            .with_context(|| format!("failed getting hash for miniblock #{miniblock_number}"))?
            .with_context(|| format!("miniblock #{miniblock_number} disappeared from Postgres"))?;

        Ok(Self {
            l2_block_number: l2_block_number as u32,
            l2_block_timestamp,
            l2_block_hash,
            txs_rolling_hash,
        })
    }
}

#[derive(Debug)]
pub(crate) struct ResolvedBlockInfo {
    pub state_l2_block_number: MiniblockNumber,
    pub vm_l1_batch_number: L1BatchNumber,
    pub l1_batch_timestamp: u64,
    pub protocol_version: ProtocolVersionId,
    pub historical_fee_input: Option<BatchFeeInput>,
}

impl BlockArgs {
    pub(crate) fn is_pending_miniblock(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
        )
    }

    pub(crate) fn is_estimate_like(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
                | api::BlockId::Number(api::BlockNumber::Latest)
                | api::BlockId::Number(api::BlockNumber::Committed)
        )
    }

    pub(crate) async fn resolve_block_info(
        &self,
        connection: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<ResolvedBlockInfo> {
        let (state_l2_block_number, vm_l1_batch_number, l1_batch_timestamp);

        if self.is_pending_miniblock() {
            let sealed_l1_batch_number =
                connection.blocks_dal().get_sealed_l1_batch_number().await?;
            let sealed_miniblock_header = connection
                .blocks_dal()
                .get_last_sealed_miniblock_header()
                .await?
                .context("no miniblocks in storage")?;

            vm_l1_batch_number = match sealed_l1_batch_number {
                Some(number) => number + 1,
                None => projected_first_l1_batch(connection).await?,
            };
            state_l2_block_number = sealed_miniblock_header.number;
            // Timestamp of the next L1 batch must be greater than the timestamp of the last miniblock.
            l1_batch_timestamp = seconds_since_epoch().max(sealed_miniblock_header.timestamp + 1);
        } else {
            vm_l1_batch_number = connection
                .storage_web3_dal()
                .resolve_l1_batch_number_of_miniblock(self.resolved_block_number)
                .await?
                .expected_l1_batch();
            l1_batch_timestamp = self.l1_batch_timestamp_s.unwrap_or_else(|| {
                panic!(
                    "L1 batch timestamp is `None`, `block_id`: {:?}, `resolved_block_number`: {}",
                    self.block_id, self.resolved_block_number.0
                );
            });
            state_l2_block_number = self.resolved_block_number;
        };

        let historical_fee_input = if !self.is_estimate_like() {
            let miniblock_header = connection
                .blocks_dal()
                .get_miniblock_header(self.resolved_block_number)
                .await?
                .context("The resolved miniblock is not in storage")?;

            Some(miniblock_header.batch_fee_input)
        } else {
            None
        };

        // Blocks without version specified are considered to be of `Version9`.
        // TODO: remove `unwrap_or` when protocol version ID will be assigned for each block.
        let protocol_version = connection
            .blocks_dal()
            .get_miniblock_protocol_version_id(state_l2_block_number)
            .await?
            .unwrap_or(ProtocolVersionId::last_potentially_undefined());

        Ok(ResolvedBlockInfo {
            state_l2_block_number,
            vm_l1_batch_number,
            l1_batch_timestamp,
            protocol_version,
            historical_fee_input,
        })
    }
}
