use std::{collections::HashMap, time::Duration};

use reqwest::Client;
use secrecy::ExposeSecret;
use serde::{de::DeserializeOwned, Serialize};
use zksync_types::{secrets::APIKey, Address};

use super::{
    errors::EtherscanError,
    types::{
        EtherscanAction, EtherscanModule, EtherscanRequest, EtherscanVerificationRequest,
        RawEtherscanResponse,
    },
};

#[derive(Debug, Clone)]
pub(super) struct EtherscanClient {
    api_url: String,
    api_key: APIKey,
    http_client: Client,
}

async fn extract_result<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, EtherscanError> {
    if response.status() == 404 {
        return Err(EtherscanError::PageNotFound);
    }
    if response.status() == 403 {
        return Err(EtherscanError::BlockedByCloudflare);
    }

    if let Some(value) = response.headers().get("cf-mitigated") {
        if value == "challenge" {
            return Err(EtherscanError::CloudflareSecurityChallenge);
        }
    }

    let raw_response: RawEtherscanResponse = response.json().await?;

    if raw_response.status == "0" {
        return Err(raw_response.error_message());
    }
    if raw_response.status == "1" {
        return raw_response.deserialize_result();
    }

    Err(EtherscanError::unexpected_response(
        raw_response.message,
        raw_response.status,
    ))
}

impl EtherscanClient {
    /// Creates a new EtherscanClient instance.
    ///
    /// # Arguments
    /// - `api_url` - URL of the Etherscan API.
    /// - `api_key` - API key for the Etherscan API.
    pub(super) fn new(api_url: String, api_key: APIKey) -> Self {
        let client = Client::builder()
            // Timeout for establishing connection
            .connect_timeout(Self::connect_timeout())
            // Total timeout for the request
            .timeout(Self::request_timeout())
            .build()
            .expect("Failed to create a reqwest client for EtherscanClient");

        Self {
            api_url,
            api_key,
            http_client: client,
        }
    }

    fn connect_timeout() -> Duration {
        Duration::from_secs(10)
    }

    fn request_timeout() -> Duration {
        Duration::from_secs(60)
    }

    async fn post<T>(
        &self,
        module: EtherscanModule,
        action: EtherscanAction,
        form: T,
    ) -> Result<String, EtherscanError>
    where
        T: Serialize,
    {
        let payload = EtherscanRequest::new(self.api_key.0.expose_secret(), module, action, form);
        let response_result = self
            .http_client
            .post(&self.api_url)
            .form(&payload)
            .send()
            .await?;

        extract_result(response_result).await
    }

    async fn get<T>(
        &self,
        module: EtherscanModule,
        action: EtherscanAction,
        query: T,
    ) -> Result<String, EtherscanError>
    where
        T: Serialize,
    {
        let query = EtherscanRequest::new(self.api_key.0.expose_secret(), module, action, query);
        let response_result = self
            .http_client
            .get(&self.api_url)
            .query(&query)
            .send()
            .await?;
        extract_result(response_result).await
    }

    pub async fn is_contract_verified(
        &self,
        contract_address: Address,
    ) -> Result<bool, EtherscanError> {
        let response = self
            .get(
                EtherscanModule::Contract,
                EtherscanAction::GetAbi,
                HashMap::from([("address", contract_address)]),
            )
            .await;

        match response {
            Ok(_) => Ok(true),
            Err(EtherscanError::ContractNotVerified) => Ok(false),
            Err(err) => Err(err),
        }
    }

    pub async fn get_verification_status(
        &self,
        verification_id: &str,
    ) -> Result<String, EtherscanError> {
        self.get(
            EtherscanModule::Contract,
            EtherscanAction::GetVerificationStatus,
            HashMap::from([("guid", verification_id)]),
        )
        .await
    }

    pub async fn verify(
        &self,
        request: EtherscanVerificationRequest,
    ) -> Result<String, EtherscanError> {
        self.post(
            EtherscanModule::Contract,
            EtherscanAction::VerifySourceCode,
            request,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use http::{header::HeaderMap, Response, StatusCode};
    use serde_json::json;

    use super::*;

    fn create_response(status: StatusCode, headers: HeaderMap, body: String) -> reqwest::Response {
        let response = Response::builder().status(status).body(body).unwrap();
        let mut response: reqwest::Response = response.into();
        *response.headers_mut() = headers;
        response
    }

    #[tokio::test]
    async fn test_extract_result_404_status() {
        let response = create_response(StatusCode::NOT_FOUND, HeaderMap::new(), "".to_string());
        let result: Result<String, _> = extract_result(response).await;
        assert!(matches!(result, Err(EtherscanError::PageNotFound)));
    }

    #[tokio::test]
    async fn test_extract_result_403_status() {
        let response = create_response(StatusCode::FORBIDDEN, HeaderMap::new(), "".to_string());
        let result: Result<String, _> = extract_result(response).await;
        assert!(matches!(result, Err(EtherscanError::BlockedByCloudflare)));
    }

    #[tokio::test]
    async fn test_extract_result_cloudflare_challenge() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-mitigated", "challenge".parse().unwrap());
        let response = create_response(StatusCode::OK, headers, "".to_string());
        let result: Result<String, _> = extract_result(response).await;
        assert!(matches!(
            result,
            Err(EtherscanError::CloudflareSecurityChallenge)
        ));
    }

    #[tokio::test]
    async fn test_extract_result_successful_response() {
        let body = json!({
            "status": "1",
            "message": "OK",
            "result": "success"
        })
        .to_string();
        let response = create_response(StatusCode::OK, HeaderMap::new(), body);
        let result: Result<String, _> = extract_result(response).await;
        assert_eq!(result.unwrap(), "success");
    }

    #[tokio::test]
    async fn test_extract_result_error_response() {
        let body = json!({
            "status": "0",
            "message": "Error",
            "result": "Contract source code not verified"
        })
        .to_string();
        let response = create_response(StatusCode::OK, HeaderMap::new(), body);
        let result: Result<String, _> = extract_result(response).await;
        assert!(matches!(result, Err(EtherscanError::ContractNotVerified)));
    }

    #[tokio::test]
    async fn test_extract_result_unexpected_status() {
        let body = json!({
            "status": "2",
            "message": "Unexpected",
            "result": "Some result"
        })
        .to_string();
        let response = create_response(StatusCode::OK, HeaderMap::new(), body);
        let result: Result<String, _> = extract_result(response).await;
        assert!(matches!(
            result,
            Err(EtherscanError::UnexpectedResponse { .. })
        ));
    }
}
