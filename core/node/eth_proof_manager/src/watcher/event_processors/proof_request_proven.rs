use std::sync::Arc;

use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::event_processors::Event};

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
pub struct ProofRequestProvenEvent {
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<U256>,
    pub assigned_to: ProvingNetwork,
}

impl Event for ProofRequestProvenEvent {
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

    fn handle_event(
        self,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
