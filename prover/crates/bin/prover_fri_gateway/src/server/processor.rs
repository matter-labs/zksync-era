use std::sync::Arc;

use zksync_object_store::ObjectStore;
use zksync_prover_dal::Prover;
use zksync_prover_dal::ConnectionPool;
use zksync_prover_dal::ProverDal;
use zksync_prover_interface::api::ProofGenerationData;
use zksync_prover_interface::api::SubmitProofRequest;
use zksync_types::prover_dal::ProofCompressionJobStatus;
use zksync_types::L1BatchNumber;

#[derive(Clone)]
pub struct Processor {
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
}

impl Processor {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        Self { blob_store, pool }
    }

    pub(crate) async fn next_submit_proof_request(&self) -> Option<(L1BatchNumber, SubmitProofRequest)> {
        let (l1_batch_number, protocol_version, status) = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_least_proven_block_not_sent_to_server()
            .await?;

        let request = match status {
            ProofCompressionJobStatus::Successful => {
                let proof = self
                    .blob_store
                    .get((l1_batch_number, protocol_version))
                    .await
                    .expect("Failed to get compressed snark proof from blob store");
                SubmitProofRequest::Proof(Box::new(proof))
            }
            ProofCompressionJobStatus::Skipped => SubmitProofRequest::SkippedProofGeneration,
            _ => panic!(
                "Trying to send proof that are not successful status: {:?}",
                status
            ),
        };

        Some((l1_batch_number, request))
    }

    pub(crate) async fn save_successful_sent_proof(&self, l1_batch_number: L1BatchNumber) {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(l1_batch_number)
            .await;
    }

    pub(crate) async fn save_proof_gen_data(&self, data: ProofGenerationData) {
        let witness_inputs = self.blob_store
            .put(data.l1_batch_number, &data.witness_input_data)
            .await
            .expect("Failed to save proof generation data to GCS");
        let mut connection = self.pool.connection().await.unwrap();

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await;

        connection
            .fri_basic_witness_generator_dal()
            .save_witness_inputs(data.l1_batch_number, &witness_inputs, data.protocol_version)
            .await;
    }
}