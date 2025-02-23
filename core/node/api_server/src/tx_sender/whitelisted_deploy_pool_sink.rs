use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool, Core, CoreDal, DalError};
use zksync_multivm::interface::{tracer::ValidationTraces, TransactionExecutionMetrics};
use zksync_types::{l2::L2Tx, CONTRACT_DEPLOYER_ADDRESS};

use super::{tx_sink::TxSink, SubmitTxError};
use crate::tx_sender::master_pool_sink::MasterPoolSink;

/// Wrapper for the master DB pool that allows submitting transactions to the mempool.
#[derive(Debug)]
pub struct WhitelistedDeployPoolSink {
    master_pool_sync: MasterPoolSink,
    master_pool: ConnectionPool<Core>,
}

impl WhitelistedDeployPoolSink {
    pub fn new(master_pool_sync: MasterPoolSink, master_pool: ConnectionPool<Core>) -> Self {
        Self {
            master_pool_sync,
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
        
        tracing::info!("Processing transaction");
        if let Some(contract_address) = tx.execute.contract_address {
            tracing::info!("Allow List Sink : {:?}, {}", tx.hash(), contract_address);

            if contract_address == CONTRACT_DEPLOYER_ADDRESS {
                let mut connection = self
                    .master_pool
                    .connection_tagged("api")
                    .await
                    .map_err(DalError::generalize)?;
                
                let allow_list = connection
                .contracts_deploy_allow_list_dal()
                .get_allow_list()
                .await
                .unwrap();
            
                if allow_list.contains(&initiator) {
                    tracing::info!("Whitelisted address {:?} allowed to deploy contract: {:?}", initiator, tx.hash());
                } else {
                    tracing::info!("Blocking contract deployment for non-whitelisted address: {:?}", initiator);
                    return Err(SubmitTxError::SenderInAllowList(initiator));
                }
            }
        }

        self.master_pool_sync.submit_tx(tx, execution_metrics, validation_traces).await
    }
}
