use zksync_types::{api::en::SyncBlock, MiniblockNumber};
use zksync_web3_decl::{
    jsonrpsee::core::{async_trait, RpcResult},
    namespaces::en::EnNamespaceServer,
};

use crate::{
    api_server::web3::{backend_jsonrpsee::into_jsrpc_error, namespaces::EnNamespace},
    l1_gas_price::L1GasPriceProvider,
};

#[async_trait]
impl<G: L1GasPriceProvider + Send + Sync + 'static> EnNamespaceServer for EnNamespace<G> {
    async fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> RpcResult<Option<SyncBlock>> {
        self.sync_l2_block_impl(block_number, include_transactions)
            .await
            .map_err(into_jsrpc_error)
    }
}
