use anyhow::Context;
use async_trait::async_trait;
use zksync_dal::{eth_watcher_dal::EventType, ConnectionPool, Core, CoreDal};
use zksync_types::{api::Log, ethabi, h256_to_u256, L1BatchNumber, L2ChainId, H256, U256};

use crate::{
    metrics::METRICS,
    types::ProvingNetwork,
    watcher::events::{EventError, EventHandler},
};

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
pub struct ProofRequestAcknowledgedHandler {
    connection_pool: ConnectionPool<Core>,
    chain_id: L2ChainId,
}

impl ProofRequestAcknowledgedHandler {
    pub fn new(connection_pool: ConnectionPool<Core>, chain_id: L2ChainId) -> Self {
        Self {
            connection_pool,
            chain_id,
        }
    }
}

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

    fn event_type(&self) -> EventType {
        EventType::ProofRequestAcknowledged
    }

    async fn handle(&self, log: &Log) -> Result<(), EventError> {
        let event = self.parse(log).map_err(EventError::fatal)?;

        tracing::info!("Received ProofRequestAcknowledgedEvent: {:?}", event);

        METRICS.acknowledged_batches[&event.assigned_to].set(event.block_number.as_u64());

        let batch_number = L1BatchNumber(event.block_number.as_u32());
        let mut connection = self
            .connection_pool
            .connection()
            .await
            .map_err(EventError::transient)?;

        if event.accepted {
            connection
                .eth_proof_manager_dal()
                .acknowledge_batch(batch_number, event.assigned_to.into())
                .await
                .map_err(EventError::transient)?;
        } else {
            tracing::info!(
                "Proof request for batch {} not accepted, moving to prover cluster",
                event.block_number
            );
            METRICS.fallbacked_batches.inc();
            connection
                .eth_proof_manager_dal()
                .fallback_batch(batch_number)
                .await
                .map_err(EventError::transient)?;
        }

        Ok(())
    }
}

impl ProofRequestAcknowledgedHandler {
    fn parse(&self, log: &Log) -> anyhow::Result<ProofRequestAcknowledged> {
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

        if chain_id != U256::from(self.chain_id.as_u64()) {
            return Err(anyhow::anyhow!(
                "Chain ID from event didn't match, expected {}, received {}",
                self.chain_id,
                chain_id
            ));
        }

        let block_number = h256_to_u256(*log.topics.get(2).context("missing topic 2")?);

        let accepted = match log.data.0.get(31) {
            Some(1) => true,
            Some(0) => false,
            other => anyhow::bail!(
                "invalid accepted value {:?} in ProofRequestAcknowledged event data {:?}",
                other,
                log.data.0
            ),
        };

        let assigned_to =
            ProvingNetwork::from_u256(h256_to_u256(*log.topics.get(3).context("missing topic 3")?))
                .context("invalid assigned_to in ProofRequestAcknowledged event")?;

        Ok(ProofRequestAcknowledged {
            chain_id,
            block_number,
            accepted,
            assigned_to,
        })
    }
}
