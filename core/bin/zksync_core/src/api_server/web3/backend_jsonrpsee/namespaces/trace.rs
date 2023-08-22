use zksync_types::{api::OpenEthActionTrace, H256};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::trace::TraceNamespaceServer,
};

use crate::api_server::web3::namespaces::TraceNamespace;
use crate::l1_gas_price::L1GasPriceProvider;

#[async_trait]
impl<G: L1GasPriceProvider + Send + Sync + 'static> TraceNamespaceServer for TraceNamespace<G> {
    async fn trace_transaction(&self, tx_hash: H256) -> RpcResult<Vec<OpenEthActionTrace>> {
        Ok(self.trace_transaction_impl(tx_hash).await)
    }
}
