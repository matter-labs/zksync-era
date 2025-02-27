//! Etherscan verifier sends the contract verification requests that have been successfully
//! verified by ZKSync Contract Verifier to Etherscan.

use std::time::{Duration, Instant};

use client::EtherscanClient;
use errors::{EtherscanError, ProcessingError, VerifierError};
use solc_versions_fetcher::SolcVersionsFetcher;
use tokio::sync::watch;
use types::EtherscanVerificationRequest;
use zksync_dal::{
    etherscan_verification_dal::EtherscanVerificationJobResultStatus, ConnectionPool, Core, CoreDal,
};
use zksync_types::contract_verification::api::VerificationRequest;

use crate::metrics::API_CONTRACT_VERIFIER_METRICS;

pub mod client;
pub mod errors;
mod solc_versions_fetcher;
pub mod types;
pub mod utils;

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

const POLL_VERIFICATION_RESULT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_POLL_VERIFICATION_RESULT_ATTEMPTS: u32 = 100;

#[derive(Debug, Clone)]
pub struct EtherscanVerifier {
    client: EtherscanClient,
    connection_pool: ConnectionPool<Core>,
    solc_versions_fetcher: SolcVersionsFetcher,
    stop_receiver: watch::Receiver<bool>,
}

impl EtherscanVerifier {
    pub fn new(
        api_url: String,
        api_key: String,
        connection_pool: ConnectionPool<Core>,
        stop_receiver: watch::Receiver<bool>,
    ) -> Self {
        Self {
            client: EtherscanClient::new(api_url, api_key),
            connection_pool,
            solc_versions_fetcher: SolcVersionsFetcher::new(),
            stop_receiver,
        }
    }

    fn wait_for_request_interval() -> Duration {
        Duration::from_secs(1)
    }

    async fn get_next_request(
        &self,
    ) -> anyhow::Result<Option<(VerificationRequest, Option<String>)>, VerifierError> {
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

    async fn send_verification_request(
        &self,
        verification_request: &VerificationRequest,
    ) -> Result<Option<String>, VerifierError> {
        let req = verification_request.req.clone();
        tracing::info!(
            "Sending verification request to Etherscan, address = {:#?}",
            req.contract_address,
        );
        let request_id = match self
            .client
            .verify(EtherscanVerificationRequest::from_verification_request(
                req.clone(),
                &self.solc_versions_fetcher,
            ))
            .await
        {
            Ok(id) => id,
            // Even though there is a call to check if the contract is already verified
            // we still need to process ContractAlreadyVerified response from the verification
            // API call. This is because the contract can be verified by another party.
            Err(EtherscanError::ContractAlreadyVerified) => {
                tracing::info!(
                    "Contract with address {:#?} is already verified",
                    req.contract_address
                );
                return Ok(None);
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

        Ok(Some(request_id))
    }

    async fn await_verification(
        &mut self,
        verification_request: &VerificationRequest,
        verification_id: String,
    ) -> Result<(), VerifierError> {
        let mut get_status_attempts = 0;
        loop {
            get_status_attempts += 1;
            tracing::info!(
                "Fetching contract verification status, address = {:#?}, attempt = {}",
                verification_request.req.contract_address,
                get_status_attempts
            );
            let result = self.client.get_verification_status(&verification_id).await;
            match result {
                Ok(_) => break,
                Err(EtherscanError::VerificationPending) => {
                    if tokio::time::timeout(
                        POLL_VERIFICATION_RESULT_INTERVAL,
                        self.stop_receiver.changed(),
                    )
                    .await
                    .is_ok()
                    {
                        return Err(VerifierError::Canceled);
                    }
                    if get_status_attempts > MAX_POLL_VERIFICATION_RESULT_ATTEMPTS {
                        tracing::warn!(
                            "Get verification status timed out for contract with address {:#?}",
                            verification_request.req.contract_address
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

    async fn verify(
        &mut self,
        verification_request: VerificationRequest,
        etherscan_verification_id: Option<String>,
    ) -> Result<(), VerifierError> {
        let req = verification_request.req.clone();
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
        let verification_id = match etherscan_verification_id {
            Some(id) => Some(id),
            None => {
                let request_id = self
                    .send_verification_request(&verification_request)
                    .await?;
                // Wait for the verification to be processed
                if tokio::time::timeout(
                    POLL_VERIFICATION_RESULT_INTERVAL,
                    self.stop_receiver.changed(),
                )
                .await
                .is_ok()
                {
                    return Err(VerifierError::Canceled);
                }
                request_id
            }
        };

        // None verification id means that the contract is already verified.
        if verification_id.is_some() {
            self.await_verification(&verification_request, verification_id.unwrap())
                .await?;
        }

        Ok(())
    }

    async fn process_next_request(&mut self) -> anyhow::Result<(), VerifierError> {
        let mut started_at = Instant::now();
        let verification_request = self.get_next_request().await?;
        if verification_request.is_none() {
            return Ok(());
        }

        let (verification_request, etherscan_verification_id) = verification_request.unwrap();
        tracing::info!(
            "Started to process request with id = {}",
            verification_request.id
        );
        let mut verify_attempts = 0;
        let processing_result: Result<(), VerifierError> = loop {
            let verification_result = self
                .verify(
                    verification_request.clone(),
                    etherscan_verification_id.clone(),
                )
                .await;
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
                            if tokio::time::timeout(pause_duration, self.stop_receiver.changed())
                                .await
                                .is_ok()
                            {
                                break Err(VerifierError::Canceled);
                            }
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
                            if tokio::time::timeout(pause_duration, self.stop_receiver.changed())
                                .await
                                .is_ok()
                            {
                                break Err(VerifierError::Canceled);
                            }
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
                Err(VerifierError::Canceled) => break Err(VerifierError::Canceled),
            }
        };

        match processing_result {
            Ok(_) => {
                self.save_result(verification_request.id).await?;
            }
            Err(VerifierError::Canceled) => {
                return Err(VerifierError::Canceled);
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

    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            if *self.stop_receiver.borrow() {
                tracing::warn!("Stop signal received, shutting down etherscan verifier");
                return Ok(());
            }

            // Update solc long versions, required for Etherscan verification.
            // Internally cached, so it's fine to call it on every iteration.
            if let Err(err) = self.solc_versions_fetcher.update_versions().await {
                tracing::error!("Failed to update solc versions: {}", err);
            }

            // Errors are handled internally so it's safe to ignore them here
            // to keep the processing loop running.
            if let Err(VerifierError::Canceled) = self.process_next_request().await {
                continue;
            }

            let wait_interval = Self::wait_for_request_interval();
            tracing::trace!("Pausing execution for {} sec.", wait_interval.as_secs());
            tokio::time::timeout(wait_interval, self.stop_receiver.changed())
                .await
                .ok();
        }
    }
}
