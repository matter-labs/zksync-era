use zksync_dal::{CoreDal, DalError};
use zksync_types::api::TransactionExecutionInfo;
use zksync_web3_decl::{error::Web3Error, types::H256};

use crate::web3::{backend_jsonrpsee::MethodTracer, RpcState};

#[derive(Debug)]
pub(crate) struct UnstableNamespace {
    state: RpcState,
}

impl UnstableNamespace {
    pub fn new(state: RpcState) -> Self {
        Self { state }
    }

    pub(crate) fn current_method(&self) -> &MethodTracer {
        &self.state.current_method
    }

    pub async fn transaction_execution_info_impl(
        &self,
        hash: H256,
    ) -> Result<Option<TransactionExecutionInfo>, Web3Error> {
        let mut storage = self.state.acquire_connection().await?;
        Ok(storage
            .transactions_web3_dal()
            .get_unstable_transaction_execution_info(hash)
            .await
            .map_err(DalError::generalize)?
            .map(|execution_info| TransactionExecutionInfo { execution_info }))
    }
}
