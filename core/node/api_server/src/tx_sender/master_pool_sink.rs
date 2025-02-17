use std::{collections::hash_map::HashMap, sync::Arc};

use tokio::sync::Mutex;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, Core, CoreDal};
use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{l2::L2Tx, Address, Nonce, H256};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::web3::metrics::API_METRICS;

type LockedTxnMap = Mutex<HashMap<(Address, Nonce), (H256, Arc<Mutex<()>>)>>;

/// Wrapper for the master DB pool that allows to submit transactions to the mempool.
#[derive(Debug)]
pub struct MasterPoolSink {
    master_pool: ConnectionPool<Core>,
    inflight_requests: LockedTxnMap,
}

impl MasterPoolSink {
    pub fn new(master_pool: ConnectionPool<Core>) -> Self {
        Self {
            master_pool,
            inflight_requests: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait::async_trait]
impl TxSink for MasterPoolSink {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_metrics: TransactionExecutionMetrics,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        let address_and_nonce = (tx.initiator_account(), tx.nonce());
        let tx_hash = tx.hash();

        let mut lock = self.inflight_requests.lock().await;
        let (hash_in_progress, mutex) = lock.entry(address_and_nonce).or_default();
        let mutex_clone = mutex.clone();
        // `_internal_lock` prevents simultaneous insertion of multiple txs with the same `address_and_nonce`.
        let Ok(_internal_lock) = mutex_clone.try_lock() else {
            let submission_res_handle = if *hash_in_progress == tx_hash {
                L2TxSubmissionResult::Duplicate
            } else {
                L2TxSubmissionResult::InsertionInProgress
            };
            APP_METRICS.processed_txs[&TxStage::Mempool(submission_res_handle)].inc();
            return Ok(submission_res_handle);
        };
        *hash_in_progress = tx_hash;
        drop(lock);
        API_METRICS.inflight_tx_submissions.inc_by(1);

        let result = match self.master_pool.connection_tagged("api").await {
            Ok(mut connection) => connection
                .transactions_dal()
                .insert_transaction_l2(tx, execution_metrics, validation_traces)
                .await
                .inspect(|submission_res_handle| {
                    APP_METRICS.processed_txs[&TxStage::Mempool(*submission_res_handle)].inc();
                })
                .map_err(|err| err.generalize().into()),
            Err(err) => Err(err.generalize().into()),
        };

        API_METRICS.inflight_tx_submissions.dec_by(1);

        result
    }
}
