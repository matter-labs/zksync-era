use std::{fmt::format, time::Duration};

use tokio::sync::watch;
use zksync_prover_interface::api::{ProofGenerationData, SubmitProofGenerationDataResponse};
use zksync_types::L1BatchNumber;

use crate::{
    metrics::METRICS, proof_data_submitter::proof_data_processor::ProofGenerationDataProcessor,
};

pub mod proof_data_processor;

#[derive(Debug)]
pub struct ProofGenerationDataSubmitter {
    poll_duration: Duration,
    processor: ProofGenerationDataProcessor,
    api_url: String,
    client: reqwest::Client,
}

const ENDPOINT: &str = "/proof_generation_data";

impl ProofGenerationDataSubmitter {
    pub fn new(
        processor: ProofGenerationDataProcessor,
        poll_duration: Duration,
        api_url: String,
    ) -> Self {
        Self {
            poll_duration,
            processor,
            api_url,
            client: reqwest::Client::new(),
        }
    }

    async fn get_next_request(&self) -> Option<ProofGenerationData> {
        self.processor.get_proof_generation_data()
    }

    /// Submits a request to the API.
    async fn send_request(
        &self,
        request: ProofGenerationData,
    ) -> reqwest::Result<SubmitProofGenerationDataResponse> {
        self.client
            .post(format!("{}/{}", self.api_url, ENDPOINT))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<SubmitProofGenerationDataResponse>()
            .await
    }

    /// Handles the response from the API.
    async fn handle_response(
        &self,
        l1_batch_number: L1BatchNumber,
        response: SubmitProofGenerationDataResponse,
    ) -> anyhow::Result<()> {
        match response {
            SubmitProofGenerationDataResponse::Success => {
                tracing::info!(
                    "Successfully sent proof generation data for batch {:?}",
                    l1_batch_number
                );
            }
            SubmitProofGenerationDataResponse::Error(err) => {
                tracing::info!("Failed to send proof generation data for batch {:?}, error: {:?}, unlocking the batch", l1_batch_number, err);
                self.processor.unlock_batch(l1_batch_number).await?;
            }
        };
        Ok(())
    }

    /// Runs `get_next_request` -> `send_request` -> `handle_response` in a loop.
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!(
            "Starting periodic job: {} with frequency: {:?}",
            Self::SERVICE_NAME,
            self.poll_duration
        );

        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down {}", Self::SERVICE_NAME);
                return Ok(());
            }

            if let Some(data) = self.get_next_request().await {
                let l1_batch_number = data.l1_batch_number;
                match self.send_request(data).await {
                    Ok(_) => {
                        tracing::info!(
                            "Successfully sent to the gateway batch {:?}",
                            l1_batch_number
                        )
                    }
                    Err(err) => {
                        METRICS.http_error[&Self::SERVICE_NAME].inc();
                        tracing::error!(
                            "HTTP request failed due to error: {}, failed to send batch",
                            err,
                            l1_batch_number
                        );
                    }
                }
            }
            // Exit condition will be checked on the next iteration.
            tokio::time::timeout(self.poll_duration, stop_receiver.changed())
                .await
                .ok();
        }
    }
}
