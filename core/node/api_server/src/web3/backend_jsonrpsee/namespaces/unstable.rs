use zksync_types::{
    api::{ChainAggProof, TeeProof, TransactionExecutionInfo},
    tee_types::TeeType,
    L1BatchNumber, L2ChainId, H256,
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

    async fn get_chain_log_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        chain_id: L2ChainId,
    ) -> RpcResult<Option<ChainAggProof>> {
        self.get_chain_log_proof_impl(l1_batch_number, chain_id)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_unconfirmed_txs_count(&self) -> RpcResult<usize> {
        self.get_unconfirmed_txs_count_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
