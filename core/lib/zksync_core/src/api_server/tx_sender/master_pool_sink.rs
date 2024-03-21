use std::collections::hash_map::{Entry, HashMap};

use tokio::sync::Mutex;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, Core, CoreDal};
use zksync_types::{fee::TransactionExecutionMetrics, l2::L2Tx, Address, Nonce, H256};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::{
    api_server::web3::metrics::API_METRICS,
    metrics::{TxStage, APP_METRICS},
};

/// Wrapper for the master DB pool that allows to submit transactions to the mempool.
#[derive(Debug)]
pub struct MasterPoolSink {
    master_pool: ConnectionPool<Core>,
    inflight_requests: Mutex<HashMap<(Address, Nonce), H256>>,
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
        tx: L2Tx,
        execution_metrics: TransactionExecutionMetrics,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        let address_and_nonce = (tx.initiator_account(), tx.nonce());

        let mut lock = self.inflight_requests.lock().await;
        match lock.entry(address_and_nonce) {
            Entry::Occupied(entry) => {
                let submission_res_handle = if entry.get() == &tx.hash() {
                    L2TxSubmissionResult::Duplicate
                } else {
                    L2TxSubmissionResult::InsertionInProgress
                };
                APP_METRICS.processed_txs[&TxStage::Mempool(submission_res_handle)].inc();
                return Ok(submission_res_handle);
            }
            Entry::Vacant(entry) => {
                entry.insert(tx.hash());
                API_METRICS.inflight_tx_submissions.inc_by(1);
            }
        };
        drop(lock);

        let result = match self.master_pool.connection_tagged("api").await {
            Ok(mut connection) => connection
                .transactions_dal()
                .insert_transaction_l2(tx, execution_metrics)
                .await
                .map(|submission_res_handle| {
                    APP_METRICS.processed_txs[&TxStage::Mempool(submission_res_handle)].inc();
                    submission_res_handle
                })
                .map_err(|err| anyhow::format_err!(err).into()),
            Err(err) => Err(err.into()),
        };

        self.inflight_requests
            .lock()
            .await
            .remove(&address_and_nonce);
        API_METRICS.inflight_tx_submissions.dec_by(1);

        result
    }
}
