use std::sync::Arc;

use zksync_config::configs::object_store;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_prover_interface::outputs::{L1BatchProofForL1, L1BatchProofForL1Key};
use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::event_processors::EventHandler};

// event ProofRequestProven(
//    uint256 indexed chainId, uint256 indexed blockNumber, bytes proof, ProvingNetwork assignedTo
//);
pub struct ProofRequestProvenEvent {
    pub chain_id: U256,
    pub block_number: U256,
    pub proof: Vec<U256>,
    pub assigned_to: ProvingNetwork,
}

pub struct ProofRequestProvenEventHandler;

#[async_trait]
impl EventHandler for ProofRequestProvenEventHandler {
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

    async fn handle_event(
        &self,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        let proof = <L1BatchProofForL1 as StoredObject>::deserialize(self.proof)?;

        // todo: verify proof
        let key = L1BatchProofForL1Key::Core((self.block_number, proof.protocol_version()));
        let blob_url = blob_store.put(key, &proof).await?;

        let transaction = connection_pool
            .connection()
            .await?
            .start_transaction()
            .await?;

        transaction
            .eth_proof_manager_dal()
            .save_validation_result(self.block_number, true)
            .await?;
        transaction
            .proof_generation_dal()
            .save_proof_artifacts_metadata(self.block_number, &blob_url)
            .await?;

        transaction.commit().await?;

        Ok(())
    }
}
