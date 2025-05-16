//! Tree API client.

use std::fmt;

use anyhow::Context;
use axum::http::header;
use zk_os_merkle_tree::BatchTreeProof;
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus};
use zksync_types::{L1BatchNumber, H256};

use super::{Problem, TreeProofRequest, TreeProofResponse, PROBLEM_CONTENT_TYPE};
use crate::{health::MerkleTreeInfo, LazyAsyncTreeReader};

/// Client-side tree API error used by [`TreeApiClient`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TreeApiError {
    #[error("missing tree version")]
    NoVersion,
    #[error("tree API is temporarily unavailable")]
    NotReady(#[source] Option<anyhow::Error>),
    /// Catch-all variant for internal errors.
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl TreeApiError {
    fn for_request(err: reqwest::Error, request_description: impl fmt::Display) -> Self {
        let is_not_ready = err.is_timeout() || err.is_connect();
        let err =
            anyhow::Error::new(err).context(format!("failed requesting {request_description}"));
        if is_not_ready {
            Self::NotReady(Some(err))
        } else {
            Self::Internal(err)
        }
    }
}

/// Client accessing Merkle tree API.
#[async_trait]
pub trait TreeApiClient: 'static + Send + Sync + fmt::Debug {
    /// Obtains general information about the tree.
    async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError>;

    /// Obtains proofs for the specified `hashed_keys` at the specified tree version (= L1 batch number).
    async fn get_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<H256>,
    ) -> Result<BatchTreeProof, TreeApiError>;
}

/// In-process API client implementation.
#[async_trait]
impl TreeApiClient for LazyAsyncTreeReader {
    async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError> {
        if let Some(reader) = self.read() {
            reader.info().await.map_err(TreeApiError::Internal)
        } else {
            Err(TreeApiError::NotReady(None))
        }
    }

    async fn get_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<H256>,
    ) -> Result<BatchTreeProof, TreeApiError> {
        if let Some(reader) = self.read() {
            reader
                .prove(l1_batch_number.0.into(), hashed_keys)
                .await
                .map_err(TreeApiError::Internal)
        } else {
            Err(TreeApiError::NotReady(None))
        }
    }
}

/// [`TreeApiClient`] implementation requesting data from a Merkle tree API server.
#[derive(Debug, Clone)]
pub struct TreeApiHttpClient {
    inner: reqwest::Client,
    info_url: String,
    proofs_url: String,
}

impl TreeApiHttpClient {
    /// Creates a new HTTP client with default settings.
    pub fn new(url_base: &str) -> Self {
        Self::from_client(reqwest::Client::new(), url_base)
    }

    /// Wraps a provided HTTP client.
    pub fn from_client(client: reqwest::Client, url_base: &str) -> Self {
        Self {
            inner: client,
            info_url: url_base.to_owned(),
            proofs_url: format!("{url_base}/proofs"),
        }
    }

    #[cfg(test)]
    pub async fn post<R>(
        &self,
        url: &str,
        request: &impl serde::Serialize,
    ) -> Result<R, TreeApiError>
    where
        R: serde::de::DeserializeOwned,
    {
        let response = self
            .inner
            .post(format!("{}{url}", self.info_url))
            .json(request)
            .send()
            .await
            .map_err(|err| TreeApiError::for_request(err, "raw nodes"))?;
        let response = response
            .error_for_status()
            .context("Requesting tree info returned non-OK response")?;
        Ok(response
            .json()
            .await
            .context("Failed deserializing tree info")?)
    }
}

#[async_trait]
impl CheckHealth for TreeApiHttpClient {
    fn name(&self) -> &'static str {
        "tree_api_http_client"
    }

    async fn check_health(&self) -> Health {
        match self.get_info().await {
            Ok(info) => Health::from(HealthStatus::Ready).with_details(info),
            // Tree API is not a critical component, so its errors are not considered fatal for the app health.
            Err(err) => Health::from(HealthStatus::Affected).with_details(serde_json::json!({
                "error": err.to_string(),
                // Retriable error detection is a best-effort estimate
                "is_retriable_error": matches!(err, TreeApiError::NotReady(_)),
            })),
        }
    }
}

#[async_trait]
impl TreeApiClient for TreeApiHttpClient {
    async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError> {
        let response = self
            .inner
            .get(&self.info_url)
            .send()
            .await
            .map_err(|err| TreeApiError::for_request(err, "tree info"))?;
        let response = response
            .error_for_status()
            .context("Requesting tree info returned non-OK response")?;
        Ok(response
            .json()
            .await
            .context("Failed deserializing tree info")?)
    }

    async fn get_proof(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<H256>,
    ) -> Result<BatchTreeProof, TreeApiError> {
        let response = self
            .inner
            .post(&self.proofs_url)
            .json(&TreeProofRequest {
                l1_batch_number,
                hashed_keys,
            })
            .send()
            .await
            .map_err(|err| {
                TreeApiError::for_request(
                    err,
                    format_args!("proofs for L1 batch #{l1_batch_number}"),
                )
            })?;

        let is_problem = response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|header| *header == PROBLEM_CONTENT_TYPE);
        if is_problem {
            // Try to parse `NoVersionError` from the response body.
            let problem: Problem = response
                .json()
                .await
                .context("failed parsing error response")?;
            return Err(if problem.r#type == Problem::NO_TREE_VERSION_TYPE {
                TreeApiError::NoVersion
            } else {
                TreeApiError::Internal(anyhow::anyhow!(
                    "requesting proofs for L1 batch #{l1_batch_number} failed: {problem:?}"
                ))
            });
        }

        let response = response.error_for_status().with_context(|| {
            format!("requesting proofs for L1 batch #{l1_batch_number} returned non-OK response")
        })?;
        let response: TreeProofResponse = response.json().await.with_context(|| {
            format!("failed deserializing proofs for L1 batch #{l1_batch_number}")
        })?;
        Ok(response.proof.into())
    }
}
