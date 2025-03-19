use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, Core, CoreDal, DalError};
use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
use zksync_types::l2::L2Tx;

use super::{tx_sink::TxSink, SubmitTxError};
use crate::tx_sender::master_pool_sink::MasterPoolSink;

/// Wrapper for the master DB pool that allows submitting transactions to the mempool.
#[derive(Debug)]
pub struct WhitelistedDeployPoolSink {
    master_pool_sink: MasterPoolSink,
    master_pool: ConnectionPool<Core>,
}

impl WhitelistedDeployPoolSink {
    pub fn new(master_pool_sink: MasterPoolSink, master_pool: ConnectionPool<Core>) -> Self {
        Self {
            master_pool_sink,
            master_pool,
        }
    }
}

#[async_trait::async_trait]
impl TxSink for WhitelistedDeployPoolSink {
    async fn submit_tx(
        &self,
        tx: &L2Tx,
        execution_metrics: TransactionExecutionMetrics,
        validation_traces: ValidationTraces,
    ) -> Result<L2TxSubmissionResult, SubmitTxError> {
        let initiator = tx.initiator_account();

        // Only enforce the allow-list check if VM actually deployed a contract.
        if execution_metrics.vm.contract_deployment_count > 0 {
            let mut connection = self
                .master_pool
                .connection_tagged("api")
                .await
                .map_err(DalError::generalize)?;

            let is_allowed = connection
                .contracts_deploy_allow_list_dal()
                .is_address_allowed(&initiator)
                .await
                .map_err(DalError::generalize)?;

            if !is_allowed {
                tracing::info!(
                    "Blocking contract deployment for non-whitelisted address: {:?}",
                    initiator
                );
                return Err(SubmitTxError::SenderNotInAllowList(initiator));
            }

            tracing::debug!(
                "Whitelisted address {:?} allowed to deploy contract: {:?}",
                initiator,
                tx.hash()
            );
        }

        self.master_pool_sink
            .submit_tx(tx, execution_metrics, validation_traces)
            .await
    }
}
