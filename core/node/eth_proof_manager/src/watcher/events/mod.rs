use async_trait::async_trait;
use zksync_types::{api::Log, H256};

pub mod proof_request_acknowledged;
pub mod proof_request_proven;

pub use proof_request_acknowledged::ProofRequestAcknowledgedHandler;
pub use proof_request_proven::ProofRequestProvenHandler;

#[async_trait]
pub trait EventHandler: Send + Sync {
    // using reference to self here, otherwise we cannot call signature() on dyn traits
    fn signature(&self) -> H256;

    async fn handle(&self, log: Log) -> anyhow::Result<()>;
}
