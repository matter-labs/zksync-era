//! Primitive Merkle tree API used internally to fetch proofs.

use anyhow::Context as _;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use std::{fmt, future::Future, net::SocketAddr, pin::Pin};

use super::helpers::AsyncTreeReader;
use zksync_merkle_tree::NoVersionError;
use zksync_types::{L1BatchNumber, H256};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TreeProofsRequest {
    pub l1_batch_number: L1BatchNumber,
    pub hashed_keys: Vec<H256>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct TreeProofsResponse {
    pub entries: Vec<TreeEntryWithProof>,
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
pub(crate) enum TreeApiError {
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

impl AsyncTreeReader {
    async fn get_proofs(
        State(mut this): State<Self>,
        Json(request): Json<TreeProofsRequest>,
    ) -> Result<Json<TreeProofsResponse>, TreeApiError> {
        // FIXME: metrics
        let entries = this
            .entries_with_proofs(request.l1_batch_number, request.hashed_keys)
            .await
            .map_err(TreeApiError::NoTreeVersion)?;
        let response = TreeProofsResponse {
            entries: entries.into_iter().map(TreeEntryWithProof::new).collect(),
        };
        Ok(Json(response))
    }

    pub(super) fn create_server(
        self,
        bind_address: &SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<MerkleTreeServer> {
        tracing::debug!("Starting Merkle tree API server on {bind_address}");

        let app = Router::new()
            .route("/proofs", routing::post(Self::get_proofs))
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
}

#[must_use = "Server must be `run()`"]
pub(super) struct MerkleTreeServer {
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
    #[allow(dead_code)] // FIXME
    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    pub async fn run(self) -> anyhow::Result<()> {
        self.server_future.await
    }
}
