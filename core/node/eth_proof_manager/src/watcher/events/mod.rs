use async_trait::async_trait;
use zksync_dal::eth_watcher_dal::EventType;
use zksync_types::{api::Log, H256};

pub mod proof_request_acknowledged;
pub mod proof_request_proven;

pub use proof_request_acknowledged::ProofRequestAcknowledgedHandler;
pub use proof_request_proven::ProofRequestProvenHandler;

/// Fatal errors skip the log; transient errors retry from its block.
#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("fatal: {0:#}")]
    Fatal(anyhow::Error),
    #[error("transient: {0:#}")]
    Transient(anyhow::Error),
}

impl EventError {
    pub fn fatal(err: impl Into<anyhow::Error>) -> Self {
        Self::Fatal(err.into())
    }

    pub fn transient(err: impl Into<anyhow::Error>) -> Self {
        Self::Transient(err.into())
    }
}

#[async_trait]
pub trait EventHandler: Send + Sync {
    // using reference to self here, otherwise we cannot call signature() on dyn traits
    fn signature(&self) -> H256;

    fn event_type(&self) -> EventType;

    async fn handle(&self, log: &Log) -> Result<(), EventError>;
}
