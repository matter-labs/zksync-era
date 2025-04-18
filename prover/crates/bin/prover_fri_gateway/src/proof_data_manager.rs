use std::sync::Arc;

use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::{api::ProofGenerationData, outputs::L1BatchProofForL1};
use zksync_types::{prover_dal::ProofCompressionJobStatus, L1BatchNumber};

use super::error::ProcessorError;

#[derive(Debug, Clone)]
pub struct ProofDataManager {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
}

impl ProofDataManager {
    pub fn new(blob_store: Arc<dyn ObjectStore>, pool: ConnectionPool<Prover>) -> Self {
        Self { blob_store, pool }
    }

    pub(crate) async fn get_next_proof(
        &self,
    ) -> Result<Option<(L1BatchNumber, L1BatchProofForL1)>, ProcessorError> {
        let Some((l1_batch_number, protocol_version, status)) = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_least_proven_block_not_sent_to_server()
            .await
        else {
            return Ok(None);
        };

        let proof = if status == ProofCompressionJobStatus::Successful {
            let proof: L1BatchProofForL1 = self
                .blob_store
                .get((l1_batch_number, protocol_version))
                .await?;
            proof
        } else {
            unreachable!(
                "Trying to send proof that are not successful status: {:?}",
                status
            );
        };

        Ok(Some((l1_batch_number, proof)))
    }

    pub(crate) async fn get_proof_for_batch(
        &self,
        batch_number: L1BatchNumber,
    ) -> Result<Option<L1BatchProofForL1>, ProcessorError> {
        let protocol_version = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_basic_witness_generator_dal()
            .protocol_version_for_l1_batch(batch_number)
            .await;

        let Some(protocol_version) = protocol_version else {
            return Ok(None);
        };

        let proof: L1BatchProofForL1 =
            match self.blob_store.get((batch_number, protocol_version)).await {
                Ok(proof) => proof,
                Err(ObjectStoreError::KeyNotFound(_)) => {
                    return Ok(None); // proof was not generated yet
                }
                Err(e) => {
                    return Err(ProcessorError::ObjectStoreErr(e));
                }
            };

        Ok(Some(proof))
    }

    pub(crate) async fn save_successful_sent_proof(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<(), ProcessorError> {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(l1_batch_number)
            .await?;
        Ok(())
    }

    pub(crate) async fn save_proof_gen_data(
        &self,
        data: ProofGenerationData,
    ) -> Result<(), ProcessorError> {
        let witness_inputs = self
            .blob_store
            .put(data.l1_batch_number, &data.witness_input_data)
            .await?;

        let mut connection = self.pool.connection().await.unwrap();

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await?;

        connection
            .fri_basic_witness_generator_dal()
            .save_witness_inputs(
                data.l1_batch_number,
                &witness_inputs,
                data.protocol_version,
                data.batch_sealed_at,
            )
            .await?;
        Ok(())
    }
}
