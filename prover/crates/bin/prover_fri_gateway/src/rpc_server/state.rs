use std::sync::Arc;
use jsonrpsee::core::{RpcResult, SubscriptionResult};
use jsonrpsee::proc_macros::rpc;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::api::{ProofGenerationData, SubmitProofRequest};
use zksync_types::{L1BatchNumber, L2ChainId};
use zksync_types::prover_dal::ProofCompressionJobStatus;
use crate::api::ProcessorError;

pub struct RpcState {
    pool: ConnectionPool<Prover>,
    blob_store: Arc<dyn ObjectStore>,
}

impl RpcState {
    pub fn new(pool: ConnectionPool<Prover>, blob_store: Arc<dyn ObjectStore>) -> Self {
        Self {
            pool,
            blob_store,
        }
    }

    pub async fn next_submit_proof_request(&self, chain_id: L2ChainId) -> Option<(L1BatchNumber, SubmitProofRequest)> {
        let (l1_batch_number, protocol_version, status) = self
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .get_least_proven_block_not_sent_to_server_by_chain_id(chain_id)
            .await?;

        let request = match status {
            ProofCompressionJobStatus::Successful => {
                let proof = self
                    .blob_store
                    .get((chain_id, l1_batch_number, protocol_version))
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

    pub async fn save_successful_sent_proof(&self, l1_batch_number: L1BatchNumber, chain_id: L2ChainId) {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(l1_batch_number, chain_id)
            .await;
    }

    pub async fn save_proof_gen_data(
        &self,
        data: ProofGenerationData,
    ) -> Result<(), ProcessorError> {
        tracing::info!(
            "Received proof generation data for batch: {:?}",
            data.l1_batch_number
        );

        let store = &*self.blob_store;
        let witness_inputs = store
            .put((data.chain_id, data.l1_batch_number), &data.witness_input_data)
            .await?;
        let mut connection = self.pool.connection().await?;

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await;

        connection
            .fri_basic_witness_generator_dal()
            .save_witness_inputs(data.chain_id, data.l1_batch_number, &witness_inputs, data.protocol_version)
            .await;
        Ok(())
    }
}
