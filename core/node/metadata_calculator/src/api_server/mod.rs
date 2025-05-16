//! Primitive Merkle tree API used internally to fetch proofs.

use std::{collections::HashMap, fmt, future::Future, net::SocketAddr, pin::Pin};

use anyhow::Context as _;
use async_trait::async_trait;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing, Json, Router,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_merkle_tree::{
    unstable::{NodeKey, RawNode},
    NoVersionError, ValueHash,
};
use zksync_shared_resources::tree::{
    MerkleTreeInfo, TreeApiClient, TreeApiError, TreeEntryWithProof,
};
use zksync_types::{u256_to_h256, web3, L1BatchNumber, H256, U256};

use self::metrics::{MerkleTreeApiMethod, API_METRICS};
use crate::{AsyncTreeReader, LazyAsyncTreeReader};

mod metrics;
#[cfg(test)]
mod tests;

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofsRequest {
    l1_batch_number: L1BatchNumber,
    hashed_keys: Vec<U256>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TreeProofsResponse {
    entries: Vec<TreeEntryWithProof>,
}

fn map_entry_with_proof(src: zksync_merkle_tree::TreeEntryWithProof) -> TreeEntryWithProof {
    let mut merkle_path = src.merkle_path;
    merkle_path.reverse(); // Use root-to-leaf enumeration direction as in Ethereum
    TreeEntryWithProof {
        value: src.base.value,
        index: src.base.leaf_index,
        merkle_path,
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct HexNodeKey(NodeKey);

impl Serialize for HexNodeKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for HexNodeKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HexNodeKeyVisitor;

        impl de::Visitor<'_> for HexNodeKeyVisitor {
            type Value = HexNodeKey;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("hex-encoded versioned key like `123:c0ffee`")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                v.parse().map(HexNodeKey).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_str(HexNodeKeyVisitor)
    }
}

#[derive(Debug, Serialize)]
struct ApiLeafNode {
    full_key: H256,
    value_hash: H256,
    leaf_index: u64,
}

#[derive(Debug, Serialize)]
struct ApiChildRef {
    hash: ValueHash,
    version: u64,
    is_leaf: bool,
}

#[derive(Debug, Serialize)]
#[serde(transparent)]
struct ApiInternalNode(HashMap<char, ApiChildRef>);

#[derive(Debug, Serialize)]
struct ApiRawNode {
    raw: web3::Bytes,
    #[serde(skip_serializing_if = "Option::is_none")]
    leaf: Option<ApiLeafNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    internal: Option<ApiInternalNode>,
}

impl From<RawNode> for ApiRawNode {
    fn from(node: RawNode) -> Self {
        Self {
            raw: web3::Bytes(node.raw),
            leaf: node.leaf.map(|leaf| ApiLeafNode {
                full_key: u256_to_h256(leaf.full_key),
                value_hash: leaf.value_hash,
                leaf_index: leaf.leaf_index,
            }),
            internal: node.internal.map(|internal| {
                ApiInternalNode(
                    internal
                        .children()
                        .map(|(nibble, child_ref)| {
                            let nibble = if nibble < 10 {
                                b'0' + nibble
                            } else {
                                b'a' + nibble - 10
                            };
                            (
                                char::from(nibble),
                                ApiChildRef {
                                    hash: child_ref.hash,
                                    version: child_ref.version,
                                    is_leaf: child_ref.is_leaf,
                                },
                            )
                        })
                        .collect(),
                )
            }),
        }
    }
}

#[derive(Debug, Deserialize)]
struct TreeNodesRequest {
    keys: Vec<HexNodeKey>,
}

#[derive(Debug, Serialize)]
struct TreeNodesResponse {
    nodes: HashMap<HexNodeKey, ApiRawNode>,
}

#[derive(Debug, Deserialize)]
struct StaleKeysRequest {
    l1_batch_number: L1BatchNumber,
}

#[derive(Debug, Serialize)]
struct StaleKeysResponse {
    stale_keys: Vec<HexNodeKey>,
}

/// Server-side tree API error.
#[derive(Debug)]
enum TreeApiServerError {
    NoTreeVersion(NoVersionError),
}

// Contains the same fields as `NoVersionError` and is serializable.
#[derive(Debug, Serialize, Deserialize)]
struct NoVersionErrorData {
    missing_version: u64,
    version_count: u64,
}

impl From<NoVersionError> for NoVersionErrorData {
    fn from(err: NoVersionError) -> Self {
        Self {
            missing_version: err.missing_version,
            version_count: err.version_count,
        }
    }
}

impl From<NoVersionErrorData> for NoVersionError {
    fn from(data: NoVersionErrorData) -> Self {
        Self {
            missing_version: data.missing_version,
            version_count: data.version_count,
        }
    }
}

// Loosely conforms to HTTP Problem Details RFC: <https://datatracker.ietf.org/doc/html/rfc7807>
#[derive(Debug, Serialize)]
struct Problem<T> {
    r#type: &'static str,
    title: &'static str,
    detail: String,
    #[serde(flatten)]
    data: T,
}

const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";

impl IntoResponse for TreeApiServerError {
    fn into_response(self) -> Response {
        let headers = [(header::CONTENT_TYPE, PROBLEM_CONTENT_TYPE)];
        match self {
            Self::NoTreeVersion(err) => {
                let body = Problem {
                    r#type: "/errors#l1-batch-not-found",
                    title: "L1 batch not found",
                    detail: err.to_string(),
                    data: NoVersionErrorData::from(err),
                };
                (StatusCode::NOT_FOUND, headers, Json(body)).into_response()
            }
        }
    }
}

fn client_error(err: reqwest::Error, request_description: impl fmt::Display) -> TreeApiError {
    let is_not_ready = err.is_timeout() || err.is_connect();
    let err = anyhow::Error::new(err).context(format!("failed requesting {request_description}"));
    if is_not_ready {
        TreeApiError::NotReady(Some(err))
    } else {
        TreeApiError::Internal(err)
    }
}

/// In-memory client implementation.
#[async_trait]
impl TreeApiClient for LazyAsyncTreeReader {
    async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError> {
        if let Some(reader) = self.read() {
            Ok(reader.info().await)
        } else {
            Err(TreeApiError::NotReady(None))
        }
    }

    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> Result<Vec<TreeEntryWithProof>, TreeApiError> {
        if let Some(reader) = self.read() {
            reader
                .get_proofs_inner(l1_batch_number, hashed_keys)
                .await
                .map_err(|err| TreeApiError::NoVersion {
                    missing_version: err.missing_version,
                    version_count: err.version_count,
                })
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
            .map_err(|err| client_error(err, "tree info"))?;
        let response = response
            .error_for_status()
            .context("Requesting tree info returned non-OK response")?;
        Ok(response
            .json()
            .await
            .context("Failed deserializing tree info")?)
    }

    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> Result<Vec<TreeEntryWithProof>, TreeApiError> {
        let response = self
            .inner
            .post(&self.proofs_url)
            .json(&TreeProofsRequest {
                l1_batch_number,
                hashed_keys,
            })
            .send()
            .await
            .map_err(|err| {
                client_error(err, format_args!("proofs for L1 batch #{l1_batch_number}"))
            })?;

        let is_problem = response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|header| *header == PROBLEM_CONTENT_TYPE);
        if response.status() == StatusCode::NOT_FOUND && is_problem {
            // Try to parse `NoVersionError` from the response body.
            let problem_data: NoVersionErrorData = response
                .json()
                .await
                .context("failed parsing error response")?;
            return Err(TreeApiError::NoVersion {
                missing_version: problem_data.missing_version,
                version_count: problem_data.version_count,
            });
        }

        let response = response.error_for_status().with_context(|| {
            format!("requesting proofs for L1 batch #{l1_batch_number} returned non-OK response")
        })?;
        let response: TreeProofsResponse = response.json().await.with_context(|| {
            format!("failed deserializing proofs for L1 batch #{l1_batch_number}")
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
        Ok(proofs.into_iter().map(map_entry_with_proof).collect())
    }

    async fn get_proofs_handler(
        State(this): State<Self>,
        Json(request): Json<TreeProofsRequest>,
    ) -> Result<Json<TreeProofsResponse>, TreeApiServerError> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetProofs].start();
        let entries = this
            .get_proofs_inner(request.l1_batch_number, request.hashed_keys)
            .await
            .map_err(TreeApiServerError::NoTreeVersion)?;
        let response = TreeProofsResponse { entries };
        latency.observe();
        Ok(Json(response))
    }

    async fn get_nodes_handler(
        State(this): State<Self>,
        Json(request): Json<TreeNodesRequest>,
    ) -> Json<TreeNodesResponse> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetNodes].start();
        let keys: Vec<_> = request.keys.iter().map(|key| key.0).collect();
        let nodes = this.clone().raw_nodes(keys).await;
        let nodes = request
            .keys
            .into_iter()
            .zip(nodes)
            .filter_map(|(key, node)| Some((key, node?.into())))
            .collect();
        let response = TreeNodesResponse { nodes };
        latency.observe();
        Json(response)
    }

    async fn get_stale_keys_handler(
        State(this): State<Self>,
        Json(request): Json<StaleKeysRequest>,
    ) -> Json<StaleKeysResponse> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetStaleKeys].start();
        let stale_keys = this.clone().raw_stale_keys(request.l1_batch_number).await;
        let stale_keys = stale_keys.into_iter().map(HexNodeKey).collect();
        latency.observe();
        Json(StaleKeysResponse { stale_keys })
    }

    async fn bogus_stale_keys_handler(
        State(this): State<Self>,
        Json(request): Json<StaleKeysRequest>,
    ) -> Json<StaleKeysResponse> {
        let latency = API_METRICS.latency[&MerkleTreeApiMethod::GetBogusStaleKeys].start();
        let stale_keys = this.clone().bogus_stale_keys(request.l1_batch_number).await;
        let stale_keys = stale_keys.into_iter().map(HexNodeKey).collect();
        latency.observe();
        Json(StaleKeysResponse { stale_keys })
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
            .route(
                "/debug/stale-keys",
                routing::post(Self::get_stale_keys_handler),
            )
            .route(
                "/debug/stale-keys/bogus",
                routing::post(Self::bogus_stale_keys_handler),
            )
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
