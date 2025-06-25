use std::sync::Arc;

use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::events::Event};

//event ProofRequestAcknowledged(
//     uint256 indexed chainId,
//     uint256 indexed blockNumber,
//     bool accepted,
//     ProvingNetwork indexed assignedTo
// );
#[derive(Debug)]
pub struct ProofRequestAcknowledged {
    pub chain_id: U256,
    pub block_number: U256,
    pub accepted: bool,
    pub assigned_to: ProvingNetwork,
}

impl ProofRequestAcknowledged {
    pub fn empty() -> Self {
        Self {
            chain_id: U256::zero(),
            block_number: U256::zero(),
            accepted: false,
            assigned_to: ProvingNetwork::None,
        }
    }
}

#[async_trait]
impl Event for ProofRequestAcknowledged {
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

    async fn handle(
        &self,
        _connection_pool: ConnectionPool<Core>,
        _blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        tracing::info!("Received ProofRequestAcknowledgedEvent: {:?}", self);
        Ok(())
    }
}
