use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{api::Log, ethabi, h256_to_u256, H256, U256};

use crate::watcher::events::EventHandler;

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProofRequestProven {
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<u8>,
}

#[derive(Debug)]
pub struct ProofRequestProvenHandler;

#[async_trait]
impl EventHandler for ProofRequestProvenHandler {
    fn signature(&self) -> H256 {
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
        log: Log,
        _connection_pool: ConnectionPool<Core>,
        _blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        if log.topics.len() != 3 {
            return Err(anyhow::anyhow!(
                "invalid number of topics: {:?}, expected 3",
                log.topics
            ));
        }

        if *log.topics.get(0).context("missing topic 0")? != self.signature() {
            return Err(anyhow::anyhow!(
                "invalid signature: {:?}, expected {:?}",
                log.topics.get(0),
                self.signature()
            ));
        }

        let chain_id = h256_to_u256(*log.topics.get(1).context("missing topic 1")?);
        let block_number = h256_to_u256(*log.topics.get(2).context("missing topic 2")?);
        let proof = log.data.0.to_vec();

        let event = ProofRequestProven {
            chain_id,
            block_number,
            proof,
        };

        tracing::info!("Received ProofRequestProvenEvent: {:?}", event);

        Ok(())
    }
}
