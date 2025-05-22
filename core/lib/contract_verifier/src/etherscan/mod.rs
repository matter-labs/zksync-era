//! Etherscan verifier sends the contract verification requests that have been successfully verified by
//! ZKSync Contract Verifier to Etherscan.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use client::EtherscanClient;
use errors::{EtherscanError, ProcessingError, VerifierError};
use solc_versions_fetcher::SolcVersionsFetcher;
use tokio::sync::watch;
use types::EtherscanVerificationRequest;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::{
    contract_verification::{
        api::{SourceCodeData, VerificationRequest},
        etherscan::EtherscanVerification,
    },
    secrets::APIKey,
};

use crate::metrics::API_CONTRACT_VERIFIER_METRICS;

pub mod client;
pub mod errors;
pub mod metrics;
mod solc_versions_fetcher;
pub mod types;
pub mod utils;

enum ApiErrorRetryPolicy {
    NotRetryable,
    Retryable { pause_duration: Duration },
    IndefinitelyRetryable { pause_duration: Duration },
}

/// Returns true if the verification request is supported by the Etherscan verifier, otherwise false.
///
/// Currently, the verifier supports only Solidity single file and standard JSON input source code data formats.
pub fn is_supported_verification_request(request: &VerificationRequest) -> bool {
    matches!(
        request.req.source_code_data,
        SourceCodeData::SolSingleFile(_) | SourceCodeData::StandardJsonInput(_)
    )
}

fn get_api_error_retry_policy(api_error: &EtherscanError, attempt: i32) -> ApiErrorRetryPolicy {
    match api_error {
        EtherscanError::DailyVerificationRequestsLimitExceeded => {
            // The number of sent verification requests over the last 24 hours is not tracked so there is no precise
            // way to determine the proper pause duration. 1 hour pause should be good enough, but if necessary more
            // sophisticated solution can be implemented.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 60), // 1 hour
            }
        }
        EtherscanError::InvalidApiKey => {
            // Etherscan API key is invalid even though it has no expiration date. Either Etherscan API is experiencing
            // issues or the key was revoked. 1 hour to give the API time to recover and not to spam the API with
            // invalid requests. Since Etherscan verification is completely async it shouldn't affect users much.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 60), // 1 hour
            }
        }
        EtherscanError::BlockedByCloudflare
        | EtherscanError::CloudflareSecurityChallenge
        | EtherscanError::PageNotFound => {
            // The request was blocked by the Cloudflare or a blank page was returned by the API, both of which are
            // unexpected. Likely Etherscan is experiencing issues.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(60 * 10), // 10 mins
            }
        }
        EtherscanError::RateLimitExceeded => {
            // Etherscan has different rate limits. Verification requests limit is handled separately. Apart from that
            // there are a per second and a per day limits. We are very unlikely to hit a daily limit. For the per
            // second limit, we can pause for 5 sec.
            ApiErrorRetryPolicy::IndefinitelyRetryable {
                pause_duration: Duration::from_secs(5),
            }
        }
        EtherscanError::ContractBytecodeNotAvailable => {
            let max_delay = 1440; // 24 hours
            let min_delay = 2;
            // Growing delay for each attempt. First attempt is in 2 minutes, last one is limited with maximum delay
            // to 1 day.
            let delay_mins = min_delay * 2_u64.pow(attempt.try_into().unwrap_or(0)).min(max_delay);

            // This error happens when the contract is not yet indexed by Etherscan.
            ApiErrorRetryPolicy::Retryable {
                pause_duration: Duration::from_secs(delay_mins * 60),
            }
        }
        EtherscanError::Reqwest(_) => {
            // Most likely the error is caused by the network issue of some sort. But if the request error is caused by
            // the constructed payload, we can't afford to retry indefinitely, so we number of attempts is limited.
            ApiErrorRetryPolicy::Retryable {
                pause_duration: Duration::from_secs(60), // 1 min
            }
        }
        _ => ApiErrorRetryPolicy::NotRetryable,
    }
}

const POLL_VERIFICATION_RESULT_INTERVAL: Duration = Duration::from_secs(5);
const MAX_POLL_VERIFICATION_RESULT_ATTEMPTS: u32 = 100;
const MAX_ATTEMPTS: i32 = 10;

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
        api_key: APIKey,
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

    async fn sleep(&mut self, time: Duration) -> Result<(), VerifierError> {
        if tokio::time::timeout(time, self.stop_receiver.changed())
            .await
            .is_ok()
        {
            Err(VerifierError::Canceled)
        } else {
            Ok(())
        }
    }

    async fn get_next_request(
        &mut self,
    ) -> anyhow::Result<Option<(VerificationRequest, EtherscanVerification)>, VerifierError> {
        // We consider that the job is stuck if it's being processed for more than 2 hours. We need large overhead
        // because Etherscan has pretty strict daily limits for verification requests. In case the daily verification
        // rate limit is reached, the processing is paused for 1 hour and the processing_started_at field is reset to
        // the current time after the pause. Since etherscan verification is completely async it shouldn't affect user
        // experience much.
        let verification_task = {
            const TIME_OVERHEAD: Duration = Duration::from_secs(2 * 60 * 60); // 2 hours
            let mut connection = self
                .connection_pool
                .connection_tagged("etherscan_verifier")
                .await?;

            connection
                .etherscan_verification_dal()
                .get_next_queued_verification_request(TIME_OVERHEAD)
                .await?
        };

        Ok(verification_task)
    }

    async fn save_result(&self, request_id: usize) -> anyhow::Result<(), VerifierError> {
        tracing::info!(request_id, "Etherscan verification succeeded");
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .save_verification_success(request_id)
            .await?;

        API_CONTRACT_VERIFIER_METRICS.successful_verifications[&"etherscan_verifier"].inc();

        Ok(())
    }

    async fn save_failure(
        &self,
        request_id: usize,
        error: String,
    ) -> anyhow::Result<(), VerifierError> {
        tracing::error!(request_id, "Etherscan verification failed");
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .save_verification_failure(request_id, &error)
            .await?;

        API_CONTRACT_VERIFIER_METRICS.failed_verifications[&"etherscan_verifier"].inc();

        Ok(())
    }

    async fn save_for_retry(
        &self,
        request_id: usize,
        attempts: i32,
        retry_at: DateTime<Utc>,
    ) -> anyhow::Result<(), VerifierError> {
        let mut connection = self
            .connection_pool
            .connection_tagged("etherscan_verifier")
            .await?;

        connection
            .etherscan_verification_dal()
            .save_for_retry(request_id, attempts, retry_at)
            .await?;

        Ok(())
    }

    async fn send_verification_request(
        &self,
        verification_request: &VerificationRequest,
    ) -> Result<Option<String>, VerifierError> {
        let req = &verification_request.req;
        // We need to check if the contract is verified before submitting a verification request, because there is a
        // hard per-day limit on request submissions (including already verified contracts).
        let is_verified = self
            .client
            .is_contract_verified(req.contract_address)
            .await?;

        if is_verified {
            tracing::info!(
                request_id = verification_request.id,
                address = format!("{:#?}", req.contract_address),
                "Contract is already verified",
            );
            return Ok(None);
        }

        tracing::info!(
            request_id = verification_request.id,
            address = format!("{:#?}", req.contract_address),
            "Sending verification request to Etherscan",
        );
        let request_id = match self
            .client
            .verify(EtherscanVerificationRequest::from_verification_request(
                req.clone(),
                &self.solc_versions_fetcher,
            )?)
            .await
        {
            Ok(id) => id,
            // Even though there is a call to check if the contract is already verified we still need to process
            // ContractAlreadyVerified response from the verification API call. This is because the contract can be
            // verified by another party.
            Err(EtherscanError::ContractAlreadyVerified) => {
                tracing::info!(
                    request_id = verification_request.id,
                    address = format!("{:#?}", req.contract_address),
                    "Contract is already verified",
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
        for attempt in 1..=MAX_POLL_VERIFICATION_RESULT_ATTEMPTS {
            tracing::info!(
                request_id = verification_request.id,
                address = format!("{:#?}", verification_request.req.contract_address),
                "Fetching contract verification status, attempt = {}",
                attempt
            );
            // Verification info won't be available immediately, so wait a bit before polling
            self.sleep(POLL_VERIFICATION_RESULT_INTERVAL).await?;
            match self.client.get_verification_status(&verification_id).await {
                Ok(_) | Err(EtherscanError::ContractAlreadyVerified) => return Ok(()),
                Err(EtherscanError::VerificationPending) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        Err(ProcessingError::VerificationStatusPollingTimeout.into())
    }

    async fn verify(
        &mut self,
        verification_request: VerificationRequest,
        etherscan_verification_id: Option<String>,
    ) -> Result<(), VerifierError> {
        let verification_id = match etherscan_verification_id {
            Some(id) => id,
            None => {
                let Some(request_id) = self
                    .send_verification_request(&verification_request)
                    .await?
                else {
                    // Contract is already verified.
                    return Ok(());
                };
                request_id
            }
        };

        self.await_verification(&verification_request, verification_id)
            .await?;

        Ok(())
    }

    async fn process_next_request(&mut self) -> anyhow::Result<(), VerifierError> {
        let Some((verification_request, verification)) = self.get_next_request().await? else {
            return Ok(());
        };
        let started_at = Instant::now();
        tracing::info!(
            request_id = verification_request.id,
            "Started to process the request",
        );
        let verification_result = self
            .verify(
                verification_request.clone(),
                verification.etherscan_verification_id,
            )
            .await;

        if let Err(err) = &verification_result {
            tracing::error!(
                request_id = verification_request.id,
                "Error while processing the request: {}",
                err
            );
        }

        match verification_result {
            Ok(_) => {
                self.save_result(verification_request.id).await?;
                API_CONTRACT_VERIFIER_METRICS
                    .etherscan_request_processing_time
                    .observe(started_at.elapsed());
            }
            Err(VerifierError::EtherscanError(api_error)) => {
                let retry_policy = get_api_error_retry_policy(&api_error, verification.attempts);
                match retry_policy {
                    ApiErrorRetryPolicy::NotRetryable => {
                        self.save_failure(verification_request.id, api_error.to_string())
                            .await?;
                    }
                    ApiErrorRetryPolicy::Retryable { .. }
                        if verification.attempts == MAX_ATTEMPTS =>
                    {
                        self.save_failure(verification_request.id, api_error.to_string())
                            .await?;
                    }
                    ApiErrorRetryPolicy::Retryable { pause_duration } => {
                        self.save_for_retry(
                            verification_request.id,
                            // Increase the number of attempts so if the number of attempts is exceeded max attempts
                            // number, the request will be marked as failed.
                            verification.attempts + 1,
                            Utc::now() + pause_duration,
                        )
                        .await?;
                    }
                    ApiErrorRetryPolicy::IndefinitelyRetryable { pause_duration } => {
                        self.save_for_retry(
                            verification_request.id,
                            // Do not increase the number of attempts so the request will be retried indefinitely.
                            verification.attempts,
                            Utc::now() + pause_duration,
                        )
                        .await?;
                    }
                }
            }
            Err(VerifierError::ProcessingError(processing_error)) => {
                self.save_failure(verification_request.id, processing_error.to_string())
                    .await?;
            }
            Err(VerifierError::Canceled) => return Err(VerifierError::Canceled),
        }

        Ok(())
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        loop {
            if *self.stop_receiver.borrow() {
                tracing::warn!("Stop request received, shutting down etherscan verifier");
                return Ok(());
            }

            // Update solc long versions, required for Etherscan verification. Internally cached, so it's fine to call
            // it on every iteration.
            if let Err(err) = self.solc_versions_fetcher.update_versions().await {
                tracing::error!("Failed to update solc versions: {}", err);
            }

            // VerifierError::Canceled happens when a stop request is received. Other errors are handled internally so
            // it's safe to ignore them to keep the processing loop running.
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

#[cfg(test)]
mod tests {
    use serde_json;
    use zksync_types::{
        contract_verification::api::{
            CompilerVersions, SourceCodeData, VerificationEvmSettings, VerificationIncomingRequest,
        },
        web3::Bytes,
        Address,
    };

    use super::*;

    fn create_test_request(source_code_data: SourceCodeData) -> VerificationRequest {
        VerificationRequest {
            id: 1,
            req: VerificationIncomingRequest {
                contract_address: Address::default(),
                contract_name: "MyContract".to_string(),
                source_code_data,
                compiler_versions: CompilerVersions::Solc {
                    compiler_zksolc_version: None,
                    compiler_solc_version: "0.8.16".to_string(),
                },
                optimization_used: true,
                optimizer_mode: Some("3".to_string()),
                constructor_arguments: Bytes::from(vec![0xde, 0xad, 0xbe, 0xef]),
                is_system: true,
                force_evmla: true,
                evm_specific: VerificationEvmSettings {
                    evm_version: Some("evm version".to_string()),
                    optimizer_runs: Some(200),
                },
            },
        }
    }

    #[test]
    fn test_supported_verification_requests() {
        let request = create_test_request(SourceCodeData::SolSingleFile(
            "contract Test {}".to_string(),
        ));
        assert!(is_supported_verification_request(&request));

        let request =
            create_test_request(SourceCodeData::StandardJsonInput(serde_json::Map::new()));
        assert!(is_supported_verification_request(&request));
    }

    #[test]
    fn test_unsupported_verification_requests() {
        let request = create_test_request(SourceCodeData::YulSingleFile("code".to_string()));
        assert!(!is_supported_verification_request(&request));
    }
}
