use zksync_types::{api::en::SyncBlock, MiniblockNumber};
use zksync_web3_decl::error::Web3Error;

use crate::api_server::web3::{backend_jsonrpsee::internal_error, state::RpcState};

/// Namespace for External Node unique methods.
/// Main use case for it is the EN synchronization.
#[derive(Debug)]
pub struct EnNamespace {
    pub state: RpcState,
}

impl EnNamespace {
    pub fn new(state: RpcState) -> Self {
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
