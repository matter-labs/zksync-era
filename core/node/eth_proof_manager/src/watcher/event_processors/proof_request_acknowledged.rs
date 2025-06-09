//event ProofRequestAcknowledged(
//     uint256 indexed chainId,
//     uint256 indexed blockNumber,
//     bool accepted,
//     ProvingNetwork indexed assignedTo
// );

use std::sync::Arc;

use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::event_processors::Event};

pub struct ProofRequestAcknowledgedEvent {
    pub chain_id: U256,
    pub block_number: U256,
    pub accepted: bool,
    pub assigned_to: ProvingNetwork,
}

#[async_trait]
impl Event for ProofRequestAcknowledgedEvent {
    fn signature() -> H256 {
        ethabi::long_signature(
            "ProofRequestAcknowledged",
            &[
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Bool,
                // ProvingNetwork is enum, encoded as uint8
                ethabi::ParamType::Uint(8),
            ],
        )
    }

    async fn handle_event(
        self,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .mark_proof_request_as_acknowledged(self.block_number, self.accepted)
            .await?;

        Ok(())
    }
}
