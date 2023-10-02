use zksync_types::{api::en::SyncBlock, MiniblockNumber};
use zksync_web3_decl::error::Web3Error;

use crate::{
    api_server::{web3::backend_jsonrpc::error::internal_error, web3::state::RpcState},
    l1_gas_price::L1GasPriceProvider,
};

/// Namespace for External Node unique methods.
/// Main use case for it is the EN synchronization.
#[derive(Debug)]
pub struct EnNamespace<G> {
    pub state: RpcState<G>,
}

impl<G> Clone for EnNamespace<G> {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
        }
    }
}

impl<G: L1GasPriceProvider> EnNamespace<G> {
    pub fn new(state: RpcState<G>) -> Self {
        Self { state }
    }

    #[tracing::instrument(skip(self))]
    pub async fn sync_l2_block_impl(
        &self,
        block_number: MiniblockNumber,
        include_transactions: bool,
    ) -> Result<Option<SyncBlock>, Web3Error> {
        let mut storage = self
            .state
            .connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();
        storage
            .sync_dal()
            .sync_block(
                block_number,
                self.state.tx_sender.0.sender_config.fee_account_addr,
                include_transactions,
            )
            .await
            .map_err(|err| internal_error("en_syncL2Block", err))
    }
}
