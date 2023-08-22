// External uses
use crate::api_server::web3::namespaces::TraceNamespace;
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;

use crate::l1_gas_price::L1GasPriceProvider;
use zksync_types::{api::OpenEthActionTrace, H256};

#[rpc]
pub trait TraceNamespaceT {
    #[rpc(name = "trace_transaction")]
    fn trace_transaction(&self, tx_hash: H256) -> BoxFuture<Result<Vec<OpenEthActionTrace>>>;
}

impl<G: L1GasPriceProvider + Send + Sync + 'static> TraceNamespaceT for TraceNamespace<G> {
    fn trace_transaction(&self, tx_hash: H256) -> BoxFuture<Result<Vec<OpenEthActionTrace>>> {
        let self_ = self.clone();
        Box::pin(async move { Ok(self_.trace_transaction_impl(tx_hash).await) })
    }
}
