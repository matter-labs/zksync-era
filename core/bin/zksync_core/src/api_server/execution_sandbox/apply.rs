//! This module provides primitives focusing on the VM instantiation and execution for different use cases.
//! It is rather generic and low-level, so it's not supposed to be a part of public API.
//!
//! Instead, we expect people to write wrappers in the `execution_sandbox` module with a more high-level API
//! that would, in its turn, be used by the actual API method handlers.
//!
//! This module is intended to be blocking.

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use vm::{
    vm_with_bootloader::{
        derive_base_fee_and_gas_per_pubdata, init_vm, BlockContext, BlockContextMode,
        DerivedBlockContext,
    },
    zk_evm::block_properties::BlockProperties,
    HistoryDisabled, VmInstance,
};
use zksync_config::constants::ZKPORTER_IS_AVAILABLE;
use zksync_dal::{ConnectionPool, SqlxError, StorageProcessor};
use zksync_state::{PostgresStorage, ReadStorage, StorageView, WriteStorage};
use zksync_types::{
    api, get_nonce_key,
    utils::{decompose_full_nonce, nonces_to_full_nonce, storage_key_for_eth_balance},
    L1BatchNumber, MiniblockNumber, Nonce, StorageKey, Transaction, H256, U256,
};
use zksync_utils::{h256_to_u256, time::seconds_since_epoch, u256_to_h256};

use super::{vm_metrics, BlockArgs, TxExecutionArgs, TxSharedArgs, VmPermit};

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_vm_in_sandbox<T>(
    vm_permit: VmPermit,
    shared_args: TxSharedArgs,
    execution_args: &TxExecutionArgs,
    connection_pool: &ConnectionPool,
    tx: Transaction,
    block_args: BlockArgs,
    storage_read_cache: HashMap<StorageKey, H256>,
    apply: impl FnOnce(&mut Box<VmInstance<'_, HistoryDisabled>>, Transaction) -> T,
) -> (T, HashMap<StorageKey, H256>) {
    let stage_started_at = Instant::now();
    let span = tracing::debug_span!("initialization").entered();

    let rt_handle = vm_permit.rt_handle();
    let mut connection = rt_handle.block_on(connection_pool.access_storage_tagged("api"));
    let connection_acquire_time = stage_started_at.elapsed();
    // We don't want to emit too many logs.
    if connection_acquire_time > Duration::from_millis(10) {
        vlog::debug!(
            "Obtained connection (took {:?})",
            stage_started_at.elapsed()
        );
    }

    let resolve_started_at = Instant::now();
    let (state_block_number, vm_block_number) = rt_handle
        .block_on(block_args.resolve_block_numbers(&mut connection))
        .expect("Failed resolving block numbers");
    let resolve_time = resolve_started_at.elapsed();
    // We don't want to emit too many logs.
    if resolve_time > Duration::from_millis(10) {
        vlog::debug!(
            "Resolved block numbers (took {:?})",
            resolve_started_at.elapsed()
        );
    }

    if block_args.resolves_to_latest_sealed_miniblock() {
        shared_args
            .caches
            .schedule_values_update(state_block_number);
    }
    let block_timestamp = block_args.block_timestamp_seconds();

    let storage = PostgresStorage::new(rt_handle.clone(), connection, state_block_number, false)
        .with_caches(shared_args.caches);
    // Moving `storage_read_cache` to `storage_view`. It will be moved back once execution is finished and `storage_view` is not needed.
    let mut storage_view = StorageView::new_with_read_keys(storage, storage_read_cache);

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
    let storage_view_setup_time = storage_view_setup_started_at.elapsed();
    // We don't want to emit too many logs.
    if storage_view_setup_time > Duration::from_millis(10) {
        vlog::debug!("Prepared the storage view (took {storage_view_setup_time:?})",);
    }

    let mut oracle_tools = vm::OracleTools::new(&mut storage_view, HistoryDisabled);
    let block_properties = BlockProperties {
        default_aa_code_hash: h256_to_u256(shared_args.base_system_contracts.default_aa.hash),
        zkporter_is_available: ZKPORTER_IS_AVAILABLE,
    };
    let TxSharedArgs {
        l1_gas_price,
        fair_l2_gas_price,
        ..
    } = shared_args;

    let block_context = DerivedBlockContext {
        context: BlockContext {
            block_number: vm_block_number.0,
            block_timestamp,
            l1_gas_price,
            fair_l2_gas_price,
            operator_address: *shared_args.operator_account.address(),
        },
        base_fee: execution_args.enforced_base_fee.unwrap_or_else(|| {
            derive_base_fee_and_gas_per_pubdata(l1_gas_price, fair_l2_gas_price).0
        }),
    };

    // Since this method assumes that the block vm_block_number-1 is present in the DB, it means that its hash
    // has already been stored in the VM.
    let block_context_properties = BlockContextMode::OverrideCurrent(block_context);

    let mut vm = init_vm(
        &mut oracle_tools,
        block_context_properties,
        &block_properties,
        execution_args.execution_mode,
        &shared_args.base_system_contracts,
    );

    metrics::histogram!("api.web3.sandbox", stage_started_at.elapsed(), "stage" => "initialization");
    span.exit();

    let tx_id = format!(
        "{:?}-{}",
        tx.initiator_account(),
        tx.nonce().unwrap_or(Nonce(0))
    );
    let stage_started_at = Instant::now();
    let result = apply(&mut vm, tx);
    let vm_execution_took = stage_started_at.elapsed();
    metrics::histogram!("api.web3.sandbox", vm_execution_took, "stage" => "execution");

    let oracles_sizes = vm_metrics::record_vm_memory_metrics(&vm);
    vm_metrics::report_storage_view_metrics(
        &tx_id,
        oracles_sizes,
        vm_execution_took,
        storage_view.metrics(),
    );
    drop(vm_permit); // Ensure that the permit lives until this point

    // Move `read_storage_keys` from `storage_view` back to cache.
    (result, storage_view.into_read_storage_keys())
}

impl BlockArgs {
    fn is_pending_miniblock(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(api::BlockNumber::Pending)
        )
    }

    fn resolves_to_latest_sealed_miniblock(&self) -> bool {
        matches!(
            self.block_id,
            api::BlockId::Number(
                api::BlockNumber::Pending | api::BlockNumber::Latest | api::BlockNumber::Committed
            )
        )
    }

    async fn resolve_block_numbers(
        &self,
        connection: &mut StorageProcessor<'_>,
    ) -> Result<(MiniblockNumber, L1BatchNumber), SqlxError> {
        Ok(if self.is_pending_miniblock() {
            let sealed_l1_batch_number = connection
                .blocks_web3_dal()
                .get_sealed_l1_batch_number()
                .await?;
            let sealed_miniblock_number = connection
                .blocks_web3_dal()
                .get_sealed_miniblock_number()
                .await?;
            (sealed_miniblock_number, sealed_l1_batch_number + 1)
        } else {
            let l1_batch_number = connection
                .storage_web3_dal()
                .resolve_l1_batch_number_of_miniblock(self.resolved_block_number)
                .await?
                .expected_l1_batch();
            (self.resolved_block_number, l1_batch_number)
        })
    }

    fn block_timestamp_seconds(&self) -> u64 {
        if self.is_pending_miniblock() {
            seconds_since_epoch()
        } else {
            self.block_timestamp_s.unwrap_or_else(|| {
                panic!(
                    "Block timestamp is `None`, `block_id`: {:?}, `resolved_block_number`: {}",
                    self.block_id, self.resolved_block_number.0
                );
            })
        }
    }
}
