#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{
    api::{TeeProof, TransactionExecutionInfo},
    tee_types::TeeType,
    L1BatchNumber, H256,
};

use crate::client::{ForNetwork, L2};

/// RPCs in this namespace are experimental, and their interface is unstable, and it WILL change.
#[cfg_attr(
    feature = "server",
    rpc(server, client, namespace = "unstable", client_bounds(Self: ForNetwork<Net = L2>))
)]
#[cfg_attr(
    not(feature = "server"),
    rpc(client, namespace = "unstable", client_bounds(Self: ForNetwork<Net = L2>))
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
}
