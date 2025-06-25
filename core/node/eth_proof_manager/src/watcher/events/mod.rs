use std::sync::Arc;

use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::H256;

pub mod proof_request_proven;
pub mod proof_request_acknowledged;
pub mod reward_claimed;

pub use proof_request_acknowledged::ProofRequestAcknowledged;
pub use proof_request_proven::ProofRequestProven;
pub use reward_claimed::RewardClaimed;

#[async_trait]
pub trait Event {
    fn signature() -> H256 where Self: Sized;

    async fn handle(
        &self,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()>;
}