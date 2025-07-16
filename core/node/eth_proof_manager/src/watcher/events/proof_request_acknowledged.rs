use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{api::Log, ethabi, h256_to_u256, H256, U256};

use crate::{types::ProvingNetwork, watcher::events::EventHandler};

//event ProofRequestAcknowledged(
//     uint256 indexed chainId,
//     uint256 indexed blockNumber,
//     bool accepted,
//     ProvingNetwork indexed assignedTo
// );
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProofRequestAcknowledged {
    pub chain_id: U256,
    pub block_number: U256,
    pub accepted: bool,
    pub assigned_to: ProvingNetwork,
}

#[derive(Debug)]
pub struct ProofRequestAcknowledgedHandler;

#[async_trait]
impl EventHandler for ProofRequestAcknowledgedHandler {
    fn signature(&self) -> H256 {
        ethabi::long_signature(
            "ProofRequestAcknowledged",
            &[
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Uint(256),
                ethabi::ParamType::Bool,
                // ProvingNetwork is enum, encoded as uint8
                ethabi::ParamType::Uint(8),
            ],
        )
    }

    async fn handle(
        &self,
        log: Log,
        _connection_pool: ConnectionPool<Core>,
        _blob_store: Arc<dyn ObjectStore>,
    ) -> anyhow::Result<()> {
        if log.topics.len() != 4 {
            return Err(anyhow::anyhow!(
                "invalid number of topics: {:?}, expected 4",
                log.topics
            ));
        }

        if *log.topics.first().context("missing topic 0")? != self.signature() {
            return Err(anyhow::anyhow!(
                "invalid signature: {:?}, expected {:?}",
                log.topics.first(),
                self.signature()
            ));
        }

        let chain_id = h256_to_u256(*log.topics.get(1).context("missing topic 1")?);
        let block_number = h256_to_u256(*log.topics.get(2).context("missing topic 2")?);

        let accepted = if let Some(accepted) = log.data.0.get(31) {
            match accepted {
                1 => true,
                0 => false,
                _ => panic!("invalid accepted value: {:?}", accepted),
            }
        } else {
            panic!("invalid accepted value: {:?}", log.data.0);
        };

        let assigned_to =
            ProvingNetwork::from_u256(h256_to_u256(*log.topics.get(3).context("missing topic 3")?));

        let event = ProofRequestAcknowledged {
            chain_id,
            block_number,
            accepted,
            assigned_to,
        };

        tracing::info!("Received ProofRequestAcknowledgedEvent: {:?}", event);
        Ok(())
    }
}
