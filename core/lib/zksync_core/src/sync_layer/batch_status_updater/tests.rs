//! Tests for batch status updater.

use std::collections::HashMap;

use chrono::TimeZone;
use tokio::sync::watch;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_types::{block::BlockGasCount, Address, L2ChainId, ProtocolVersionId};

use super::*;
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    utils::testonly::create_l1_batch,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum L1Stage {
    None,
    Committed,
    Proven,
    Executed,
}

#[derive(Debug, PartialEq)]
struct L1StagesMap {
    first_batch_number: L1BatchNumber,
    stages: Vec<L1Stage>,
}

impl L1StagesMap {
    fn empty(first_batch_number: L1BatchNumber, len: usize) -> Self {
        Self {
            first_batch_number,
            stages: vec![L1Stage::None; len],
        }
    }

    fn new(first_batch_number: L1BatchNumber, stages: Vec<L1Stage>) -> Self {
        assert!(stages.windows(2).all(|window| {
            let [prev, next] = window else { unreachable!() };
            prev >= next
        }));
        Self {
            first_batch_number,
            stages,
        }
    }

    fn iter(&self) -> impl Iterator<Item = (L1BatchNumber, L1Stage)> + '_ {
        self.stages
            .iter()
            .enumerate()
            .map(|(i, &stage)| (self.first_batch_number + i as u32, stage))
    }

    fn update(&mut self, changes: &StatusChanges) {
        self.update_to_stage(&changes.commit, L1Stage::Committed);
        self.update_to_stage(&changes.prove, L1Stage::Proven);
        self.update_to_stage(&changes.execute, L1Stage::Executed);
    }

    fn update_to_stage(&mut self, batch_changes: &[BatchStatusChange], target: L1Stage) {
        for change in batch_changes {
            let number = change.number;
            let index = number
                .0
                .checked_sub(self.first_batch_number.0)
                .unwrap_or_else(|| panic!("stage is missing for L1 batch #{number}"));
            let stage = self
                .stages
                .get_mut(index as usize)
                .unwrap_or_else(|| panic!("stage is missing for L1 batch #{number}"));
            assert!(
                *stage < target,
                "Invalid update for L1 batch #{number}: {stage:?} -> {target:?}"
            );
            *stage = target;
        }
    }
}

fn mock_block_details(number: u32, stage: L1Stage) -> api::BlockDetails {
    api::BlockDetails {
        number: MiniblockNumber(number),
        l1_batch_number: L1BatchNumber(number),
        base: api::BlockDetailsBase {
            timestamp: number.into(),
            l1_tx_count: 0,
            l2_tx_count: 0,
            root_hash: Some(H256::zero()),
            status: api::BlockStatus::Sealed,
            commit_tx_hash: (stage >= L1Stage::Committed).then(|| H256::repeat_byte(1)),
            committed_at: (stage >= L1Stage::Committed).then(|| Utc.timestamp_opt(100, 0).unwrap()),
            prove_tx_hash: (stage >= L1Stage::Proven).then(|| H256::repeat_byte(2)),
            proven_at: (stage >= L1Stage::Proven).then(|| Utc.timestamp_opt(200, 0).unwrap()),
            execute_tx_hash: (stage >= L1Stage::Executed).then(|| H256::repeat_byte(3)),
            executed_at: (stage >= L1Stage::Executed).then(|| Utc.timestamp_opt(300, 0).unwrap()),
            l1_gas_price: 1,
            l2_fair_gas_price: 2,
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        },
        operator_address: Address::zero(),
        protocol_version: Some(ProtocolVersionId::default()),
    }
}

#[derive(Debug, Default)]
struct MockMainNodeClient {
    resolve_l1_batch_to_miniblock_responses: HashMap<L1BatchNumber, MiniblockNumber>,
    block_details_responses: HashMap<MiniblockNumber, api::BlockDetails>,
}

impl MockMainNodeClient {
    fn insert_responses(&mut self, batch_stages: &L1StagesMap) {
        for (number, batch_stage) in batch_stages.iter() {
            let details = mock_block_details(number.0, batch_stage);
            self.resolve_l1_batch_to_miniblock_responses
                .insert(details.l1_batch_number, details.number);
            self.block_details_responses.insert(details.number, details);
        }
    }
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn resolve_l1_batch_to_miniblock(
        &self,
        number: L1BatchNumber,
    ) -> Result<Option<MiniblockNumber>, ClientError> {
        Ok(self
            .resolve_l1_batch_to_miniblock_responses
            .get(&number)
            .copied())
    }

    async fn block_details(
        &self,
        number: MiniblockNumber,
    ) -> Result<Option<api::BlockDetails>, ClientError> {
        Ok(self.block_details_responses.get(&number).cloned())
    }
}

#[tokio::test]
async fn normal_updater_operation() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();
    let target_batch_stages = L1StagesMap::new(
        L1BatchNumber(1),
        vec![
            L1Stage::Executed,
            L1Stage::Proven,
            L1Stage::Proven,
            L1Stage::Committed,
            L1Stage::Committed,
            L1Stage::None,
        ],
    );
    let mut observed_batch_stages =
        L1StagesMap::empty(L1BatchNumber(1), target_batch_stages.stages.len());

    // Make all L1 batches present in the storage from the start.
    for (number, _) in target_batch_stages.iter() {
        let l1_batch = create_l1_batch(number.0);
        storage
            .blocks_dal()
            .insert_l1_batch(&l1_batch, &[], BlockGasCount::default(), &[], &[], 0)
            .await
            .unwrap();
    }

    let mut client = MockMainNodeClient::default();
    client.insert_responses(&target_batch_stages);

    let (changes_sender, mut changes_receiver) = mpsc::unbounded_channel();
    let mut updater =
        BatchStatusUpdater::from_parts(Box::new(client), pool.clone(), Duration::from_millis(10))
            .await
            .unwrap();
    updater.changes_sender = changes_sender;
    assert_eq!(updater.last_committed_l1_batch, L1BatchNumber(0));
    assert_eq!(updater.last_proven_l1_batch, L1BatchNumber(0));
    assert_eq!(updater.last_executed_l1_batch, L1BatchNumber(0));

    let (stop_sender, stop_receiver) = watch::channel(false);
    let updater_task = tokio::spawn(updater.run(stop_receiver));

    loop {
        let changes = changes_receiver.recv().await.unwrap();
        observed_batch_stages.update(&changes);
        if observed_batch_stages == target_batch_stages {
            break;
        }
    }

    stop_sender.send_replace(true);
    updater_task.await.unwrap().expect("updater failed");
}
