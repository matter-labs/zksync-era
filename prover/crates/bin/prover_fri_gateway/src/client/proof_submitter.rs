use std::sync::Arc;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::api::{SubmitProofRequest, SubmitProofResponse};
use zksync_types::L1BatchId;

use crate::{client::ProverApiClient, proof_data_manager::ProofDataManager, traits::PeriodicApi};

/// The path to the API endpoint that submits the proof.
const SUBMIT_PROOF_PATH: &str = "/submit_proof";

/// Poller structure that will periodically check the database for new proofs to submit.
/// Once a new proof is detected, it will be sent to the prover API.
#[derive(Debug)]
pub struct ProofSubmitter {
    manager: ProofDataManager,
    client: ProverApiClient,
}

impl ProofSubmitter {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        base_url: String,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        let api_url = format!("{base_url}{SUBMIT_PROOF_PATH}");
        let client = ProverApiClient::new(api_url);
        let manager = ProofDataManager::new(blob_store.clone(), pool.clone());
        Self { manager, client }
    }
}

#[async_trait]
impl PeriodicApi for ProofSubmitter {
    type JobId = L1BatchId;
    type Request = SubmitProofRequest;
    type Response = SubmitProofResponse;
    const SERVICE_NAME: &'static str = "ProofSubmitter";

    async fn get_next_request(&self) -> anyhow::Result<Option<(Self::JobId, SubmitProofRequest)>> {
        let Some((l1_batch_id, proof)) = self
            .manager
            .get_next_proof()
            .await
            .map_err(|e| anyhow::anyhow!(e))?
        else {
            return Ok(None);
        };

        Ok(Some((
            l1_batch_id,
            SubmitProofRequest::Proof(Box::new(proof.into())),
        )))
    }

    async fn send_request(
        &self,
        job_id: Self::JobId,
        request: SubmitProofRequest,
    ) -> reqwest::Result<Self::Response> {
        let endpoint = format!(
            "{}/{job_id}",
            self.client.api_url,
            job_id = job_id.batch_number()
        );
        self.client.send_http_request(request, &endpoint).await
    }

    async fn handle_response(
        &self,
        job_id: L1BatchId,
        response: Self::Response,
    ) -> anyhow::Result<()> {
        tracing::info!("Received response: {:?}", response);
        self.manager.save_successful_sent_proof(job_id).await?;
        Ok(())
    }
}
