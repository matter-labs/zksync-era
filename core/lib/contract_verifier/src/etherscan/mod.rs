//! Etherscan verifier sends the contract verification requests that have been successfully
//! verified by ZKSync Contract Verifier to Etherscan.

use std::time::{Duration, Instant};

use client::EtherscanClient;
use errors::{EtherscanError, ProcessingError, VerifierError};
use solc_builds_fetcher::SOLC_BUILDS_FETCHER;
use tokio::sync::watch;
use zksync_dal::{
    etherscan_verification_dal::EtherscanVerificationJobResultStatus, ConnectionPool, Core, CoreDal,
};
use zksync_types::contract_verification::api::EtherscanVerificationRequest;

use crate::metrics::API_CONTRACT_VERIFIER_METRICS;

pub mod client;
pub mod errors;
mod solc_builds_fetcher;
pub mod types;
pub mod utils;

#[derive(Debug, Clone)]
pub struct EtherscanVerifier {
    client: EtherscanClient,
    connection_pool: ConnectionPool<Core>,
}

enum ApiErrorRetryPolicy {
    NotRetryable,
    Retryable {
        pause_duration: Duration,
        max_attempts: u32,
    },
    IndefinitelyRetryable {
        pause_duration: Duration,
    },
}

fn get_api_error_retry_policy(api_error: &EtherscanError) -> ApiErrorRetryPolicy {
    match api_error {
        EtherscanError::DailyVerificationRequestsLimitExceeded => {
            // The number of sent verification requests over the last 24 hours is not tracked
            // so there is no precise way to determine the proper pause duration.
            // 1 hour pause should be good enough, but if necessary more sophisticated solution can be implemented.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 60), // 1 hour
            }
        }
        EtherscanError::InvalidApiKey => {
            // Etherscan API key is invalid even though it has no expiration date.
            // Either Etherscan API is experiencing issues or the key was revoked.
            // 1 hour to give the API time to recover and not to spam the API with invalid requests.
            // Since Etherscan verification is completely async it shouldn't affect users much.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 60), // 1 hour
            }
        }
        EtherscanError::BlockedByCloudflare
        | EtherscanError::CloudFlareSecurityChallenge
        | EtherscanError::PageNotFound => {
            // The request was blocked by the Cloudflare or a blank page was returned by the API, both of which are unexpected.
            // Likely Etherscan is experiencing issues.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 10), // 10 mins
            }
        }
        EtherscanError::RateLimitExceeded => {
            // Etherscan has different rate limits. Verification requests limit is handled separately.
            // Apart from that there are a per second and a per day limits. We are very unlikely to hit
            // a daily limit. For the per second limit, we can pause for 5 sec.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(5),
            }
        }
        EtherscanError::Reqwest(_) => {
            // Most likely the error is caused by the network issue of some sort.
            // But if the request error is caused by the constructed payload, we can't afford to retry indefinitely,
            // so we number of attempts is limited.
            ApiErrorRetryPolicy::Retryable {
                pause_duration: Duration::from_secs(60), // 1 min
                max_attempts: 10,
            }
        }
        _ => ApiErrorRetryPolicy::NotRetryable,
    }
}

impl EtherscanVerifier {
    pub fn new(api_url: String, api_key: String, connection_pool: ConnectionPool<Core>) -> Self {
        Self {
            client: EtherscanClient::new(api_url, api_key),
            connection_pool,
        }
    }

    fn wait_for_request_interval() -> Duration {
        Duration::from_secs(1)
    }

    async fn get_next_request(
        &self,
    ) -> anyhow::Result<Option<EtherscanVerificationRequest>, VerifierError> {
        if let Err(err) = SOLC_BUILDS_FETCHER.update_builds().await {
            tracing::error!("Failed to update solc builds: {}", err);
        }
        // We consider that the job is stuck if it's being processed for more than 2 hours.
        // We need large overhead because Etherscan has pretty strict daily
        // limits for verification requests.
        // In case the daily verification rate limit is reached, the processing is paused for 1 hour
        // and the processing_started_at field is reset to the current time after the pause.
        // Since etherscan verification is completely async it shouldn't affect user experience much.
        const TIME_OVERHEAD: Duration = Duration::from_secs(2 * 60 * 60); // 2 hours

        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        let job = connection
            .etherscan_verification_dal()
            .get_next_queued_verification_request(TIME_OVERHEAD)
            .await?;
        Ok(job)
    }

    async fn save_result(&self, job_id: usize) -> anyhow::Result<(), VerifierError> {
        tracing::info!("Successfully processed etherscan verification request with id = {job_id}");
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .save_verification_result(
                job_id,
                EtherscanVerificationJobResultStatus::Successful,
                None,
            )
            .await?;

        API_CONTRACT_VERIFIER_METRICS.successful_verifications[&"etherscan_verifier"].inc();

        Ok(())
    }

    async fn save_failure(
        &self,
        job_id: usize,
        error: String,
    ) -> anyhow::Result<(), VerifierError> {
        tracing::warn!("Failed to process etherscan verification request with id = {job_id}");
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .save_verification_result(
                job_id,
                EtherscanVerificationJobResultStatus::Failed,
                Some(&error),
            )
            .await?;

        API_CONTRACT_VERIFIER_METRICS.failed_verifications[&"etherscan_verifier"].inc();

        Ok(())
    }

    async fn reset_etherscan_verification_processing_started_at(
        &self,
        request_id: usize,
    ) -> Result<(), VerifierError> {
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .reset_processing_started_at(request_id)
            .await?;

        Ok(())
    }

    async fn verify(
        &self,
        verification_request: EtherscanVerificationRequest,
    ) -> Result<(), VerifierError> {
        const POLL_VERIFICATION_RESULT_INTERVAL: Duration = Duration::from_secs(5);
        const MAX_POLL_VERIFICATION_RESULT_ATTEMPTS: u32 = 100;

        let req = verification_request.req;
        let is_verified = self
            .client
            .is_contract_verified(req.contract_address)
            .await?;

        if is_verified {
            tracing::info!(
                "Contract with address {:#?} is already verified",
                req.contract_address
            );
            return Ok(());
        }
        let verification_id = match verification_request.etherscan_verification_id {
            Some(id) => id,
            None => {
                tracing::info!(
                    "Sending verification request to Etherscan, address = {:#?}",
                    req.contract_address,
                );
                let request_id = match self.client.verify(req.clone().into()).await {
                    Ok(id) => id,
                    // Even though there is a call to check if the contract is already verified
                    // we still need to process ContractAlreadyVerified response from the verification
                    // API call. This is because the contract can be verified by another party.
                    Err(EtherscanError::ContractAlreadyVerified) => {
                        tracing::info!(
                            "Contract with address {:#?} is already verified",
                            req.contract_address
                        );
                        return Ok(());
                    }
                    Err(e) => return Err(e.into()),
                };
                let mut connection = self
                    .connection_pool
                    .connection_tagged("etherscan_verifier")
                    .await?;

                connection
                    .etherscan_verification_dal()
                    .save_etherscan_verification_id(verification_request.id, &request_id)
                    .await?;

                // Wait for the verification to be processed
                tokio::time::sleep(POLL_VERIFICATION_RESULT_INTERVAL).await;

                request_id
            }
        };

        let mut get_status_attempts = 0;
        loop {
            get_status_attempts += 1;
            tracing::info!(
                "Fetching contract verification status, address = {:#?}, attempt = {}",
                req.contract_address,
                get_status_attempts
            );
            let result = self.client.get_verification_status(&verification_id).await;
            match result {
                Ok(_) => break,
                Err(EtherscanError::VerificationPending) => {
                    tokio::time::sleep(POLL_VERIFICATION_RESULT_INTERVAL).await;
                    if get_status_attempts > MAX_POLL_VERIFICATION_RESULT_ATTEMPTS {
                        tracing::warn!(
                            "Get verification status timed out for contract with address {:#?}",
                            req.contract_address
                        );
                        return Err(ProcessingError::VerificationStatusPollingTimeout.into());
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(())
    }

    async fn process_next_request(&self) -> anyhow::Result<(), VerifierError> {
        let mut started_at = Instant::now();
        let verification_request = self.get_next_request().await?;
        if verification_request.is_none() {
            return Ok(());
        }

        let verification_request = verification_request.unwrap();
        tracing::info!(
            "Started to process request with id = {}",
            verification_request.id
        );
        let mut verify_attempts = 0;
        let processing_result: Result<(), anyhow::Error> = loop {
            let verification_result = self.verify(verification_request.clone()).await;
            match verification_result {
                Ok(_) => break Ok(()),
                Err(VerifierError::EtherscanError(api_error)) => {
                    let retry_policy = get_api_error_retry_policy(&api_error);
                    match retry_policy {
                        ApiErrorRetryPolicy::NotRetryable => break Err(api_error.into()),
                        ApiErrorRetryPolicy::Retryable {
                            pause_duration,
                            max_attempts,
                        } => {
                            verify_attempts += 1;
                            if verify_attempts > max_attempts {
                                tracing::warn!(
                                        "Number of verification retries has exceed the limit. Failing the job.",
                                    );
                                break Err(api_error.into());
                            }
                            tracing::warn!(
                                "Pausing processing for {:?} due to the error: {:#?}",
                                pause_duration,
                                api_error
                            );
                            tokio::time::sleep(pause_duration).await;
                            self.reset_etherscan_verification_processing_started_at(
                                verification_request.id,
                            )
                            .await?;
                            started_at = Instant::now();
                        }
                        ApiErrorRetryPolicy::IndefinitelyRetryable { pause_duration } => {
                            tracing::warn!(
                                "Pausing processing for {:?} due to the error: {}",
                                pause_duration,
                                api_error
                            );
                            tokio::time::sleep(pause_duration).await;
                            self.reset_etherscan_verification_processing_started_at(
                                verification_request.id,
                            )
                            .await?;
                            started_at = Instant::now();
                        }
                    }
                }
                Err(VerifierError::ProcessingError(processing_error)) => {
                    break Err(processing_error.into())
                }
            }
        };

        match processing_result {
            Ok(_) => {
                self.save_result(verification_request.id).await?;
            }
            Err(err) => {
                self.save_failure(verification_request.id, err.to_string())
                    .await?;
            }
        }

        API_CONTRACT_VERIFIER_METRICS
            .etherscan_request_processing_time
            .observe(started_at.elapsed());

        Ok(())
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down etherscan verifier");
                return Ok(());
            }

            // Errors are handled internally so it's safe to ignore them here
            // to keep the processing loop running.
            self.process_next_request().await.ok();

            let wait_interval = Self::wait_for_request_interval();
            tracing::trace!("Pausing execution for {} sec.", wait_interval.as_secs());
            tokio::time::timeout(wait_interval, stop_receiver.changed())
                .await
                .ok();
        }
    }
}
