use zksync_types::{
    api::{TeeProof, TransactionExecutionInfo},
    tee_types::TeeType,
    L1BatchNumber, H256,
};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::UnstableNamespaceServer,
};

use crate::web3::namespaces::UnstableNamespace;

#[async_trait]
impl UnstableNamespaceServer for UnstableNamespace {
    async fn transaction_execution_info(
        &self,
        hash: H256,
    ) -> RpcResult<Option<TransactionExecutionInfo>> {
        self.transaction_execution_info_impl(hash)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn tee_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> RpcResult<Vec<TeeProof>> {
        self.get_tee_proofs_impl(l1_batch_number, tee_type)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
