use std::sync::Arc;

use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::{ObjectStore, StoredObject};
use zksync_prover_interface::outputs::{L1BatchProofForL1, L1BatchProofForL1Key};

use crate::client::EthProofManagerClient;

pub struct SubmitProofValidationSubmitter {
    client: Box<dyn EthProofManagerClient>,
    blob_store: Arc<dyn ObjectStore>,
    connection_pool: ConnectionPool<Core>,
    config: EthProofManagerConfig,
}

impl SubmitProofValidationSubmitter {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        blob_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Core>,
        config: EthProofManagerConfig,
    ) -> Self {
        Self {
            client,
            blob_store,
            connection_pool,
            config,
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        let next_batch_to_be_validated = self
            .connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .get_batch_to_validate()
            .await?;

        if let Some((batch_number, proof_blob_url)) = next_batch_to_be_validated {
            let proof_blob = self
                .blob_store
                .get_raw(Bucket::PublicWitnessInputs, &proof_blob_url)
                .await?;

            let proof = <L1BatchProofForL1 as StoredObject>::deserialize(proof_blob)?;

            let proof_request_identifier = ProofRequestIdentifier {
                chain_id: proof_generation_data.chain_id,
                block_number: proof_generation_data.l1_batch_number,
            };
        }
    }
}
