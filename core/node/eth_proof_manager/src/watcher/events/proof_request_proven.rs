use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_prover_interface::outputs::{L1BatchProofForL1, L1BatchProofForL1Key};
use zksync_types::{api::Log, ethabi, h256_to_u256, L1BatchNumber, H256, U256};

use crate::watcher::events::EventHandler;

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
#[derive(Debug)]
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
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
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

        let proof = <L1BatchProofForL1 as StoredObject>::deserialize(proof).map_err(|e| anyhow::anyhow!("Failed to deserialize proof: {}", e))?;
        // todo: verify proof
        let verification_result = true;

        let batch_number = L1BatchNumber(block_number.as_u32());

        if verification_result {
            let proof_blob_url = blob_store.put(
                L1BatchProofForL1Key::Core((batch_number, proof.protocol_version())),
                &proof,
            ).await?;

            connection_pool.connection().await?.proof_generation_dal().save_proof_artifacts_metadata(batch_number, &proof_blob_url).await?;
        }

        connection_pool.connection().await?.eth_proof_manager_dal().mark_batch_as_proven(batch_number, verification_result).await?;

        Ok(())
    }
}
