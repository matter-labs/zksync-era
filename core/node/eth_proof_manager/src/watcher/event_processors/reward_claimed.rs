use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::event_processors::EventHandler};

// event RewardClaimed(ProvingNetwork indexed by, uint256 amount);
struct RewardClaimedEvent {
    pub by: ProvingNetwork,
    pub amount: U256,
}

pub struct RewardClaimedEventHandler;

#[async_trait]
impl EventHandler for RewardClaimedEvent {
    type Event = RewardClaimedEvent;

    fn signature() -> H256 {
        ethabi::long_signature(
            "RewardClaimed",
            &[
                // ProvingNetwork is enum, encoded as uint8
                ethabi::ParamType::Uint(8),
                ethabi::ParamType::Uint(256),
            ],
        )
    }

    async fn handle_event(
        event: RewardClaimedEvent,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
}
