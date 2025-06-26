use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{api::Log, ethabi, h256_to_u256, H256, U256};

use crate::{types::ProvingNetwork, watcher::events::EventHandler};

// event RewardClaimed(ProvingNetwork indexed by, uint256 amount);
#[derive(Debug)]
pub struct RewardClaimed {
    pub by: ProvingNetwork,
    pub amount: U256,
}

pub struct RewardClaimedHandler;

#[async_trait]
impl EventHandler for RewardClaimedHandler {
    fn signature(&self) -> H256 {
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
        log: Log,
        _connection_pool: ConnectionPool<Core>,
        _blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        if log.topics.len() != 2 {
            return Err(anyhow::anyhow!(
                "invalid number of topics: {:?}, expected 2",
                log.topics
            ));
        }

        if log.data.0.len() != 32 {
            return Err(anyhow::anyhow!(
                "invalid data length: {:?}, expected 32",
                log.data.0
            ));
        }

        if *log.topics.get(0).context("missing topic 0")? != self.signature() {
            return Err(anyhow::anyhow!(
                "invalid signature: {:?}, expected {:?}",
                log.topics.get(0),
                self.signature()
            ));
        }

        let chain_id = h256_to_u256(*log.topics.get(1).context("missing topic 1")?);
        let amount = U256::from_big_endian(log.data.0.as_slice());

        let event = RewardClaimed {
            by: ProvingNetwork::from_u256(chain_id),
            amount,
        };

        tracing::info!("Received RewardClaimedEvent: {:?}", event);
        Ok(())
    }
}
