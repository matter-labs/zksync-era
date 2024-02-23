use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool};
use zksync_types::{fee::TransactionExecutionMetrics, l2::L2Tx};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::metrics::{TxStage, APP_METRICS};

/// Wrapper for the master DB pool that allows to submit transactions to the mempool.
#[derive(Debug)]
pub struct MasterPoolSink {
    master_pool: ConnectionPool,
}

impl MasterPoolSink {
    pub fn new(master_pool: ConnectionPool) -> Self {
        Self { master_pool }
    }
}

#[async_trait::async_trait]
impl TxSink for MasterPoolSink {
    async fn submit_tx(
        &self,
        tx: L2Tx,
        execution_metrics: TransactionExecutionMetrics,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        let submission_res_handle = self
            .master_pool
            .access_storage_tagged("api")
            .await?
            .transactions_dal()
            .insert_transaction_l2(tx, execution_metrics)
            .await;

        APP_METRICS.processed_txs[&TxStage::Mempool(submission_res_handle)].inc();
        Ok(submission_res_handle)
    }
}
