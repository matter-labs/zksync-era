//! Shared Merkle tree resources.

use std::fmt;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use zksync_config::configs::database::MerkleTreeMode;
use zksync_node_framework::{resource, Resource};
use zksync_types::{L1BatchNumber, H256, U256};

/// General information about the Merkle tree.
#[derive(Debug, Serialize, Deserialize)]
pub struct MerkleTreeInfo {
    pub mode: MerkleTreeMode,
    pub root_hash: H256,
    pub next_l1_batch_number: L1BatchNumber,
    pub min_l1_batch_number: Option<L1BatchNumber>,
    pub leaf_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TreeEntryWithProof {
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

/// Client-side tree API error used by [`TreeApiClient`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TreeApiError {
    #[error(
        "version {missing_version} does not exist in Merkle tree; it has {version_count} versions"
    )]
    NoVersion {
        /// Missing requested version of the tree.
        missing_version: u64,
        /// Current number of versions in the tree.
        version_count: u64,
    },
    #[error("tree API is temporarily unavailable")]
    NotReady(#[source] Option<anyhow::Error>),
    /// Catch-all variant for internal errors.
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

/// Client accessing Merkle tree API.
#[async_trait]
pub trait TreeApiClient: 'static + Send + Sync + fmt::Debug {
    /// Obtains general information about the tree.
    async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError>;

    /// Obtains proofs for the specified `hashed_keys` at the specified tree version (= L1 batch number).
    async fn get_proofs(
        &self,
        l1_batch_number: L1BatchNumber,
        hashed_keys: Vec<U256>,
    ) -> Result<Vec<TreeEntryWithProof>, TreeApiError>;
}

impl Resource<resource::Shared> for dyn TreeApiClient {
    fn name() -> String {
        "api/tree_api_client".into()
    }
}
