use std::{collections::HashMap, time::Duration};

use reqwest::Client;
use serde::Serialize;
use zksync_types::Address;

use super::{
    errors::{is_blocked_by_cloudflare_response, is_cloudflare_security_challenge, ApiError},
    types::{ApiAction, ApiModule, ApiRequest, ApiResponseRaw, EtherscanVerificationRequest},
};

#[derive(Debug, Clone)]
pub(crate) struct EtherscanClient {
    api_url: String,
    api_key: String,
    http_client: Client,
}

fn process_api_call_result(
    result: Result<impl AsRef<str>, reqwest::Error>,
) -> Result<String, ApiError> {
    match result {
        Ok(result) => {
            let result = result.as_ref();
            let raw_response: ApiResponseRaw = serde_json::from_str(result).map_err(|error| {
                if result.contains("Page not found") {
                    ApiError::PageNotFound
                } else if is_blocked_by_cloudflare_response(result) {
                    ApiError::BlockedByCloudflare
                } else if is_cloudflare_security_challenge(result) {
                    ApiError::CloudFlareSecurityChallenge
                } else {
                    ApiError::Serde {
                        error,
                        content: result.to_string(),
                    }
                }
            })?;

            match raw_response.status.as_str() {
                "0" => {
                    if raw_response.result == "Contract source code not verified" {
                        return Err(ApiError::ContractNotVerified);
                    }
                    if raw_response.result == "Contract source code already verified" {
                        return Err(ApiError::ContractAlreadyVerified);
                    }
                    if raw_response.result == "Pending in queue" {
                        return Err(ApiError::VerificationPending);
                    }
                    // There is a number of daily limit in between the checked values.
                    // I don't want to rely on the exact number as it is a subject to change.
                    if raw_response.result.starts_with("Daily limit")
                        && raw_response
                            .result
                            .ends_with("source code submissions reached")
                    {
                        return Err(ApiError::DailyVerificationRequestsLimitExceeded);
                    }
                    let result_lower = raw_response.result.to_lowercase();
                    if result_lower.contains("rate limit reached") {
                        return Err(ApiError::RateLimitExceeded);
                    }
                    // Error message can be "Invalid API Key" or "Missing/Invalid API Key"
                    if result_lower.contains("invalid api key") {
                        return Err(ApiError::InvalidApiKey);
                    }
                    Err(ApiError::ErrorResponse {
                        message: raw_response.message,
                        result: raw_response.result,
                    })
                }
                "1" => Ok(raw_response.result),
                _ => Err(ApiError::UnexpectedResponse {
                    message: raw_response.message,
                    result: raw_response.result,
                }),
            }
        }
        Err(err) => Err(ApiError::Reqwest(err)),
    }
}

impl EtherscanClient {
    /// Creates a new EtherscanClient instance.
    ///
    /// # Arguments
    /// - `api_url` - URL of the Etherscan API.
    /// - `api_key` - API key for the Etherscan API.
    /// - `connect_timeout_secs` - Timeout for establishing connection in seconds.
    /// - `request_timeout_secs` - Total timeout for the client request in seconds.
    pub fn new(
        api_url: String,
        api_key: String,
        connect_timeout_secs: u64,
        request_timeout_secs: u64,
    ) -> Self {
        let client = Client::builder()
            // Timeout for establishing connection
            .connect_timeout(Duration::from_secs(connect_timeout_secs))
            // Total timeout for the request
            .timeout(Duration::from_secs(request_timeout_secs))
            .build()
            .expect("Failed to create a reqwest client for EtherscanClient");

        Self {
            api_url,
            api_key,
            http_client: client,
        }
    }

    async fn post<'a, T>(
        &self,
        module: ApiModule,
        action: ApiAction,
        form: T,
    ) -> Result<String, ApiError>
    where
        T: Serialize,
    {
        let payload = ApiRequest::new(&self.api_key, module, action, form);
        let response_result = self
            .http_client
            .post(&self.api_url)
            .form(&payload)
            .send()
            .await?
            .text()
            .await;
        process_api_call_result(response_result)
    }

    async fn get<T>(
        &self,
        module: ApiModule,
        action: ApiAction,
        query: T,
    ) -> Result<String, ApiError>
    where
        T: Serialize,
    {
        let query = ApiRequest::new(&self.api_key, module, action, query);
        let response_result = self
            .http_client
            .get(&self.api_url)
            .query(&query)
            .send()
            .await?
            .text()
            .await;
        process_api_call_result(response_result)
    }

    pub async fn is_contract_verified(&self, contract_address: Address) -> Result<bool, ApiError> {
        let response = self
            .get(
                ApiModule::Contract,
                ApiAction::GetAbi,
                HashMap::from([("address", contract_address)]),
            )
            .await;

        match response {
            Ok(_) => Ok(true),
            Err(ApiError::ContractNotVerified) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn get_verification_status(&self, verification_id: &str) -> Result<String, ApiError> {
        self.get(
            ApiModule::Contract,
            ApiAction::GetVerificationStatus,
            HashMap::from([("guid", verification_id)]),
        )
        .await
    }

    pub async fn verify(&self, request: EtherscanVerificationRequest) -> Result<String, ApiError> {
        self.post(ApiModule::Contract, ApiAction::VerifySourceCode, request)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_api_call_result_success_response() {
        let input = r#"{"status": "1", "message": "OK", "result": "success_result"}"#;
        let result = process_api_call_result(Ok(input));
        assert_eq!(result.unwrap(), "success_result");
    }

    #[test]
    fn test_process_api_call_result_contract_not_verified() {
        let input =
            r#"{"status": "0", "message": "Error", "result": "Contract source code not verified"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::ContractNotVerified) => {}
            _ => panic!("Expected ContractNotVerified error"),
        }
    }

    #[test]
    fn test_process_api_call_result_contract_already_verified() {
        let input = r#"{"status": "0", "message": "Error", "result": "Contract source code already verified"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::ContractAlreadyVerified) => {}
            _ => panic!("Expected ContractAlreadyVerified error"),
        }
    }

    #[test]
    fn test_process_api_call_result_verification_pending() {
        let input = r#"{"status": "0", "message": "Error", "result": "Pending in queue"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::VerificationPending) => {}
            _ => panic!("Expected VerificationPending error"),
        }
    }

    #[test]
    fn test_process_api_call_result_daily_limit_exceeded() {
        let input = r#"{"status": "0", "message": "Error", "result": "Daily limit of 100 source code submissions reached"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::DailyVerificationRequestsLimitExceeded) => {}
            _ => panic!("Expected DailyVerificationRequestsLimitExceeded error"),
        }
    }

    #[test]
    fn test_process_api_call_result_rate_limit_exceeded() {
        let input = r#"{"status": "0", "message": "Error", "result": "Max rate limit reached"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::RateLimitExceeded) => {}
            _ => panic!("Expected RateLimitExceeded error"),
        }
    }

    #[test]
    fn test_process_api_call_result_invalid_api_key() {
        let input = r#"{"status": "0", "message": "Error", "result": "Missing/Invalid API Key"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::InvalidApiKey) => {}
            _ => panic!("Expected InvalidApiKey error"),
        }

        let input = r#"{"status": "0", "message": "Error", "result": "Invalid API Key"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::InvalidApiKey) => {}
            _ => panic!("Expected InvalidApiKey error"),
        }
    }

    #[test]
    fn test_process_api_call_result_unexpected_status() {
        let input = r#"{"status": "2", "message": "Unexpected", "result": "Some result"}"#;
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::UnexpectedResponse {
                message,
                result: res,
            }) => {
                assert_eq!(message, "Unexpected");
                assert_eq!(res, "Some result");
            }
            _ => panic!("Expected UnexpectedResponse error"),
        }
    }

    #[test]
    fn test_process_api_call_result_page_not_found() {
        let input = "Page not found";
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::PageNotFound) => {}
            _ => panic!("Expected PageNotFound error"),
        }
    }

    #[test]
    fn test_process_api_call_result_blocked_by_cloudflare() {
        let input = "Sorry, you have been blocked by Cloudflare";
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::BlockedByCloudflare) => {}
            _ => panic!("Expected BlockedByCloudflare error"),
        }
    }

    #[test]
    fn test_process_api_call_result_cloudflare_security_challenge() {
        let input = "Security challenge link https://www.cloudflare.com?utm_source=challenge";
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::CloudFlareSecurityChallenge) => {}
            _ => panic!("Expected CloudFlareSecurityChallenge error"),
        }

        let input = "Checking if the site connection is secure...";
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::CloudFlareSecurityChallenge) => {}
            _ => panic!("Expected CloudFlareSecurityChallenge error"),
        }
    }

    #[test]
    fn test_invalid_json_without_special_message() {
        let input = "this is not a valid json";
        let result = process_api_call_result(Ok(input));
        match result {
            Err(ApiError::Serde { content, .. }) => {
                assert_eq!(content, input.to_string());
            }
            _ => panic!("Expected Serde error"),
        }
    }
}
