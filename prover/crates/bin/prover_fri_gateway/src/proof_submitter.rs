use std::{sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::api::{SubmitProofRequest, SubmitProofResponse};
use zksync_types::{prover_dal::ProofCompressionJobStatus, L1BatchNumber};

use crate::metrics::METRICS;

/// The path to the API endpoint that submits the proof.
const SUBMIT_PROOF_PATH: &str = "/submit_proof";

/// Poller structure that will periodically check the database for new proofs to submit.
/// Once a new proof is detected, it will be sent to the prover API.
#[derive(Debug)]
pub struct PeriodicProofSubmitter {
    client: reqwest::Client,
    blob_store: Arc<dyn ObjectStore>,
    pool: ConnectionPool<Prover>,
    api_poll_duration: Duration,
    api_url: String,
}

impl PeriodicProofSubmitter {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        base_url: String,
        api_poll_duration: Duration,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            blob_store,
            pool,
            api_poll_duration,
            api_url: format!("{}{}", base_url, SUBMIT_PROOF_PATH),
        }
    }

    async fn next_submit_proof_request(&self) -> Option<(L1BatchNumber, SubmitProofRequest)> {
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

    async fn save_successful_sent_proof(&self, l1_batch_number: L1BatchNumber) {
        self.pool
            .connection()
            .await
            .unwrap()
            .fri_proof_compressor_dal()
            .mark_proof_sent_to_server(l1_batch_number)
            .await;
    }

    async fn get_next_request(&self) -> Option<(L1BatchNumber, SubmitProofRequest)> {
        let (l1_batch_number, request) = self.next_submit_proof_request().await?;
        Some((l1_batch_number, request))
    }

    async fn send_request(
        &self,
        l1_batch_number: L1BatchNumber,
        request: SubmitProofRequest,
    ) -> reqwest::Result<SubmitProofResponse> {
        tracing::info!("Sending proof for batch {:?}", l1_batch_number);

        self.client
            .post(format!(
                "{}{}/{}",
                self.api_url, SUBMIT_PROOF_PATH, l1_batch_number.0
            ))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<SubmitProofResponse>()
            .await
    }

    async fn handle_response(&self, l1_batch_number: L1BatchNumber, response: SubmitProofResponse) {
        tracing::info!("Received response: {:?}", response);
        self.save_successful_sent_proof(l1_batch_number).await;
    }

    /// Runs `get_next_request` -> `send_request` -> `handle_response` in a loop.
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            "Starting PeriodicProofSubmitter with frequency: {:?}",
            self.api_poll_duration
        );

        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down PeriodicProofSubmitter");
                return Ok(());
            }

            if let Some((l1_batch_number, request)) = self.get_next_request().await {
                match self.send_request(l1_batch_number, request).await {
                    Ok(response) => {
                        self.handle_response(l1_batch_number, response).await;
                    }
                    Err(err) => {
                        if let Some(status) = err.status() {
                            METRICS.submitter_http_error[status.as_u16()].inc();
                        } else {
                            tracing::warn!("Failed to decode status code from error");
                        }
                        tracing::error!("HTTP request failed due to error: {}", err);
                    }
                }
            } else {
                tracing::info!("There is currently no proof to submit");
            }
            // Exit condition will be checked on the next iteration.
            tokio::time::timeout(self.api_poll_duration, stop_receiver.changed())
                .await
                .ok();
        }
    }
}
