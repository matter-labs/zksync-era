#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{
        ChainAggProof, DataAvailabilityDetails, GatewayMigrationStatus, L1ToL2TxsStatus, TeeProof,
        TransactionDetailedResult, TransactionExecutionInfo,
    },
    block::BatchOrBlockNumber,
    tee_types::TeeType,
    L1BatchNumber, L2ChainId, H256,
};

use crate::{
    client::{ForWeb3Network, L2},
    types::Bytes,
};

/// RPCs in this namespace are experimental, and their interface is unstable, and it WILL change.
#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "unstable", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "unstable", client_bounds(Self: ForWeb3Network<Net = L2>))
)]
pub trait UnstableNamespace {
    #[method(name = "getTransactionExecutionInfo")]
    async fn transaction_execution_info(
        &self,
        hash: H256,
    ) -> RpcResult<Option<TransactionExecutionInfo>>;

    #[method(name = "getTeeProofs")]
    async fn tee_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        tee_type: Option<TeeType>,
    ) -> RpcResult<Vec<TeeProof>>;

    // Accepts batch_or_block_number as a JSON number (for L1 batch) or an object `{"block": <number>}` (for GW block).
    #[method(name = "getChainLogProof")]
    async fn get_chain_log_proof(
        &self,
        batch_or_block_number: BatchOrBlockNumber,
        chain_id: L2ChainId,
    ) -> RpcResult<Option<ChainAggProof>>;

    #[method(name = "unconfirmedTxsCount")]
    async fn get_unconfirmed_txs_count(&self) -> RpcResult<usize>;

    #[method(name = "getDataAvailabilityDetails")]
    async fn get_data_availability_details(
        &self,
        batch: L1BatchNumber,
    ) -> RpcResult<Option<DataAvailabilityDetails>>;

    #[method(name = "supportsUnsafeDepositFilter")]
    async fn supports_unsafe_deposit_filter(&self) -> RpcResult<bool>;

    #[method(name = "l1ToL2TxsStatus")]
    async fn l1_to_l2_txs_status(&self) -> RpcResult<L1ToL2TxsStatus>;

    #[method(name = "gatewayMigrationStatus")]
    async fn gateway_migration_status(&self) -> RpcResult<GatewayMigrationStatus>;

    #[method(name = "sendRawTransactionWithDetailedOutput")]
    async fn send_raw_transaction_with_detailed_output(
        &self,
        tx_bytes: Bytes,
    ) -> RpcResult<TransactionDetailedResult>;
}
