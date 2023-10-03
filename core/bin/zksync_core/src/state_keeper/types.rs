use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use zksync_mempool::{L2TxFilter, MempoolInfo, MempoolStore};
use zksync_types::{
    block::BlockGasCount, tx::ExecutionMetrics, Address, Nonce, PriorityOpId, Transaction,
};

#[derive(Debug, Clone)]
pub struct MempoolGuard(Arc<Mutex<MempoolStore>>);

impl MempoolGuard {
    pub fn new(next_priority_id: PriorityOpId, capacity: u64) -> Self {
        let store = MempoolStore::new(next_priority_id, capacity);
        Self(Arc::new(Mutex::new(store)))
    }

    pub fn insert(&mut self, transactions: Vec<Transaction>, nonces: HashMap<Address, Nonce>) {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .insert(transactions, nonces);
    }

    pub fn has_next(&self, filter: &L2TxFilter) -> bool {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .has_next(filter)
    }

    pub fn next_transaction(&mut self, filter: &L2TxFilter) -> Option<Transaction> {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .next_transaction(filter)
    }

    pub fn rollback(&mut self, rejected: &Transaction) {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .rollback(rejected);
    }

    pub fn get_mempool_info(&mut self) -> MempoolInfo {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .get_mempool_info()
    }

    // TODO (PLA-554): rework as a metrics collector
    pub fn run_metrics_reporting(&self) -> impl Future<Output = ()> {
        const METRICS_INTERVAL: Duration = Duration::from_secs(15);

        let weak_ref = Arc::downgrade(&self.0);
        async move {
            while let Some(pool) = weak_ref.upgrade() {
                let stats = pool.lock().expect("failed to acquire mempool lock").stats();
                drop(pool); // Don't prevent the pool to be dropped

                metrics::gauge!(
                    "server.state_keeper.mempool_l1_size",
                    stats.l1_transaction_count as f64
                );
                metrics::gauge!(
                    "server.state_keeper.mempool_l2_size",
                    stats.l2_transaction_count as f64
                );
                metrics::gauge!(
                    "server.state_keeper.mempool_l2_priority_queue_size",
                    stats.l2_priority_queue_size as f64
                );

                tokio::time::sleep(METRICS_INTERVAL).await;
                // ^ Since this is the only wait point, we don't need to handle this task a stop signal
                // or track its progress.
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionMetricsForCriteria {
    pub l1_gas: BlockGasCount,
    pub execution_metrics: ExecutionMetrics,
}
