use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{ethabi, H256, U256};
use std::sync::Arc;

use crate::{types::ProvingNetwork, watcher::events::Event};

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
#[derive(Debug)]
pub struct ProofRequestProven {
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<U256>,
    pub assigned_to: ProvingNetwork,
}

impl ProofRequestProven {
    pub fn empty() -> Self {
        Self {
            chain_id: U256::zero(),
            block_number: U256::zero(),
            proof: vec![],
            assigned_to: ProvingNetwork::None,
        }
    }
}

#[async_trait]
impl Event for ProofRequestProven {
    fn signature() -> H256 {
        ethabi::long_signature(
            "ProofRequestProven",
            &[
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Bytes,
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
        tracing::info!("Received ProofRequestProvenEvent: {:?}", self);

        Ok(())
    }
}