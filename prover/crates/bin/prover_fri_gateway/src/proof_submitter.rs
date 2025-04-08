use std::sync::Arc;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::api::{SubmitProofRequest, SubmitProofResponse};
use zksync_types::{prover_dal::ProofCompressionJobStatus, L1BatchNumber};

use crate::{client::ProverApiClient, traits::PeriodicApi};

/// The path to the API endpoint that submits the proof.
const SUBMIT_PROOF_PATH: &str = "/submit_proof";

/// Poller structure that will periodically check the database for new proofs to submit.
/// Once a new proof is detected, it will be sent to the prover API.
#[derive(Debug)]
pub struct ProofSubmitter(ProverApiClient);

impl ProofSubmitter {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        base_url: String,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        let api_url = format!("{base_url}{SUBMIT_PROOF_PATH}");
        let inner = ProverApiClient::new(blob_store, pool, api_url);
        Self(inner)
    }
}

impl ProofSubmitter {
    async fn next_submit_proof_request(&self) -> Option<(L1BatchNumber, SubmitProofRequest)> {
        let (l1_batch_number, protocol_version, status) = self
            .0
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
                    .0
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

    async fn save_successful_sent_proof(&self, l1_batch_number: L1BatchNumber) {
        self.0
            .pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(l1_batch_number)
            .await;
    }
}

#[async_trait]
impl PeriodicApi for ProofSubmitter {
    type JobId = L1BatchNumber;
    type Request = SubmitProofRequest;
    type Response = SubmitProofResponse;
    const SERVICE_NAME: &'static str = "ProofSubmitter";

    async fn get_next_request(&self) -> Option<(Self::JobId, SubmitProofRequest)> {
        let (l1_batch_number, request) = self.next_submit_proof_request().await?;
        Some((l1_batch_number, request))
    }

    async fn send_request(
        &self,
        job_id: Self::JobId,
        request: SubmitProofRequest,
    ) -> reqwest::Result<Self::Response> {
        let endpoint = format!("{}/{job_id}", self.0.api_url);
        self.0.send_http_request(request, &endpoint).await
    }

    async fn handle_response(&self, job_id: L1BatchNumber, response: Self::Response) {
        tracing::info!("Received response: {:?}", response);
        self.save_successful_sent_proof(job_id).await;
    }
}
