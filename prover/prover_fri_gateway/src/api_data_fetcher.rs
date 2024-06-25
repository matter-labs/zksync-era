use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use reqwest::Client;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{sync::watch, time::sleep};
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};

use crate::metrics::METRICS;

/// The path to the API endpoint that returns the next proof generation data.
pub(crate) const PROOF_GENERATION_DATA_PATH: &str = "/proof_generation_data";

/// The path to the API endpoint that submits the proof.
pub(crate) const SUBMIT_PROOF_PATH: &str = "/submit_proof";

pub(crate) struct PeriodicApiStruct {
    pub(crate) blob_store: Arc<dyn ObjectStore>,
    pub(crate) pool: ConnectionPool<Prover>,
    pub(crate) api_url: String,
    pub(crate) poll_duration: Duration,
    pub(crate) client: Client,
}

impl PeriodicApiStruct {
    pub(crate) async fn send_http_request<Req, Resp>(
        &self,
        request: Req,
        endpoint: &str,
    ) -> Result<Resp, reqwest::Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        tracing::info!("Sending request to {}", endpoint);

        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<Resp>()
            .await
    }

    pub(crate) async fn run<Req>(
        self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()>
    where
        Req: Send,
        Self: PeriodicApi<Req>,
    {
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

            if let Some((job_id, request)) = self.get_next_request().await {
                match self.send_request(job_id, request).await {
                    Ok(response) => {
                        self.handle_response(job_id, response).await;
                    }
                    Err(err) => {
                        METRICS.http_error[&Self::SERVICE_NAME].inc();
                        tracing::error!("HTTP request failed due to error: {}", err);
                    }
                }
            }
            tokio::select! {
                _ = stop_receiver.changed() => {
                    tracing::warn!("Stop signal received, shutting down {}", Self::SERVICE_NAME);
                    return Ok(());
                }
                _ = sleep(self.poll_duration) => {}
            }
        }
    }
}

/// Trait for fetching data from an API periodically.
#[async_trait]
pub(crate) trait PeriodicApi<Req: Send>: Sync + Send {
    type JobId: Send + Copy;
    type Response: Send;

    const SERVICE_NAME: &'static str;

    /// Returns the next request to be sent to the API and the endpoint to send it to.
    async fn get_next_request(&self) -> Option<(Self::JobId, Req)>;

    /// Handles the response from the API.
    async fn send_request(
        &self,
        job_id: Self::JobId,
        request: Req,
    ) -> reqwest::Result<Self::Response>;

    async fn handle_response(&self, job_id: Self::JobId, response: Self::Response);
}
