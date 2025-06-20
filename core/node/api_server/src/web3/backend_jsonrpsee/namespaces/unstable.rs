use zksync_multivm::zk_evm_latest::ethereum_types::U256;
use zksync_types::{
    api::{
        ChainAggProof, DataAvailabilityDetails, GatewayMigrationStatus, L1ToL2TxsStatus, TeeProof,
        TransactionDetailedResult, TransactionExecutionInfo,
    },
    block::BatchOrBlockNumber,
    tee_types::TeeType,
    web3, L1BatchNumber, L2BlockNumber, L2ChainId, H256,
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
        batch_number: L1BatchNumber,
        chain_id: L2ChainId,
    ) -> RpcResult<Option<ChainAggProof>> {
        self.get_chain_log_proof_impl(BatchOrBlockNumber::BatchNumber(batch_number), chain_id)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_chain_log_proof_until_msg_root(
        &self,
        block_number: L2BlockNumber,
        chain_id: L2ChainId,
    ) -> RpcResult<Option<ChainAggProof>> {
        self.get_chain_log_proof_impl(BatchOrBlockNumber::BlockNumber(block_number), chain_id)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_unconfirmed_txs_count(&self) -> RpcResult<usize> {
        self.get_unconfirmed_txs_count_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn get_data_availability_details(
        &self,
        batch: L1BatchNumber,
    ) -> RpcResult<Option<DataAvailabilityDetails>> {
        self.get_data_availability_details_impl(batch)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn supports_unsafe_deposit_filter(&self) -> RpcResult<bool> {
        Ok(self.supports_unsafe_deposit_filter_impl())
    }

    async fn l1_to_l2_txs_status(&self) -> RpcResult<L1ToL2TxsStatus> {
        self.l1_to_l2_txs_status_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn gateway_migration_status(&self) -> RpcResult<GatewayMigrationStatus> {
        self.gateway_migration_status_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn send_raw_transaction_with_detailed_output(
        &self,
        tx_bytes: web3::Bytes,
    ) -> RpcResult<TransactionDetailedResult> {
        self.send_raw_transaction_with_detailed_output_impl(tx_bytes)
            .await
            .map_err(|err| self.current_method().map_err(err))
    }

    async fn gas_per_pubdata(&self) -> RpcResult<U256> {
        self.gas_per_pubdata_impl()
            .await
            .map_err(|err| self.current_method().map_err(err))
    }
}
