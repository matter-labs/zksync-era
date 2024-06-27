#[cfg_attr(not(feature = "server"), allow(unused_imports))]
use jsonrpsee::core::RpcResult;
use jsonrpsee::proc_macros::rpc;
use zksync_types::{api::TransactionExecutionInfo, H256};

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
}
