use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use vm::{
    vm_with_bootloader::{derive_base_fee_and_gas_per_pubdata, DerivedBlockContext},
    VmBlockResult,
};
use zksync_contracts::{BaseSystemContracts, BaseSystemContractsHashes};
use zksync_dal::ConnectionPool;
use zksync_mempool::L2TxFilter;
use zksync_types::{Address, L1BatchNumber, MiniblockNumber, Transaction};
use zksync_utils::time::millis_since_epoch;

use crate::state_keeper::mempool_actor::l2_tx_filter;
use crate::{
    l1_gas_price::L1GasPriceProvider,
    state_keeper::{
        extractors,
        io::{
            common::{l1_batch_params, poll_until, StateKeeperStats},
            seal_logic::{seal_l1_batch_impl, seal_miniblock_impl},
            L1BatchParams, PendingBatchData, StateKeeperIO,
        },
        updates::UpdatesManager,
        MempoolGuard,
    },
};

use super::common::load_pending_batch;

/// Mempool-based IO for the state keeper.
/// Receives transactions from the database through the mempool filtering logic.
/// Decides which batch parameters should be used for the new batch.
/// This is an IO for the main server application.
#[derive(Debug)]
pub(crate) struct MempoolIO<G> {
    mempool: MempoolGuard,
    pool: ConnectionPool,
    filter: L2TxFilter,
    current_miniblock_number: MiniblockNumber,
    current_l1_batch_number: L1BatchNumber,
    fee_account: Address,
    fair_l2_gas_price: u64,
    delay_interval: Duration,

    // Grafana metrics
    statistics: StateKeeperStats,

    // Used to keep track of gas prices to set accepted price per pubdata byte in blocks.
    l1_gas_price_provider: Arc<G>,

    base_system_contracts: BaseSystemContracts,
}

impl<G: 'static + L1GasPriceProvider + std::fmt::Debug + Send + Sync> StateKeeperIO
    for MempoolIO<G>
{
    fn current_l1_batch_number(&self) -> L1BatchNumber {
        self.current_l1_batch_number
    }

    fn current_miniblock_number(&self) -> MiniblockNumber {
        self.current_miniblock_number
    }

    fn load_pending_batch(&mut self) -> Option<PendingBatchData> {
        let mut storage = self.pool.access_storage_blocking();

        let PendingBatchData { params, txs } =
            load_pending_batch(&mut storage, self.current_l1_batch_number, self.fee_account)?;
        // Initialize the filter for the transactions that come after the pending batch.
        // We use values from the pending block to match the filter with one used before the restart.
        let context = params.context_mode.inner_block_context().context;
        let (base_fee, gas_per_pubdata) =
            derive_base_fee_and_gas_per_pubdata(context.l1_gas_price, context.fair_l2_gas_price);
        self.filter = L2TxFilter {
            l1_gas_price: context.l1_gas_price,
            fee_per_gas: base_fee,
            gas_per_pubdata: gas_per_pubdata as u32,
        };

        Some(PendingBatchData { params, txs })
    }

    fn wait_for_new_batch_params(&mut self, max_wait: Duration) -> Option<L1BatchParams> {
        // Block until at least one transaction in the mempool can match the filter (or timeout happens).
        // This is needed to ensure that block timestamp is not too old.
        poll_until(self.delay_interval, max_wait, || {
            // We create a new filter each time, since parameters may change and a previously
            // ignored transaction in the mempool may be scheduled for the execution.
            self.filter = l2_tx_filter(self.l1_gas_price_provider.as_ref(), self.fair_l2_gas_price);
            self.mempool.has_next(&self.filter).then(|| {
                // We only need to get the root hash when we're certain that we have a new transaction.
                vlog::info!("getting previous block hash");
                let previous_l1_batch_hash = {
                    let mut storage = self.pool.access_storage_blocking();

                    let stage_started_at: Instant = Instant::now();
                    let hash = extractors::wait_for_prev_l1_batch_state_root_unchecked(
                        &mut storage,
                        self.current_l1_batch_number,
                    );
                    metrics::histogram!(
                        "server.state_keeper.wait_for_prev_hash_time",
                        stage_started_at.elapsed()
                    );
                    hash
                };
                vlog::info!("previous_l1_batch_hash: {}", previous_l1_batch_hash);
                vlog::info!(
                    "(l1_gas_price,fair_l2_gas_price) for block {} is ({},{})",
                    self.current_l1_batch_number.0,
                    self.filter.l1_gas_price,
                    self.fair_l2_gas_price
                );

                l1_batch_params(
                    self.current_l1_batch_number,
                    self.fee_account,
                    (millis_since_epoch() / 1000) as u64,
                    previous_l1_batch_hash,
                    self.filter.l1_gas_price,
                    self.fair_l2_gas_price,
                    self.base_system_contracts.clone(),
                )
            })
        })
    }

    fn wait_for_new_miniblock_params(&mut self, _max_wait: Duration) -> Option<u64> {
        let new_miniblock_timestamp = (millis_since_epoch() / 1000) as u64;
        Some(new_miniblock_timestamp)
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

    fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) {
        let pool = self.pool.clone();
        let mut storage = pool.access_storage_blocking();
        seal_miniblock_impl(
            self.current_miniblock_number,
            self.current_l1_batch_number,
            &mut self.statistics,
            &mut storage,
            updates_manager,
            false,
        );
        self.current_miniblock_number += 1;
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
        seal_l1_batch_impl(
            self.current_miniblock_number,
            self.current_l1_batch_number,
            &mut self.statistics,
            &mut storage,
            block_result,
            updates_manager,
            block_context,
        );
        self.current_miniblock_number += 1; // Due to fictive miniblock being sealed.
        self.current_l1_batch_number += 1;
    }
}

impl<G: L1GasPriceProvider> MempoolIO<G> {
    pub(crate) fn new(
        mempool: MempoolGuard,
        pool: ConnectionPool,
        fee_account: Address,
        fair_l2_gas_price: u64,
        delay_interval: Duration,
        l1_gas_price_provider: Arc<G>,
        base_system_contracts_hashes: BaseSystemContractsHashes,
    ) -> Self {
        let mut storage = pool.access_storage_blocking();
        let last_sealed_block_header = storage.blocks_dal().get_newest_block_header();
        let last_miniblock_number = storage.blocks_dal().get_sealed_miniblock_number();
        let num_contracts = storage.storage_load_dal().load_number_of_contracts();
        let filter = L2TxFilter::default(); // Will be initialized properly on the first newly opened batch.

        let base_system_contracts = storage.storage_dal().get_base_system_contracts(
            base_system_contracts_hashes.bootloader,
            base_system_contracts_hashes.default_aa,
        );
        drop(storage);

        Self {
            mempool,
            pool,
            filter,
            current_l1_batch_number: last_sealed_block_header.number + 1,
            current_miniblock_number: last_miniblock_number + 1,
            fee_account,
            fair_l2_gas_price,
            delay_interval,
            statistics: StateKeeperStats { num_contracts },
            l1_gas_price_provider,
            base_system_contracts,
        }
    }
}

/// Getters reqiored for testing the MempoolIO.
#[cfg(test)]
impl<G: L1GasPriceProvider> MempoolIO<G> {
    pub(super) fn filter(&self) -> &L2TxFilter {
        &self.filter
    }
}
