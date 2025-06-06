use std::{convert::TryFrom, sync::Arc};

use anyhow::Context;
use zksync_contracts::hyperchain_contract;
use zksync_dal::{eth_watcher_dal::EventType, Connection, Core, CoreDal, DalError};
use zksync_shared_metrics::{TxStage, APP_METRICS};
use zksync_types::{api::Log, l1::L1Tx, PriorityOpId, H256};

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
    sl_client: Arc<dyn EthClient>,
}

impl PriorityOpsEventProcessor {
    pub fn new(
        next_expected_priority_id: PriorityOpId,
        sl_client: Arc<dyn EthClient>,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            next_expected_priority_id,
            new_priority_request_signature: hyperchain_contract()
                .event("NewPriorityRequest")
                .context("NewPriorityRequest event is missing in ABI")?
                .signature(),
            sl_client,
        })
    }
}

#[async_trait::async_trait]
impl EventProcessor for PriorityOpsEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut Connection<'_, Core>,
        events: Vec<Log>,
    ) -> Result<usize, EventProcessorError> {
        let mut priority_ops = Vec::new();
        let events_count = events.len();
        for event in events {
            assert_eq!(event.topics[0], self.new_priority_request_signature); // guaranteed by the watcher
            let tx = L1Tx::try_from(Into::<zksync_types::web3::Log>::into(event))
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
        let skipped_ops = events_count - new_ops.len();
        let Some(first_new) = new_ops.first() else {
            return Ok(events_count);
        };
        assert_eq!(
            first_new.serial_id(),
            self.next_expected_priority_id,
            "priority transaction serial id mismatch"
        );

        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistL1Txs].start();
        APP_METRICS.processed_txs[&TxStage::added_to_mempool()].inc();
        APP_METRICS.processed_l1_txs[&TxStage::added_to_mempool()].inc();
        let processed_priority_transactions = self.sl_client.get_total_priority_txs().await?;
        let ops_to_insert: Vec<&L1Tx> = new_ops
            .iter()
            .take_while(|op| processed_priority_transactions > op.serial_id().0)
            .collect();

        for new_op in &ops_to_insert {
            storage
                .transactions_dal()
                .insert_transaction_l1(new_op, new_op.eth_block())
                .await
                .map_err(DalError::generalize)?;
        }
        stage_latency.observe();
        if let Some(last_op) = ops_to_insert.last() {
            self.next_expected_priority_id = last_op.serial_id().next();
        }

        Ok(skipped_ops + ops_to_insert.len())
    }

    fn topic1(&self) -> Option<H256> {
        Some(self.new_priority_request_signature)
    }

    fn event_source(&self) -> EventsSource {
        EventsSource::L1
    }

    fn event_type(&self) -> EventType {
        EventType::PriorityTransactions
    }
}
