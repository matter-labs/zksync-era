mod proof_request_acknowledged;
mod proof_request_proven;
mod reward_claimed;

use std::sync::Arc;

pub use proof_request_acknowledged::*;
pub use proof_request_proven::*;
pub use reward_claimed::*;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::H256;

#[async_trait]
pub trait EventHandler {
    type Event;

    fn signature() -> H256;
    async fn handle_event(
        event: Self::Event,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()>;
}
