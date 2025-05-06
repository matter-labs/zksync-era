use std::time::Duration;

use tokio::sync::watch;

use crate::metrics::METRICS;

/// Trait for fetching data from an API periodically.
#[async_trait::async_trait]
pub(crate) trait PeriodicApi: Sync + Send + 'static + Sized {
    type JobId: Send + Copy;
    type Request: Send;
    type Response: Send;

    const SERVICE_NAME: &'static str;

    /// Returns the next request to be sent to the API and the endpoint to send it to.
    async fn get_next_request(&self) -> anyhow::Result<Option<(Self::JobId, Self::Request)>>;

    /// Submits a request to the API.
    async fn send_request(
        &self,
        job_id: Self::JobId,
        request: Self::Request,
    ) -> reqwest::Result<Self::Response>;

    /// Handles the response from the API.
    async fn handle_response(
        &self,
        job_id: Self::JobId,
        response: Self::Response,
    ) -> anyhow::Result<()>;

    /// Runs `get_next_request` -> `send_request` -> `handle_response` in a loop.
    async fn run(
        self,
        poll_duration: Duration,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Starting periodic job: {} with frequency: {:?}",
            Self::SERVICE_NAME,
            poll_duration
        );

        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down {}", Self::SERVICE_NAME);
                return Ok(());
            }

            if let Some((job_id, request)) = self.get_next_request().await? {
                match self.send_request(job_id, request).await {
                    Ok(response) => {
                        self.handle_response(job_id, response).await?;
                    }
                    Err(err) => {
                        METRICS.http_error[&Self::SERVICE_NAME].inc();
                        tracing::error!("HTTP request failed due to error: {}", err);
                    }
                }
            }
            // Exit condition will be checked on the next iteration.
            tokio::time::timeout(poll_duration, stop_receiver.changed())
                .await
                .ok();
        }
    }
}
