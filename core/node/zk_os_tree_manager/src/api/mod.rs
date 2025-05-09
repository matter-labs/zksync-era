//! Simple Merkle tree API used internally to fetch proofs.

use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    future::Future,
    net::SocketAddr,
    ops,
    pin::Pin,
};

use anyhow::Context;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::watch;
use zk_os_merkle_tree::{unstable, BatchTreeProof, TreeOperation};
use zksync_types::{web3, L1BatchNumber, H256};

pub use self::client::{TreeApiClient, TreeApiError, TreeApiHttpClient};
use self::metrics::{MerkleTreeApiMethod, API_METRICS};
use crate::{health::MerkleTreeInfo, helpers::AsyncTreeReader};

mod client;
mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofRequest {
    l1_batch_number: L1BatchNumber,
    hashed_keys: Vec<H256>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ApiTreeOperation {
    Hit { index: u64 },
    Miss { prev_index: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiBatchTreeProof {
    operations: Vec<ApiTreeOperation>,
    sorted_leaves: BTreeMap<u64, ApiLeaf>,
    hashes: Vec<H256>,
}

impl From<BatchTreeProof> for ApiBatchTreeProof {
    fn from(proof: BatchTreeProof) -> Self {
        assert!(proof.operations.is_empty());
        Self {
            operations: proof
                .read_operations
                .into_iter()
                .map(|op| match op {
                    TreeOperation::Hit { index } => ApiTreeOperation::Hit { index },
                    TreeOperation::Miss { prev_index } => ApiTreeOperation::Miss { prev_index },
                })
                .collect(),
            sorted_leaves: proof
                .sorted_leaves
                .into_iter()
                .map(|(idx, leaf)| (idx, leaf.into()))
                .collect(),
            hashes: proof.hashes.into_iter().map(|hash| hash.value).collect(),
        }
    }
}

impl From<ApiBatchTreeProof> for BatchTreeProof {
    fn from(api: ApiBatchTreeProof) -> Self {
        Self {
            operations: vec![],
            read_operations: api
                .operations
                .into_iter()
                .map(|op| match op {
                    ApiTreeOperation::Hit { index } => TreeOperation::Hit { index },
                    ApiTreeOperation::Miss { prev_index } => TreeOperation::Miss { prev_index },
                })
                .collect(),
            sorted_leaves: api
                .sorted_leaves
                .into_iter()
                .map(|(idx, leaf)| (idx, leaf.into()))
                .collect(),
            hashes: api.hashes.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofResponse {
    proof: ApiBatchTreeProof,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct SerdeNodeKey(unstable::NodeKey);

impl From<unstable::NodeKey> for SerdeNodeKey {
    fn from(key: unstable::NodeKey) -> Self {
        Self(key)
    }
}

impl Serialize for SerdeNodeKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for SerdeNodeKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HexNodeKeyVisitor;

        impl de::Visitor<'_> for HexNodeKeyVisitor {
            type Value = SerdeNodeKey;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("hex-encoded versioned key like `123:c0ffee`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse().map(SerdeNodeKey).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(HexNodeKeyVisitor)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiLeaf {
    key: H256,
    value: H256,
    next_index: u64,
}

impl From<unstable::Leaf> for ApiLeaf {
    fn from(leaf: unstable::Leaf) -> Self {
        Self {
            key: leaf.key,
            value: leaf.value,
            next_index: leaf.next_index,
        }
    }
}

impl From<ApiLeaf> for unstable::Leaf {
    fn from(api: ApiLeaf) -> Self {
        Self {
            key: api.key,
            value: api.value,
            next_index: api.next_index,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ApiChildRef {
    hash: H256,
    version: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(transparent)]
struct ApiInternalNode(Vec<ApiChildRef>);

#[derive(Debug, Serialize, Deserialize)]
struct ApiRawNode {
    raw: web3::Bytes,
    #[serde(skip_serializing_if = "Option::is_none")]
    leaf: Option<ApiLeaf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    internal: Option<ApiInternalNode>,
}

impl From<unstable::RawNode> for ApiRawNode {
    fn from(node: unstable::RawNode) -> Self {
        Self {
            raw: web3::Bytes(node.raw),
            leaf: node.leaf.map(Into::into),
            internal: node.internal.map(|node| {
                let child_refs = node
                    .child_refs()
                    .map(|(hash, version)| ApiChildRef { hash, version });
                ApiInternalNode(child_refs.collect())
            }),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeNodesRequest {
    keys: Vec<SerdeNodeKey>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeNodesResponse {
    nodes: HashMap<SerdeNodeKey, ApiRawNode>,
}

#[derive(Debug, thiserror::Error)]
enum ApiServerError {
    #[error(
        "L1 batch #{requested} is missing from the tree; the available versions are {available:?}"
    )]
    NoTreeVersion {
        requested: u64,
        available: ops::Range<u64>,
    },
    #[error("Internal server error")]
    Internal(#[from] anyhow::Error),
}

// Loosely conforms to HTTP Problem Details RFC: <https://datatracker.ietf.org/doc/html/rfc7807>
#[derive(Debug, Serialize, Deserialize)]
struct Problem {
    r#type: String,
    title: String,
    detail: String,
}

impl Problem {
    const NO_TREE_VERSION_TYPE: &'static str = "/errors#l1-batch-not-found";
    const INTERNAL_TYPE: &'static str = "/errors#internal";
}

const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

impl IntoResponse for ApiServerError {
    fn into_response(self) -> Response {
        let headers = [(header::CONTENT_TYPE, PROBLEM_CONTENT_TYPE)];
        match self {
            Self::NoTreeVersion { .. } => {
                let body = Problem {
                    r#type: Problem::NO_TREE_VERSION_TYPE.to_owned(),
                    title: "L1 batch not found".to_owned(),
                    detail: self.to_string(),
                };
                (StatusCode::NOT_FOUND, headers, Json(body)).into_response()
            }
            Self::Internal(err) => {
                tracing::warn!("Internal server error: {err:#}");
                let body = Problem {
                    r#type: Problem::INTERNAL_TYPE.to_owned(),
                    title: "Internal server error".to_owned(),
                    detail: "Error details are logged".to_owned(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, headers, Json(body)).into_response()
            }
        }
    }
}

impl AsyncTreeReader {
    async fn info_handler(
        State(this): State<Self>,
    ) -> Result<Json<MerkleTreeInfo>, ApiServerError> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::Info].start();
        let info = this.info().await?;
        latency.observe();
        Ok(Json(info))
    }

    async fn get_proofs_inner(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<H256>,
    ) -> Result<BatchTreeProof, ApiServerError> {
        let info = self.clone().info().await?;
        let available_versions = info.min_version.unwrap_or(0)..info.next_version;
        let requested_version = l1_batch_number.0.into();
        if !available_versions.contains(&requested_version) {
            return Err(ApiServerError::NoTreeVersion {
                requested: requested_version,
                available: available_versions,
            });
        }

        // There's a potential for TOCTOU here once tree pruning is implemented, but its possibility is reasonably low.
        self.clone()
            .prove(l1_batch_number.0.into(), hashed_keys)
            .await
            .map_err(ApiServerError::Internal)
    }

    async fn get_proofs_handler(
        State(this): State<Self>,
        Json(request): Json<TreeProofRequest>,
    ) -> Result<Json<TreeProofResponse>, ApiServerError> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetProof].start();
        let proof = this
            .get_proofs_inner(request.l1_batch_number, request.hashed_keys)
            .await?;
        let response = TreeProofResponse {
            proof: proof.into(),
        };
        latency.observe();
        Ok(Json(response))
    }

    async fn get_nodes_handler(
        State(this): State<Self>,
        Json(request): Json<TreeNodesRequest>,
    ) -> Result<Json<TreeNodesResponse>, ApiServerError> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetNodes].start();
        let keys: Vec<_> = request.keys.iter().map(|key| key.0).collect();
        let nodes = this.clone().raw_nodes(keys).await?;
        let nodes = request
            .keys
            .into_iter()
            .zip(nodes)
            .filter_map(|(key, node)| Some((key, node?.into())))
            .collect();
        let response = TreeNodesResponse { nodes };
        latency.observe();
        Ok(Json(response))
    }

    async fn create_api_server(
        self,
        bind_address: &SocketAddr,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<MerkleTreeServer> {
        tracing::debug!("Starting Merkle tree API server on {bind_address}");

        let app = Router::new()
            .route("/", routing::get(Self::info_handler))
            .route("/proofs", routing::post(Self::get_proofs_handler))
            .route("/debug/nodes", routing::post(Self::get_nodes_handler))
            .with_state(self);

        let listener = tokio::net::TcpListener::bind(bind_address)
            .await
            .with_context(|| format!("Failed binding Merkle tree API server to {bind_address}"))?;
        let local_addr = listener.local_addr()?;
        let server = axum::serve(listener, app);
        let server_future = async move {
            server.with_graceful_shutdown(async move {
                if stop_receiver.changed().await.is_err() {
                    tracing::warn!(
                        "Stop request sender for Merkle tree API server was dropped without sending a signal"
                    );
                }
                tracing::info!("Stop request received, Merkle tree API server is shutting down");
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
        self.create_api_server(&bind_address, stop_receiver)
            .await?
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
