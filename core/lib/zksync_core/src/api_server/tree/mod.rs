//! Primitive Merkle tree API used internally to fetch proofs.

use anyhow::Context as _;
use async_trait::async_trait;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use std::{fmt, future::Future, net::SocketAddr, pin::Pin};

use zksync_merkle_tree::NoVersionError;
use zksync_types::{L1BatchNumber, H256, U256};

mod metrics;
#[cfg(test)]
mod tests;

use self::metrics::{MerkleTreeApiMethod, API_METRICS};
use crate::metadata_calculator::{AsyncTreeReader, MerkleTreeInfo};

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofsRequest {
    l1_batch_number: L1BatchNumber,
    hashed_keys: Vec<U256>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofsResponse {
    entries: Vec<TreeEntryWithProof>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TreeEntryWithProof {
    #[serde(default, skip_serializing_if = "H256::is_zero")]
    pub value: H256,
    #[serde(default, skip_serializing_if = "TreeEntryWithProof::is_zero")]
    pub index: u64,
    pub merkle_path: Vec<H256>,
}

impl TreeEntryWithProof {
    fn is_zero(&value: &u64) -> bool {
        value == 0
    }
}

impl TreeEntryWithProof {
    fn new(src: zksync_merkle_tree::TreeEntryWithProof) -> Self {
        let mut merkle_path = src.merkle_path;
        merkle_path.reverse(); // Use root-to-leaf enumeration direction as in Ethereum
        Self {
            value: src.base.value_hash,
            index: src.base.leaf_index,
            merkle_path,
        }
    }
}

#[derive(Debug)]
enum TreeApiError {
    NoTreeVersion(NoVersionError),
}

impl IntoResponse for TreeApiError {
    fn into_response(self) -> Response {
        let (status, title, detail) = match self {
            Self::NoTreeVersion(err) => {
                (StatusCode::NOT_FOUND, "L1 batch not found", err.to_string())
            }
        };

        // Loosely conforms to HTTP Problem Details RFC: https://datatracker.ietf.org/doc/html/rfc7807
        let body = serde_json::json!({
            "type": "/errors#l1-batch-not-found",
            "title": title,
            "detail": detail,
        });
        let headers = [(header::CONTENT_TYPE, "application/problem+json")];
        (status, headers, Json(body)).into_response()
    }
}

/// Client accessing Merkle tree API.
#[async_trait]
pub(crate) trait TreeApiClient {
    /// Obtains general information about the tree.
    async fn get_info(&self) -> anyhow::Result<MerkleTreeInfo>;

    /// Obtains proofs for the specified `hashed_keys` at the specified tree version (= L1 batch number).
    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> anyhow::Result<Vec<TreeEntryWithProof>>;
}

/// In-memory client implementation.
#[async_trait]
impl TreeApiClient for AsyncTreeReader {
    async fn get_info(&self) -> anyhow::Result<MerkleTreeInfo> {
        Ok(self.clone().info().await)
    }

    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> anyhow::Result<Vec<TreeEntryWithProof>> {
        self.get_proofs_inner(l1_batch_number, hashed_keys)
            .await
            .map_err(Into::into)
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
    pub fn new(url_base: &str) -> Self {
        Self {
            inner: reqwest::Client::new(),
            info_url: url_base.to_owned(),
            proofs_url: format!("{url_base}/proofs"),
        }
    }
}

#[async_trait]
impl TreeApiClient for TreeApiHttpClient {
    async fn get_info(&self) -> anyhow::Result<MerkleTreeInfo> {
        let response = self
            .inner
            .get(&self.info_url)
            .send()
            .await
            .context("Failed requesting tree info")?;
        let response = response
            .error_for_status()
            .context("Requesting tree info returned non-OK response")?;
        response
            .json()
            .await
            .context("Failed deserializing tree info")
    }

    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> anyhow::Result<Vec<TreeEntryWithProof>> {
        let response = self
            .inner
            .post(&self.proofs_url)
            .json(&TreeProofsRequest {
                l1_batch_number,
                hashed_keys,
            })
            .send()
            .await
            .with_context(|| format!("Failed requesting proofs for L1 batch #{l1_batch_number}"))?;
        let response = response.error_for_status().with_context(|| {
            format!("Requesting proofs for L1 batch #{l1_batch_number} returned non-OK response")
        })?;
        let response: TreeProofsResponse = response.json().await.with_context(|| {
            format!("Failed deserializing proofs for L1 batch #{l1_batch_number}")
        })?;
        Ok(response.entries)
    }
}

impl AsyncTreeReader {
    async fn info_handler(State(this): State<Self>) -> Json<MerkleTreeInfo> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::Info].start();
        let info = this.info().await;
        latency.observe();
        Json(info)
    }

    async fn get_proofs_inner(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> Result<Vec<TreeEntryWithProof>, NoVersionError> {
        let proofs = self
            .clone()
            .entries_with_proofs(l1_batch_number, hashed_keys)
            .await?;
        Ok(proofs.into_iter().map(TreeEntryWithProof::new).collect())
    }

    async fn get_proofs_handler(
        State(this): State<Self>,
        Json(request): Json<TreeProofsRequest>,
    ) -> Result<Json<TreeProofsResponse>, TreeApiError> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetProofs].start();
        let entries = this
            .get_proofs_inner(request.l1_batch_number, request.hashed_keys)
            .await
            .map_err(TreeApiError::NoTreeVersion)?;
        let response = TreeProofsResponse { entries };
        latency.observe();
        Ok(Json(response))
    }

    fn create_api_server(
        self,
        bind_address: &SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<MerkleTreeServer> {
        tracing::debug!("Starting Merkle tree API server on {bind_address}");

        let app = Router::new()
            .route("/", routing::get(Self::info_handler))
            .route("/proofs", routing::post(Self::get_proofs_handler))
            .with_state(self);

        let server = axum::Server::try_bind(bind_address)
            .with_context(|| format!("Failed binding Merkle tree API server to {bind_address}"))?
            .serve(app.into_make_service());
        let local_addr = server.local_addr();
        let server_future = async move {
            server.with_graceful_shutdown(async move {
                if stop_receiver.changed().await.is_err() {
                    tracing::warn!(
                        "Stop signal sender for Merkle tree API server was dropped without sending a signal"
                    );
                }
                tracing::info!("Stop signal received, Merkle tree API server is shutting down");
            })
                .await
                .context("Merkle tree API server failed")?;

            tracing::info!("Merkle tree API server shut down");
            Ok(())
        };

        Ok(MerkleTreeServer {
            local_addr,
            server_future: Box::pin(server_future),
        })
    }

    /// Runs the HTTP API server.
    pub async fn run_api_server(
        self,
        bind_address: SocketAddr,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        self.create_api_server(&bind_address, stop_receiver)?
            .run()
            .await
    }
}

/// `axum`-powered REST server for Merkle tree API.
#[must_use = "Server must be `run()`"]
struct MerkleTreeServer {
    local_addr: SocketAddr,
    server_future: Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>,
}

impl fmt::Debug for MerkleTreeServer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MerkleTreeServer")
            .field("local_addr", &self.local_addr)
            .finish_non_exhaustive()
    }
}

impl MerkleTreeServer {
    #[cfg(test)]
    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    async fn run(self) -> anyhow::Result<()> {
        self.server_future.await
    }
}
