use std::convert::TryFrom;

use anyhow::Context;
use zksync_contracts::hyperchain_contract;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{l1::L1Tx, web3::Log, PriorityOpId, H256};

use crate::{
    client::EthClient,
    event_processors::{EventProcessor, EventProcessorError, EventsSource},
    metrics::{PollStage, METRICS},
};

/// Responsible for saving new priority L1 transactions to the database.
#[derive(Debug)]
pub struct PriorityOpsEventProcessor {
    next_expected_priority_id: PriorityOpId,
    new_priority_request_signature: H256,
    priority_merkle_tree: SyncMerkleTree<L1Tx>,
}

impl PriorityOpsEventProcessor {
    pub fn new(
        next_expected_priority_id: PriorityOpId,
        priority_merkle_tree: SyncMerkleTree<L1Tx>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            next_expected_priority_id,
            new_priority_request_signature: hyperchain_contract()
                .event("NewPriorityRequest")
                .context("NewPriorityRequest event is missing in ABI")?
                .signature(),
            priority_merkle_tree,
        })
    }
}

#[async_trait::async_trait]
impl EventProcessor for PriorityOpsEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        client: &dyn EthClient,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        let mut priority_ops = Vec::new();
        let events_count = events.len();
        for event in events {
            assert_eq!(event.topics[0], self.new_priority_request_signature); // guaranteed by the watcher
            let tx = L1Tx::try_from(event)
                .map_err(|err| EventProcessorError::log_parse(err, "priority op"))?;
            priority_ops.push(tx);
        }

        if priority_ops.is_empty() {
            return Ok(events_count);
        }

        let first = &priority_ops[0];
        let last = &priority_ops[priority_ops.len() - 1];
        tracing::debug!(
            "Received priority requests with serial ids: {} (block {}) - {} (block {})",
            first.serial_id(),
            first.eth_block(),
            last.serial_id(),
            last.eth_block(),
        );
        assert_eq!(
            last.serial_id().0 - first.serial_id().0 + 1,
            priority_ops.len() as u64,
            "There is a gap in priority ops received"
        );

        let new_ops: Vec<_> = priority_ops
            .into_iter()
            .skip_while(|tx| tx.serial_id() < self.next_expected_priority_id)
            .collect();
        let Some(first_new) = new_ops.first() else {
            return Ok(events_count);
        };
        assert_eq!(
            first_new.serial_id(),
            self.next_expected_priority_id,
            "priority transaction serial id mismatch"
        );
        let mut next_expected_priority_id = self.next_expected_priority_id;

        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistL1Txs].start();
        APP_METRICS.processed_txs[&TxStage::added_to_mempool()].inc();
        APP_METRICS.processed_l1_txs[&TxStage::added_to_mempool()].inc();
        let processed_priority_transactions = client.get_total_priority_txs().await?;
        let mut processed_events_count = 0;
        for new_op in new_ops {
            if processed_priority_transactions <= new_op.serial_id().0 {
                break;
            }
            let eth_block = new_op.eth_block();
            let inserted = storage
                .transactions_dal()
                .insert_transaction_l1(&new_op, eth_block)
                .await
                .map_err(DalError::generalize)?;
            // Transaction could have been a duplicate.
            if inserted {
                self.priority_merkle_tree.push_hash(new_op.hash());
            }
            processed_events_count += 1;
            next_expected_priority_id = new_op.serial_id().next();
        }
        stage_latency.observe();
        self.next_expected_priority_id = next_expected_priority_id;
        Ok(processed_events_count)
    }

    fn relevant_topic(&self) -> H256 {
        self.new_priority_request_signature
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::PriorityTransactions
    }
}
