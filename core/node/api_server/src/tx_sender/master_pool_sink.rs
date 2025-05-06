use std::{
    collections::hash_map::{Entry, HashMap},
    sync::{Arc, Weak},
};

use tokio::sync::Mutex;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, Core, CoreDal, DalError};
use zksync_multivm::interface::tracer::ValidationTraces;
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{l2::L2Tx, Address, Nonce, H256};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::{execution_sandbox::SandboxExecutionOutput, web3::metrics::API_METRICS};

/// Guard for `address_and_nonce` keyed tx insertion.
struct Guard {
    inflight_requests: Weak<Mutex<HashMap<(Address, Nonce), H256>>>,
    address_and_nonce: (Address, Nonce),
}

impl Guard {
    pub fn new(
        inflight_requests: Weak<Mutex<HashMap<(Address, Nonce), H256>>>,
        address_and_nonce: (Address, Nonce),
    ) -> Self {
        Self {
            inflight_requests,
            address_and_nonce,
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let inflight_requests = self.inflight_requests.upgrade();
        let address_and_nonce = self.address_and_nonce;
        tokio::spawn(async move {
            if let Some(inflight_requests) = inflight_requests {
                inflight_requests.lock().await.remove(&address_and_nonce);
                API_METRICS.inflight_tx_submissions.dec_by(1);
            }
        });
    }
}

/// Wrapper for the master DB pool that allows to submit transactions to the mempool.
#[derive(Debug)]
pub struct MasterPoolSink {
    master_pool: ConnectionPool<Core>,
    inflight_requests: Arc<Mutex<HashMap<(Address, Nonce), H256>>>,
}

impl MasterPoolSink {
    pub fn new(master_pool: ConnectionPool<Core>) -> Self {
        Self {
            master_pool,
            inflight_requests: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl TxSink for MasterPoolSink {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_output: &SandboxExecutionOutput,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        let address_and_nonce = (tx.initiator_account(), tx.nonce());

        let mut lock = self.inflight_requests.lock().await;
        let _guard = match lock.entry(address_and_nonce) {
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

                Guard::new(Arc::downgrade(&self.inflight_requests), address_and_nonce)
            }
        };
        drop(lock);

        let mut connection = self
            .master_pool
            .connection_tagged("api")
            .await
            .map_err(DalError::generalize)?;
        let result = connection
            .transactions_dal()
            .insert_transaction_l2(tx, execution_output.metrics, validation_traces)
            .await
            .inspect(|submission_res_handle| {
                APP_METRICS.processed_txs[&TxStage::Mempool(*submission_res_handle)].inc();
            })
            .map_err(DalError::generalize)?;

        Ok(result)
    }
}
