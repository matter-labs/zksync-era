use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Clone, Error)]
pub enum ApiFetchError {
    #[error("Requests to the remote API were rate limited. Should wait for {} seconds", .0.as_secs())]
    RateLimit(Duration),
    #[error("Remote API is unavailable. Used URL: {0}")]
    ApiUnavailable(String),
    #[error("Unexpected JSON format. Error: {0}")]
    UnexpectedJsonFormat(String),
    #[error("Unable to receive data due to request timeout")]
    RequestTimeout,
    #[error("Unspecified error: {0}")]
    Other(String),
}

#[derive(Debug, Clone)]
pub struct ErrorAnalyzer {
    fetcher: String,
    min_errors_to_report: u64,
    error_counter: u64,
    requested_delay: Option<Duration>,
}

impl ErrorAnalyzer {
    pub fn new(fetcher: &str) -> Self {
        const MIN_ERRORS_FOR_REPORT: u64 = 20;

        Self {
            fetcher: fetcher.to_string(),
            min_errors_to_report: MIN_ERRORS_FOR_REPORT,
            error_counter: 0,
            requested_delay: None,
        }
    }

    pub fn reset(&mut self) {
        self.error_counter = 0;
    }

    pub async fn update(&mut self) {
        if self.error_counter >= self.min_errors_to_report {
            tracing::error!(
                "[{}] A lot of requests to the remote API failed in a row. Current error count: {}",
                &self.fetcher,
                self.error_counter
            );
        }

        if let Some(time) = self.requested_delay.take() {
            tokio::time::sleep(time).await;
        }
    }

    pub fn process_error(&mut self, error: ApiFetchError) {
        let fetcher = &self.fetcher;
        self.error_counter += 1;
        match error {
            ApiFetchError::RateLimit(time) => {
                tracing::warn!(
                        "[{}] Remote API notified us about rate limiting. Going to wait {} seconds before next loop iteration",
                        fetcher,
                        time.as_secs()
                    );
                self.requested_delay = Some(time);
            }
            ApiFetchError::UnexpectedJsonFormat(err) => {
                tracing::warn!("[{}] Parse data error: {}", fetcher, err);
            }
            ApiFetchError::ApiUnavailable(err) => {
                tracing::warn!("[{}] Remote API is unavailable: {}", fetcher, err);
            }
            ApiFetchError::RequestTimeout => {
                tracing::warn!("[{}] Request for data timed out", fetcher);
            }
            ApiFetchError::Other(err) => {
                tracing::warn!("[{}] Unspecified API error: {}", fetcher, err);
            }
        }
    }
}
