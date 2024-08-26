use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use zksync_dal::{Connection, Core, CoreDal};
use zksync_mempool::{L2TxFilter, MempoolInfo, MempoolStore};
use zksync_multivm::interface::{VmExecutionMetrics, VmExecutionResultAndLogs};
use zksync_types::{block::BlockGasCount, Address, Nonce, PriorityOpId, Transaction};

use super::{
    metrics::StateKeeperGauges,
    utils::{gas_count_from_metrics, gas_count_from_tx_and_metrics},
};

#[derive(Debug, Clone)]
pub struct MempoolGuard(Arc<Mutex<MempoolStore>>);

impl MempoolGuard {
    pub async fn from_storage(storage_processor: &mut Connection<'_, Core>, capacity: u64) -> Self {
        let next_priority_id = storage_processor
            .transactions_dal()
            .next_priority_id()
            .await;
        Self::new(next_priority_id, capacity)
    }

    pub(super) fn new(next_priority_id: PriorityOpId, capacity: u64) -> Self {
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

    #[cfg(test)]
    pub fn stats(&self) -> zksync_mempool::MempoolStats {
        self.0
            .lock()
            .expect("failed to acquire mempool lock")
            .stats()
    }

    pub fn register_metrics(&self) {
        StateKeeperGauges::register(Arc::downgrade(&self.0));
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionMetricsForCriteria {
    pub l1_gas: BlockGasCount,
    pub execution_metrics: VmExecutionMetrics,
}

impl ExecutionMetricsForCriteria {
    pub fn new(
        tx: Option<&Transaction>,
        execution_result: &VmExecutionResultAndLogs,
    ) -> ExecutionMetricsForCriteria {
        let execution_metrics = execution_result.get_execution_metrics(tx);
        let l1_gas = match tx {
            Some(tx) => gas_count_from_tx_and_metrics(tx, &execution_metrics),
            None => gas_count_from_metrics(&execution_metrics),
        };

        ExecutionMetricsForCriteria {
            l1_gas,
            execution_metrics,
        }
    }
}
