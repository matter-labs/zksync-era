use std::sync::Arc;

use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use zksync_health_check::{async_trait, CheckHealth, Health, HealthStatus, ReactiveHealthCheck};
use zksync_types::H256;

use crate::helpers::{LazyAsyncTreeReader, WeakAsyncTreeReader};

/// General information about the Merkle tree.
#[derive(Debug, Serialize, Deserialize)]
pub struct MerkleTreeInfo {
    pub root_hash: H256,
    pub next_version: u64,
    pub min_version: Option<u64>,
    pub leaf_count: u64,
}

/// Health details for a Merkle tree.
#[derive(Debug, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub(super) enum MerkleTreeHealth {
    Initialization,
    MainLoop(MerkleTreeInfo),
}

impl From<MerkleTreeHealth> for Health {
    fn from(details: MerkleTreeHealth) -> Self {
        let status = match &details {
            MerkleTreeHealth::Initialization => HealthStatus::Affected,
            MerkleTreeHealth::MainLoop(_) => HealthStatus::Ready,
        };
        Self::from(status).with_details(details)
    }
}

/// Health check for the Merkle tree.
///
/// [`ReactiveHealthCheck`] is not sufficient for the tree because in the main loop, tree info
/// can be updated by multiple tasks (the metadata calculator and the pruning task). Additionally,
/// keeping track of all places where the info is updated is error-prone.
#[derive(Debug)]
pub(super) struct MerkleTreeHealthCheck {
    reactive_check: ReactiveHealthCheck,
    weak_reader: Arc<OnceCell<WeakAsyncTreeReader>>,
}

impl MerkleTreeHealthCheck {
    pub fn new(reactive_check: ReactiveHealthCheck, reader: LazyAsyncTreeReader) -> Self {
        // We must not retain a strong RocksDB ref in the health check because it will prevent
        // proper node shutdown (which waits until all RocksDB instances are dropped); health checks
        // are dropped after all components are terminated.
        let weak_reader = Arc::<OnceCell<WeakAsyncTreeReader>>::default();
        let weak_reader_for_task = weak_reader.clone();
        tokio::spawn(async move {
            if let Some(reader) = reader.wait().await {
                weak_reader_for_task.set(reader.downgrade()).ok();
            }
            // Otherwise, the tree is dropped before getting initialized; this is not an error in this context.
        });

        Self {
            reactive_check,
            weak_reader,
        }
    }
}

#[async_trait]
impl CheckHealth for MerkleTreeHealthCheck {
    fn name(&self) -> &'static str {
        "tree"
    }

    async fn check_health(&self) -> Health {
        let health = self.reactive_check.check_health().await;
        if !matches!(health.status(), HealthStatus::Ready) {
            return health;
        }

        if let Some(reader) = self
            .weak_reader
            .get()
            .and_then(WeakAsyncTreeReader::upgrade)
        {
            let info = match reader.info().await {
                Ok(info) => info,
                Err(err) => {
                    tracing::warn!("Getting Merkle tree health info failed: {err:#}");
                    return HealthStatus::Affected.into();
                }
            };
            health.with_details(MerkleTreeHealth::MainLoop(info))
        } else {
            health
        }
    }
}
