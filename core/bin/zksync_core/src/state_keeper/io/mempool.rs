use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use vm::utils::default_block_properties;
use vm::vm_with_bootloader::BlockContext;
use vm::vm_with_bootloader::BlockContextMode;
use vm::vm_with_bootloader::DerivedBlockContext;
use vm::zk_evm::block_properties::BlockProperties;
use vm::VmBlockResult;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_eth_client::EthInterface;
use zksync_mempool::L2TxFilter;
use zksync_types::block::MiniblockHeader;
use zksync_types::event::{extract_added_tokens, extract_long_l2_to_l1_messages};
use zksync_types::log_query_sorter::sort_storage_access_queries;
use zksync_types::FAIR_L2_GAS_PRICE;
use zksync_types::{
    block::L1BatchHeader, Address, L1BatchNumber, MiniblockNumber, StorageLogQueryType,
    Transaction, U256,
};
use zksync_utils::{miniblock_hash, time::millis_since_epoch};

use crate::gas_adjuster::GasAdjuster;
use crate::state_keeper::extractors;
use crate::state_keeper::updates::UpdatesManager;
use crate::state_keeper::MempoolGuard;

use super::PendingBatchData;
use super::StateKeeperIO;

#[derive(Debug)]
struct StateKeeperStats {
    num_contracts: u64,
}

/// Mempool-based IO for the state keeper.
/// Receives transactions from the database through the mempool filtering logic.
/// Decides which batch parameters should be used for the new batch.
/// This is an IO for the main server application.
#[derive(Debug)]
pub(crate) struct MempoolIO<E> {
    mempool: MempoolGuard,
    pool: ConnectionPool,
    filter: L2TxFilter,
    current_miniblock_number: MiniblockNumber,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    delay_interval: Duration,

    // Grafana metrics
    statistics: StateKeeperStats,

    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    gas_adjuster: Arc<GasAdjuster<E>>,
}

impl<E: 'static + EthInterface + std::fmt::Debug + Send + Sync> StateKeeperIO for MempoolIO<E> {
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self.pool.access_storage_blocking();

        // If pending miniblock doesn't exist, it means that there is no unsynced state (i.e. no transaction
        // were executed after the last sealed batch).
        let pending_miniblock_number = self.pending_miniblock_number(&mut storage);
        let pending_miniblock_header = storage
            .blocks_dal()
            .get_miniblock_header(pending_miniblock_number)?;

        vlog::info!("getting previous block hash");
        let previous_l1_batch_hash = extractors::wait_for_prev_l1_batch_state_root_unchecked(
            &mut storage,
            self.current_l1_batch_number,
        );
        vlog::info!("previous_l1_batch_hash: {}", previous_l1_batch_hash);
        let params = self.default_block_params(
            pending_miniblock_header.timestamp,
            previous_l1_batch_hash,
            pending_miniblock_header.l1_gas_price,
            pending_miniblock_header.l2_fair_gas_price,
        );

        let txs = storage.transactions_dal().get_transactions_to_reexecute();

        Some(PendingBatchData { params, txs })
    }

    fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> Option<(BlockContextMode, BlockProperties)> {
        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        poll_until(self.delay_interval, max_wait, || {
            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = self.gas_adjuster.l2_tx_filter();
            self.mempool.has_next(&self.filter).then(|| {
                // We only need to get the root hash when we're certain that we have a new transaction.
                vlog::info!("getting previous block hash");
                let previous_l1_batch_hash = {
                    let mut storage = self.pool.access_storage_blocking();
                    extractors::wait_for_prev_l1_batch_state_root_unchecked(
                        &mut storage,
                        self.current_l1_batch_number,
                    )
                };
                vlog::info!("previous_l1_batch_hash: {}", previous_l1_batch_hash);

                self.default_block_params(
                    (millis_since_epoch() / 1000) as u64,
                    previous_l1_batch_hash,
                    self.filter.l1_gas_price,
                    FAIR_L2_GAS_PRICE,
                )
            })
        })
    }

    fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction> {
        poll_until(self.delay_interval, max_wait, || {
            let started_at = Instant::now();
            let res = self.mempool.next_transaction(&self.filter);
            metrics::histogram!(
                "server.state_keeper.get_tx_from_mempool",
                started_at.elapsed(),
            );
            res
        })
    }

    fn rollback(&mut self, tx: &Transaction) {
        // Reset nonces in the mempool.
        self.mempool.rollback(tx);
        // Insert the transaction back.
        self.mempool.insert(vec![tx.clone()], Default::default());
    }

    fn reject(&mut self, rejected: &Transaction, error: &str) {
        assert!(
            !rejected.is_l1(),
            "L1 transactions should not be rejected: {}",
            error
        );

        // Reset the nonces in the mempool, but don't insert the transaction back.
        self.mempool.rollback(rejected);

        // Mark tx as rejected in the storage.
        let mut storage = self.pool.access_storage_blocking();
        metrics::increment_counter!("server.state_keeper.rejected_transactions");
        vlog::warn!(
            "transaction {} is rejected with error {}",
            rejected.hash(),
            error
        );
        storage
            .transactions_dal()
            .mark_tx_as_rejected(rejected.hash(), &format!("rejected: {}", error));
    }

    fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) -> u64 {
        let new_miniblock_timestamp = (millis_since_epoch() / 1000) as u64;
        let pool = self.pool.clone();
        let mut storage = pool.access_storage_blocking();
        self.seal_miniblock_impl(&mut storage, updates_manager, false);
        new_miniblock_timestamp
    }

    fn seal_l1_batch(
        &mut self,
        block_result: VmBlockResult,
        updates_manager: UpdatesManager,
        block_context: DerivedBlockContext,
    ) {
        assert_eq!(
            updates_manager.batch_timestamp(),
            block_context.context.block_timestamp,
            "Batch timestamps don't match, batch number {}",
            self.current_l1_batch_number()
        );
        let pool = self.pool.clone();
        let mut storage = pool.access_storage_blocking();
        self.seal_l1_batch_impl(&mut storage, block_result, updates_manager, block_context);
    }
}

impl<E: EthInterface> MempoolIO<E> {
    pub(crate) fn new(
        mempool: MempoolGuard,
        pool: ConnectionPool,
        fee_account: Address,
        delay_interval: Duration,
        gas_adjuster: Arc<GasAdjuster<E>>,
    ) -> Self {
        let mut storage = pool.access_storage_blocking();
        let last_sealed_block_header = storage.blocks_dal().get_newest_block_header();
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number();
        let num_contracts = storage.storage_load_dal().load_number_of_contracts();
        let filter = L2TxFilter::default(); // Will be initialized properly on the first newly opened batch.
        drop(storage);

        Self {
            mempool,
            pool,
            filter,
            current_l1_batch_number: last_sealed_block_header.number + 1,
            current_miniblock_number: last_miniblock_number + 1,
            fee_account,
            delay_interval,
            statistics: StateKeeperStats { num_contracts },
            gas_adjuster,
        }
    }

    fn pending_miniblock_number(&self, storage: &mut StorageProcessor<'_>) -> MiniblockNumber {
        let (_, last_miniblock_number_included_in_l1_batch) = storage
            .blocks_dal()
            .get_miniblock_range_of_l1_batch(self.current_l1_batch_number - 1)
            .unwrap();
        last_miniblock_number_included_in_l1_batch + 1
    }

    fn miniblock_assertions(&self, updates_manager: &UpdatesManager, is_fictive: bool) {
        if is_fictive {
            assert!(updates_manager.miniblock.executed_transactions.is_empty());
        } else {
            assert!(!updates_manager.miniblock.executed_transactions.is_empty());
        }

        let first_tx_index_in_miniblock = updates_manager.l1_batch.executed_transactions.len();
        let next_tx_index = updates_manager.pending_executed_transactions_len();
        let miniblock_tx_index_range = if is_fictive {
            next_tx_index..(next_tx_index + 1)
        } else {
            first_tx_index_in_miniblock..next_tx_index
        };

        for event in updates_manager.miniblock.events.iter() {
            assert!(miniblock_tx_index_range.contains(&(event.location.1 as usize)))
        }
        for storage_log in updates_manager.miniblock.storage_logs.iter() {
            assert!(miniblock_tx_index_range
                .contains(&(storage_log.log_query.tx_number_in_block as usize)))
        }
    }

    fn track_l1_batch_execution_stage(stage: &'static str, stage_started_at: &mut Instant) {
        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_time_stage",
            stage_started_at.elapsed(),
            "stage" => stage
        );
        *stage_started_at = Instant::now();
    }

    fn track_miniblock_execution_stage(stage: &'static str, stage_started_at: &mut Instant) {
        metrics::histogram!(
            "server.state_keeper.miniblock.sealed_time_stage",
            stage_started_at.elapsed(),
            "stage" => stage
        );
        *stage_started_at = Instant::now();
    }

    // If `is_fictive` flag is set to true, then it is assumed that
    // we should seal a fictive miniblock with no transactions in it. It is needed because
    // there might be some storage logs/events that are created
    // after the last processed tx in l1 batch.
    // For now, there is only one event for sending the fee to the operator..
    fn seal_miniblock_impl(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        updates_manager: &UpdatesManager,
        is_fictive: bool,
    ) {
        self.miniblock_assertions(updates_manager, is_fictive);

        let started_at = Instant::now();
        let mut stage_started_at: Instant = Instant::now();

        let (l1_tx_count, l2_tx_count) =
            extractors::l1_l2_tx_count(&updates_manager.miniblock.executed_transactions);
        vlog::info!(
            "sealing miniblock {} (l1 batch {}) with {} ({} l2 + {} l1) txs, {} events, (writes, reads): {:?}",
            self.current_miniblock_number,
            self.current_l1_batch_number,
            l1_tx_count + l2_tx_count,
            l2_tx_count,
            l1_tx_count,
            updates_manager.miniblock.events.len(),
            extractors::log_query_write_read_counts(&updates_manager.miniblock.storage_logs),
        );

        let mut transaction = storage.start_transaction_blocking();
        let miniblock_header = MiniblockHeader {
            number: self.current_miniblock_number,
            timestamp: updates_manager.miniblock.timestamp,
            hash: miniblock_hash(self.current_miniblock_number),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            base_fee_per_gas: updates_manager.base_fee_per_gas(),
            l1_gas_price: updates_manager.l1_gas_price(),
            l2_fair_gas_price: updates_manager.fair_l2_gas_price(),
        };

        transaction.blocks_dal().insert_miniblock(miniblock_header);
        Self::track_miniblock_execution_stage("insert_miniblock_header", &mut stage_started_at);

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_miniblock(
                self.current_miniblock_number,
                &updates_manager.miniblock.executed_transactions,
                updates_manager.base_fee_per_gas().into(),
            );
        Self::track_miniblock_execution_stage(
            "mark_transactions_in_miniblock",
            &mut stage_started_at,
        );

        let storage_logs = extractors::log_queries_to_storage_logs(
            &updates_manager.miniblock.storage_logs,
            updates_manager,
            is_fictive,
        );
        let write_logs = extractors::write_logs_from_storage_logs(storage_logs);

        transaction
            .storage_logs_dal()
            .insert_storage_logs(self.current_miniblock_number, &write_logs);
        Self::track_miniblock_execution_stage("insert_storage_logs", &mut stage_started_at);

        let unique_updates = transaction.storage_dal().apply_storage_logs(&write_logs);
        Self::track_miniblock_execution_stage("apply_storage_logs", &mut stage_started_at);

        let new_factory_deps = updates_manager.miniblock.new_factory_deps.clone();
        if !new_factory_deps.is_empty() {
            transaction
                .storage_dal()
                .insert_factory_deps(self.current_miniblock_number, new_factory_deps);
        }
        Self::track_miniblock_execution_stage("insert_factory_deps", &mut stage_started_at);

        // Factory deps should be inserted before using `contracts_deployed_this_miniblock`.
        let deployed_contracts =
            extractors::contracts_deployed_this_miniblock(unique_updates, &mut transaction);
        if !deployed_contracts.is_empty() {
            self.statistics.num_contracts += deployed_contracts.len() as u64;
        }

        let added_tokens = extract_added_tokens(&updates_manager.miniblock.events);
        if !added_tokens.is_empty() {
            transaction.tokens_dal().add_tokens(added_tokens);
        }
        Self::track_miniblock_execution_stage("insert_tokens", &mut stage_started_at);

        let events_this_miniblock = extractors::extract_events_this_block(
            &updates_manager.miniblock.events,
            updates_manager,
            is_fictive,
        );
        transaction
            .events_dal()
            .save_events(self.current_miniblock_number, events_this_miniblock);
        Self::track_miniblock_execution_stage("insert_events", &mut stage_started_at);

        let l2_to_l1_logs_this_miniblock = extractors::extract_l2_to_l1_logs_this_block(
            &updates_manager.miniblock.l2_to_l1_logs,
            updates_manager,
            is_fictive,
        );
        transaction
            .events_dal()
            .save_l2_to_l1_logs(self.current_miniblock_number, l2_to_l1_logs_this_miniblock);
        Self::track_miniblock_execution_stage("insert_l2_to_l1_logs", &mut stage_started_at);

        transaction.commit_blocking();
        Self::track_miniblock_execution_stage("commit_miniblock", &mut stage_started_at);

        metrics::histogram!(
            "server.state_keeper.miniblock.transactions_in_miniblock",
            updates_manager.miniblock.executed_transactions.len() as f64
        );
        metrics::histogram!(
            "server.miniblock.latency",
            ((millis_since_epoch() - updates_manager.miniblock.timestamp as u128 * 1000) as f64) / 1000f64,
             "stage" => "sealed"
        );
        metrics::histogram!(
            "server.state_keeper.miniblock.sealed_time",
            started_at.elapsed(),
        );
        metrics::gauge!(
            "server.miniblock.number",
            self.current_miniblock_number.0 as f64,
            "stage" => "sealed"
        );

        metrics::gauge!(
            "server.state_keeper.storage_contracts_size",
            self.statistics.num_contracts as f64
        );
        vlog::debug!(
            "sealed miniblock {} in {:?}",
            self.current_miniblock_number,
            started_at.elapsed()
        );

        Self::track_miniblock_execution_stage(
            "apply_miniblock_updates_to_l1_batch_updates_accumulator",
            &mut stage_started_at,
        );
        self.current_miniblock_number += 1;
    }

    fn seal_l1_batch_impl(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        block_result: VmBlockResult,
        mut updates_manager: UpdatesManager,
        block_context: DerivedBlockContext,
    ) {
        let started_at = Instant::now();
        let mut stage_started_at: Instant = Instant::now();

        let mut transaction = storage.start_transaction_blocking();

        // The vm execution was paused right after the last transaction was executed.
        // There is some post-processing work that the VM needs to do before the block is fully processed.
        let VmBlockResult {
            full_result,
            block_tip_result,
        } = block_result;
        assert!(
            full_result.revert_reason.is_none(),
            "VM must not revert when finalizing block. Revert reason: {:?}",
            full_result.revert_reason
        );
        Self::track_l1_batch_execution_stage("vm_finalization", &mut stage_started_at);

        updates_manager
            .miniblock
            .extend_from_fictive_transaction(block_tip_result.logs);
        // Seal fictive miniblock with last events and storage logs.
        self.seal_miniblock_impl(&mut transaction, &updates_manager, true);
        Self::track_l1_batch_execution_stage("fictive_miniblock", &mut stage_started_at);

        let (_, deduped_log_queries) =
            sort_storage_access_queries(&full_result.storage_log_queries);
        Self::track_l1_batch_execution_stage("log_deduplication", &mut stage_started_at);

        let (l1_tx_count, l2_tx_count) =
            extractors::l1_l2_tx_count(&updates_manager.l1_batch.executed_transactions);
        vlog::info!(
            "sealing l1 batch {:?} with {:?} ({:?} l2 + {:?} l1) txs, {:?} l2_l1_logs, {:?} events, (writes, reads): {:?} , (writes_dedup, reads_dedup): {:?} ",
            self.current_l1_batch_number,
            l1_tx_count + l2_tx_count,
            l2_tx_count,
            l1_tx_count,
            full_result.l2_to_l1_logs.len(),
            full_result.events.len(),
            extractors::log_query_write_read_counts(&full_result.storage_log_queries),
            extractors::log_query_write_read_counts(&deduped_log_queries),
        );

        let hash = extractors::wait_for_prev_l1_batch_state_root_unchecked(
            &mut transaction,
            self.current_l1_batch_number,
        );
        let block_context_properties = BlockContextMode::NewBlock(block_context, hash);

        let l1_batch = L1BatchHeader {
            number: self.current_l1_batch_number,
            is_finished: true,
            timestamp: block_context.context.block_timestamp,
            fee_account_address: self.fee_account,
            priority_ops_onchain_data: updates_manager.l1_batch.priority_ops_onchain_data.clone(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: full_result.l2_to_l1_logs,
            l2_to_l1_messages: extract_long_l2_to_l1_messages(&full_result.events),
            bloom: Default::default(),
            initial_bootloader_contents: extractors::get_initial_bootloader_memory(
                &updates_manager.l1_batch,
                block_context_properties,
            ),
            used_contract_hashes: full_result.used_contract_hashes,
            base_fee_per_gas: block_context.base_fee,
            l1_gas_price: updates_manager.l1_gas_price(),
            l2_fair_gas_price: updates_manager.fair_l2_gas_price(),
        };

        transaction
            .blocks_dal()
            .insert_l1_batch(l1_batch, updates_manager.l1_batch.l1_gas_count);
        Self::track_l1_batch_execution_stage("insert_l1_batch_header", &mut stage_started_at);

        transaction
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(self.current_l1_batch_number);
        Self::track_l1_batch_execution_stage(
            "set_l1_batch_number_for_miniblocks",
            &mut stage_started_at,
        );

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                self.current_l1_batch_number,
                &updates_manager.l1_batch.executed_transactions,
            );
        Self::track_l1_batch_execution_stage(
            "mark_txs_as_executed_in_l1_batch",
            &mut stage_started_at,
        );

        transaction
            .storage_logs_dedup_dal()
            .insert_storage_logs(self.current_l1_batch_number, &deduped_log_queries);
        Self::track_l1_batch_execution_stage("insert_storage_dedup_logs", &mut stage_started_at);

        let (protective_reads, deduplicated_writes): (Vec<_>, Vec<_>) = deduped_log_queries
            .into_iter()
            .partition(|log_query| log_query.log_type == StorageLogQueryType::Read);
        transaction
            .storage_logs_dedup_dal()
            .insert_protective_reads(self.current_l1_batch_number, &protective_reads);
        Self::track_l1_batch_execution_stage("insert_protective_reads", &mut stage_started_at);

        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(self.current_l1_batch_number, &deduplicated_writes);
        Self::track_l1_batch_execution_stage("insert_initial_writes", &mut stage_started_at);

        transaction.commit_blocking();
        Self::track_l1_batch_execution_stage("commit_l1_batch", &mut stage_started_at);

        metrics::histogram!(
            "server.state_keeper.l1_batch.updated_storage_keys_len",
            updates_manager.l1_batch.modified_storage_keys_number as f64
        );
        metrics::histogram!(
            "server.state_keeper.l1_batch.transactions_in_l1_batch",
            updates_manager.l1_batch.executed_transactions.len() as f64
        );
        metrics::histogram!(
            "server.l1_batch.latency",
            ((millis_since_epoch() - block_context.context.block_timestamp as u128 * 1000) as f64) / 1000f64,
             "stage" => "sealed"
        );
        metrics::gauge!(
            "server.block_number",
            self.current_l1_batch_number.0 as f64,
            "stage" => "sealed"
        );

        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_time",
            started_at.elapsed(),
        );
        vlog::debug!(
            "sealed l1 batch {} in {:?}",
            self.current_l1_batch_number,
            started_at.elapsed()
        );

        self.current_l1_batch_number += 1;
    }

    fn default_block_params(
        &self,
        l1_batch_timestamp: u64,
        previous_block_hash: U256,
        l1_gas_price: u64,
        fair_l2_gas_price: u64,
    ) -> (BlockContextMode, BlockProperties) {
        vlog::info!(
            "(l1_gas_price,fair_l2_gas_price) for block {} is ({l1_gas_price},{fair_l2_gas_price}",
            self.current_l1_batch_number.0
        );

        let block_properties = default_block_properties();

        let context = BlockContext {
            block_number: self.current_l1_batch_number.0,
            block_timestamp: l1_batch_timestamp,
            l1_gas_price,
            fair_l2_gas_price,
            operator_address: self.fee_account,
        };

        (
            BlockContextMode::NewBlock(context.into(), previous_block_hash),
            block_properties,
        )
    }
}

fn poll_until<T, F: FnMut() -> Option<T>>(
    delay_interval: Duration,
    max_wait: Duration,
    mut f: F,
) -> Option<T> {
    let wait_interval = delay_interval.min(max_wait);
    let start = Instant::now();
    while start.elapsed() <= max_wait {
        let res = f();
        if res.is_some() {
            return res;
        }
        std::thread::sleep(wait_interval);
    }
    None
}
