use std::sync::Arc;

use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{ethabi, H256, U256};

use crate::{types::ProvingNetwork, watcher::events::Event};

// event RewardClaimed(ProvingNetwork indexed by, uint256 amount);
#[derive(Debug)]
pub struct RewardClaimed {
    pub by: ProvingNetwork,
    pub amount: U256,
}

impl RewardClaimed {
    pub fn empty() -> Self {
        Self { by: ProvingNetwork::None, amount: U256::zero() }
    }
}

#[async_trait]
impl Event for RewardClaimed {
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

    async fn handle(
        &self,
        _connection_pool: ConnectionPool<Core>,
        _blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        tracing::info!("Received RewardClaimedEvent: {:?}", self);
        Ok(())
    }
}