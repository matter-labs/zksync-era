use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::{Mutex as TokioMutex, MutexGuard};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_mempool::{AdvanceInput, L2TxFilter, MempoolInfo, MempoolStore};
use zksync_types::{Address, Nonce, PriorityOpId, Transaction, TransactionTimeRangeConstraint};

use super::metrics::StateKeeperGauges;

#[derive(Debug, Clone)]
pub struct MempoolGuard {
    mempool: Arc<Mutex<MempoolStore>>,
    critical_mutex: Arc<TokioMutex<()>>,
}

impl MempoolGuard {
    pub async fn from_storage(
        storage_processor: &mut Connection<'_, Core>,
        capacity: u64,
        high_priority_l2_tx_initiator: Option<Address>,
        high_priority_l2_tx_protocol_version: Option<ProtocolVersionId>,
    ) -> Self {
        let next_priority_id = storage_processor
            .transactions_dal()
            .next_priority_id()
            .await;
        Self::new(
            next_priority_id,
            capacity,
            high_priority_l2_tx_initiator,
            high_priority_l2_tx_protocol_version,
        )
    }

    pub(super) fn new(
        next_priority_id: PriorityOpId,
        capacity: u64,
        high_priority_l2_tx_initiator: Option<Address>,
        high_priority_l2_tx_protocol_version: Option<ProtocolVersionId>,
    ) -> Self {
        let store = MempoolStore::new(
            next_priority_id,
            capacity,
            high_priority_l2_tx_initiator,
            high_priority_l2_tx_protocol_version,
        );
        Self {
            mempool: Arc::new(Mutex::new(store)),
            critical_mutex: Arc::new(TokioMutex::new(())),
        }
    }

    pub fn insert(
        &self,
        transactions: Vec<(Transaction, TransactionTimeRangeConstraint)>,
        nonces: HashMap<Address, Nonce>,
    ) {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .insert(transactions, nonces);
    }

    pub fn has_next(&self, filter: &L2TxFilter) -> bool {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .has_next(filter)
    }

    pub fn next_transaction(
        &mut self,
        filter: &L2TxFilter,
    ) -> Option<(Transaction, TransactionTimeRangeConstraint)> {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .next_transaction(filter)
    }

    pub fn rollback(&mut self, rejected: &Transaction) -> TransactionTimeRangeConstraint {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .rollback(rejected)
    }

    pub fn get_mempool_info(&mut self) -> MempoolInfo {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .get_mempool_info()
    }

    pub fn advance_after_block(&self, input: AdvanceInput) {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .advance_after_block(input)
    }

    pub async fn enter_critical(&self) -> MutexGuard<'_, ()> {
        self.critical_mutex.lock().await
    }

    #[cfg(test)]
    pub fn stats(&self) -> zksync_mempool::MempoolStats {
        self.mempool
            .lock()
            .expect("failed to acquire mempool lock")
            .stats()
    }

    pub fn register_metrics(&self) {
        StateKeeperGauges::register(Arc::downgrade(&self.mempool));
    }
}
