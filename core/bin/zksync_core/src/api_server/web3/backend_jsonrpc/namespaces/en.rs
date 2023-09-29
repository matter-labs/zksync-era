// Built-in uses

// External uses
use jsonrpc_core::{BoxFuture, Result};
use jsonrpc_derive::rpc;

// Workspace uses
use zksync_types::{api::en::SyncBlock, MiniblockNumber};

// Local uses
use crate::{
    api_server::web3::{backend_jsonrpc::error::into_jsrpc_error, EnNamespace},
    l1_gas_price::L1GasPriceProvider,
};

#[rpc]
pub trait EnNamespaceT {
    #[rpc(name = "en_syncL2Block")]
    fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> BoxFuture<Result<Option<SyncBlock>>>;
}

impl<G: L1GasPriceProvider + Send + Sync + 'static> EnNamespaceT for EnNamespace<G> {
    fn sync_l2_block(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> BoxFuture<Result<Option<SyncBlock>>> {
        let self_ = self.clone();
        Box::pin(async move {
            self_
                .sync_l2_block_impl(block_number, include_transactions)
                .await
                .map_err(into_jsrpc_error)
        })
    }
}
