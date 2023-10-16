use std::convert::TryFrom;

use zksync_contracts::zksync_contract;
use zksync_dal::StorageProcessor;
use zksync_types::{l1::L1Tx, web3::types::Log, PriorityOpId, H256};

use crate::{
    eth_watch::{
        client::{Error, EthClient},
        event_processors::EventProcessor,
        metrics::{PollStage, METRICS},
    },
    metrics::{TxStage, APP_METRICS},
};

/// Responsible for saving new priority L1 transactions to the database.
#[derive(Debug)]
pub struct PriorityOpsEventProcessor {
    next_expected_priority_id: PriorityOpId,
    new_priority_request_signature: H256,
}

impl PriorityOpsEventProcessor {
    pub fn new(next_expected_priority_id: PriorityOpId) -> Self {
        Self {
            next_expected_priority_id,
            new_priority_request_signature: zksync_contract()
                .event("NewPriorityRequest")
                .expect("NewPriorityRequest event is missing in abi")
                .signature(),
        }
    }
}

#[async_trait::async_trait]
impl<W: EthClient + Sync> EventProcessor<W> for PriorityOpsEventProcessor {
    async fn process_events(
        &mut self,
        storage: &mut StorageProcessor<'_>,
        _client: &W,
        events: Vec<Log>,
    ) -> Result<(), Error> {
        let mut priority_ops = Vec::new();
        for event in events
            .into_iter()
            .filter(|event| event.topics[0] == self.new_priority_request_signature)
        {
            let tx = L1Tx::try_from(event).map_err(|err| Error::LogParse(format!("{}", err)))?;
            priority_ops.push(tx);
        }

        if priority_ops.is_empty() {
            return Ok(());
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
        if new_ops.is_empty() {
            return Ok(());
        }

        let first_new = &new_ops[0];
        let last_new = new_ops[new_ops.len() - 1].clone();
        assert_eq!(
            first_new.serial_id(),
            self.next_expected_priority_id,
            "priority transaction serial id mismatch"
        );

        let stage_latency = METRICS.poll_eth_node[&PollStage::PersistL1Txs].start();
        APP_METRICS.processed_txs[&TxStage::added_to_mempool()].inc();
        APP_METRICS.processed_l1_txs[&TxStage::added_to_mempool()].inc();
        for new_op in new_ops {
            let eth_block = new_op.eth_block();
            storage
                .transactions_dal()
                .insert_transaction_l1(new_op, eth_block)
                .await;
        }
        stage_latency.observe();
        self.next_expected_priority_id = last_new.serial_id().next();
        Ok(())
    }

    fn relevant_topic(&self) -> H256 {
        self.new_priority_request_signature
    }
}
