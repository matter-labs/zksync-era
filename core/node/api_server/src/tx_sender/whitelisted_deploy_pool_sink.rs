use zksync_dal::transactions_dal::L2TxSubmissionResult;
use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
use zksync_types::l2::L2Tx;

use super::{allow_list_service::AllowListService, tx_sink::TxSink, SubmitTxError};
use crate::tx_sender::master_pool_sink::MasterPoolSink;

#[derive(Debug)]
/// Wrapper that submits transactions to the mempool and enforces contract deployment allow-list.
pub struct WhitelistedDeployPoolSink {
    master_pool_sink: MasterPoolSink,
    allowlist_service: AllowListService,
}

impl WhitelistedDeployPoolSink {
    pub fn new(master_pool_sink: MasterPoolSink, allowlist_service: AllowListService) -> Self {
        Self {
            master_pool_sink,
            allowlist_service,
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
            let initiator_allowed = self.allowlist_service.is_address_allowed(&initiator).await;

            let contract_address_allowed = self
                .allowlist_service
                .is_address_allowed(&tx.execute.contract_address.unwrap_or_default())
                .await;

            if !initiator_allowed && !contract_address_allowed {
                tracing::info!(
                    "Blocking contract deployment. Neither initiator {:?} nor contract address {:?} is whitelisted.",
                    initiator,
                    tx.execute.contract_address
                );
                return Err(SubmitTxError::SenderNotInAllowList(initiator));
            }

            tracing::debug!(
                "Contract deployment allowed. Initiator: {:?}, Contract: {:?}, Tx hash: {:?}",
                initiator,
                tx.execute.contract_address,
                tx.hash()
            );
        }

        self.master_pool_sink
            .submit_tx(tx, execution_metrics, validation_traces)
            .await
    }
}
