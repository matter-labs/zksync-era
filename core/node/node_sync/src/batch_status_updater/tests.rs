//! Tests for batch status updater.

use std::{future, sync::Arc};

use chrono::TimeZone;
use test_casing::{test_casing, Product};
use tokio::sync::{watch, Mutex};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l1_batch, create_l2_block, prepare_recovery_snapshot};
use zksync_types::{
    block::L1BatchTreeData,
    commitment::L1BatchCommitmentArtifacts,
    web3::{Log, TransactionReceipt},
    L2BlockNumber,
};
use zksync_web3_decl::client::{MockClient, L1};

use super::*;
use crate::metrics::L1BatchStage;

async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: L1BatchNumber) {
    let mut storage = storage.start_transaction().await.unwrap();
    // Insert a mock L2 block so that `get_block_details()` will return values.
    let l2_block = create_l2_block(number.0);
    storage
        .blocks_dal()
        .insert_l2_block(&l2_block)
        .await
        .unwrap();

    let l1_batch = create_l1_batch(number.0);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&l1_batch)
        .await
        .unwrap();

    storage
        .blocks_dal()
        .save_l1_batch_tree_data(
            number,
            &L1BatchTreeData {
                hash: H256::zero(),
                rollup_last_leaf_index: 0,
            },
        )
        .await
        .unwrap();

    storage
        .blocks_dal()
        .save_l1_batch_commitment_artifacts(number, &L1BatchCommitmentArtifacts::default())
        .await
        .unwrap();

    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(number)
        .await
        .unwrap();
    storage.commit().await.unwrap();
}

/// Mapping `L1BatchNumber` -> `L1BatchStage` for a continuous range of numbers.
#[derive(Debug, Clone, Default, PartialEq)]
struct L1BatchStagesMap {
    first_batch_number: L1BatchNumber,
    stages: Vec<L1BatchStage>,
}

impl L1BatchStagesMap {
    fn empty(first_batch_number: L1BatchNumber, len: usize) -> Self {
        Self {
            first_batch_number,
            stages: vec![L1BatchStage::Open; len],
        }
    }

    fn new(first_batch_number: L1BatchNumber, stages: Vec<L1BatchStage>) -> Self {
        assert!(stages.windows(2).all(|window| {
            let [prev, next] = window else { unreachable!() };
            prev >= next
        }));
        Self {
            first_batch_number,
            stages,
        }
    }

    fn get(&self, number: L1BatchNumber) -> Option<L1BatchStage> {
        let index = number.0.checked_sub(self.first_batch_number.0)?;
        self.stages.get(index as usize).copied()
    }

    fn iter(&self) -> impl Iterator<Item = (L1BatchNumber, L1BatchStage)> + '_ {
        self.stages
            .iter()
            .enumerate()
            .map(|(i, &stage)| (self.first_batch_number + i as u32, stage))
    }

    fn update(&mut self, changes: &StatusChanges) {
        self.update_to_stage(&changes.commit, L1BatchStage::Committed);
        self.update_to_stage(&changes.prove, L1BatchStage::Proven);
        self.update_to_stage(&changes.execute, L1BatchStage::Executed);
    }

    fn update_to_stage(&mut self, batch_changes: &[BatchStatusChange], target: L1BatchStage) {
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

    async fn assert_storage(&self, storage: &mut Connection<'_, Core>) {
        for (number, stage) in self.iter() {
            let local_details = storage
                .blocks_web3_dal()
                .get_l1_batch_details(L1BatchNumber(number.0))
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("no details for block #{number}"));
            let expected_details = mock_batch_details(number.0, stage);

            assert_eq!(
                local_details.base.commit_tx_hash,
                expected_details.base.commit_tx_hash
            );
            assert_eq!(
                local_details.base.committed_at,
                expected_details.base.committed_at
            );
            assert_eq!(
                local_details.base.commit_chain_id,
                expected_details.base.commit_chain_id,
            );
            assert_eq!(
                local_details.base.prove_tx_hash,
                expected_details.base.prove_tx_hash
            );
            assert_eq!(
                local_details.base.proven_at,
                expected_details.base.proven_at
            );
            assert_eq!(
                local_details.base.prove_chain_id,
                expected_details.base.prove_chain_id,
            );
            assert_eq!(
                local_details.base.execute_tx_hash,
                expected_details.base.execute_tx_hash
            );
            assert_eq!(
                local_details.base.executed_at,
                expected_details.base.executed_at
            );
            assert_eq!(
                local_details.base.execute_chain_id,
                expected_details.base.execute_chain_id,
            );
        }
    }
}

fn mock_batch_details(number: u32, stage: L1BatchStage) -> api::L1BatchDetails {
    api::L1BatchDetails {
        number: L1BatchNumber(number),
        base: api::BlockDetailsBase {
            timestamp: number.into(),
            l1_tx_count: 0,
            l2_tx_count: 0,
            root_hash: Some(H256::zero()),
            status: api::BlockStatus::Sealed,
            commit_tx_hash: (stage >= L1BatchStage::Committed).then(|| {
                let mut h = [0u8; 32];
                h[0] = 1;
                h[28..].copy_from_slice(&number.to_be_bytes());
                H256::from(h)
            }),
            committed_at: (stage >= L1BatchStage::Committed)
                .then(|| Utc.timestamp_opt(100, 0).unwrap()),
            commit_chain_id: (stage >= L1BatchStage::Committed).then_some(SLChainId(1)),
            prove_tx_hash: (stage >= L1BatchStage::Proven).then(|| {
                let mut h = [0u8; 32];
                h[0] = 2;
                h[28..].copy_from_slice(&number.to_be_bytes());
                H256::from(h)
            }),
            proven_at: (stage >= L1BatchStage::Proven).then(|| Utc.timestamp_opt(200, 0).unwrap()),
            prove_chain_id: (stage >= L1BatchStage::Proven).then_some(SLChainId(1)),
            execute_tx_hash: (stage >= L1BatchStage::Executed).then(|| {
                let mut h = [0u8; 32];
                h[0] = 3;
                h[28..].copy_from_slice(&number.to_be_bytes());
                H256::from(h)
            }),
            executed_at: (stage >= L1BatchStage::Executed)
                .then(|| Utc.timestamp_opt(300, 0).unwrap()),
            execute_chain_id: (stage >= L1BatchStage::Executed).then_some(SLChainId(1)),
            l1_gas_price: 1,
            l2_fair_gas_price: 2,
            fair_pubdata_price: None,
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        },
    }
}

#[derive(Debug, Default)]
struct MockMainNodeClient(Arc<Mutex<L1BatchStagesMap>>);

impl From<L1BatchStagesMap> for MockMainNodeClient {
    fn from(map: L1BatchStagesMap) -> Self {
        Self(Arc::new(Mutex::new(map)))
    }
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn batch_details(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<api::L1BatchDetails>> {
        let map = self.0.lock().await;
        let Some(stage) = map.get(L1BatchNumber(number.0)) else {
            return Ok(None);
        };
        Ok(Some(mock_batch_details(number.0, stage)))
    }
}

fn mock_change(number: L1BatchNumber) -> BatchStatusChange {
    BatchStatusChange {
        number,
        l1_tx_hash: H256::zero(),
        happened_at: DateTime::default(),
        sl_chain_id: Some(SLChainId(0)),
    }
}

const MOCK_DIAMON_PROXY_ADDRESS: zksync_types::H160 = Address::repeat_byte(0x42);

fn new_mock_eth_interface() -> Box<dyn EthInterface> {
    let contract = zksync_contracts::hyperchain_contract();
    Box::new(
        MockClient::builder(L1::default())
            .method("eth_getTransactionReceipt", move |tx_hash: H256| {
                // Extract the batch number from the tx hash
                // The batch number is stored in the last 4 bytes
                let bytes = tx_hash.as_bytes();
                let tx_type = bytes[0]; // 1 for commit, 2 for prove, 3 for execute

                // Extract batch number from the last 4 bytes
                let mut batch_number_bytes = [0u8; 4];
                batch_number_bytes.copy_from_slice(&bytes[28..32]);
                let batch_number = u32::from_be_bytes(batch_number_bytes);

                let topics: Vec<H256> = match tx_type {
                    1 => {
                        //BlockCommit (index_topic_1 uint256 blockNumber, index_topic_2 bytes32 blockHash, index_topic_3 bytes32 commitment)
                        let event = contract.event("BlockCommit").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be(batch_number.into()),
                            H256::zero(),
                            H256::zero(),
                        ]
                    }
                    2 => {
                        // BlocksVerification (index_topic_1 uint256 previousLastVerifiedBlock, index_topic_2 uint256 currentLastVerifiedBlock
                        let event = contract.event("BlocksVerification").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be((batch_number - 1).into()),
                            H256::from_low_u64_be(batch_number.into()),
                        ]
                    }
                    3 => {
                        // BlockExecution (index_topic_1 uint256 blockNumber, index_topic_2 bytes32 blockHash, index_topic_3 bytes32 commitment)
                        let event = contract.event("BlockExecution").unwrap();
                        vec![
                            event.signature(),
                            H256::from_low_u64_be(batch_number.into()),
                            H256::zero(),
                            H256::zero(),
                        ]
                    }
                    _ => return Ok(None),
                };

                // Create a receipt with status 1 (success)
                let receipt = TransactionReceipt {
                    status: Some(U64::one()),
                    logs: vec![Log {
                        address: MOCK_DIAMON_PROXY_ADDRESS,
                        topics,
                        data: vec![].into(),
                        block_hash: None,
                        block_number: None,
                        transaction_hash: None,
                        transaction_index: None,
                        log_index: None,
                        transaction_log_index: None,
                        log_type: Some("Regular".to_string()),
                        removed: None,
                        block_timestamp: None,
                    }],
                    ..Default::default()
                };

                Ok(Some(receipt))
            })
            .build(),
    )
}

fn mock_updater(
    client: MockMainNodeClient,
    pool: ConnectionPool<Core>,
) -> (BatchStatusUpdater, mpsc::UnboundedReceiver<StatusChanges>) {
    let (changes_sender, changes_receiver) = mpsc::unbounded_channel();

    let mut updater = BatchStatusUpdater::from_parts(
        Box::new(client),
        new_mock_eth_interface(),
        MOCK_DIAMON_PROXY_ADDRESS,
        pool,
        Duration::from_millis(10),
        1u64.into(),
    );
    updater.changes_sender = changes_sender;
    (updater, changes_receiver)
}

#[tokio::test]
async fn updater_cursor_for_storage_with_genesis_block() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    for number in [1, 2] {
        seal_l1_batch(&mut storage, L1BatchNumber(number)).await;
    }

    let mut cursor = UpdaterCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.last_committed_l1_batch, L1BatchNumber(0));
    assert_eq!(cursor.last_proven_l1_batch, L1BatchNumber(0));
    assert_eq!(cursor.last_executed_l1_batch, L1BatchNumber(0));

    let (updater, _) = mock_updater(MockMainNodeClient::default(), pool.clone());
    let changes = StatusChanges {
        commit: vec![mock_change(L1BatchNumber(1)), mock_change(L1BatchNumber(2))],
        prove: vec![mock_change(L1BatchNumber(1))],
        execute: vec![],
    };
    updater
        .apply_status_changes(&mut cursor, changes)
        .await
        .unwrap();

    assert_eq!(cursor.last_committed_l1_batch, L1BatchNumber(2));
    assert_eq!(cursor.last_proven_l1_batch, L1BatchNumber(1));
    assert_eq!(cursor.last_executed_l1_batch, L1BatchNumber(0));

    let restored_cursor = UpdaterCursor::new(&mut storage).await.unwrap();
    assert_eq!(restored_cursor, cursor);
}

#[tokio::test]
async fn updater_cursor_after_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;

    let cursor = UpdaterCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.last_committed_l1_batch, L1BatchNumber(23));
    assert_eq!(cursor.last_proven_l1_batch, L1BatchNumber(23));
    assert_eq!(cursor.last_executed_l1_batch, L1BatchNumber(23));
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[tokio::test]
async fn normal_updater_operation(snapshot_recovery: bool, async_batches: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let first_batch_number = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;
        L1BatchNumber(24)
    } else {
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        L1BatchNumber(1)
    };

    let target_batch_stages = L1BatchStagesMap::new(
        first_batch_number,
        vec![
            L1BatchStage::Executed,
            L1BatchStage::Proven,
            L1BatchStage::Proven,
            L1BatchStage::Committed,
            L1BatchStage::Committed,
            L1BatchStage::Open,
        ],
    );
    let batch_numbers: Vec<_> = target_batch_stages
        .iter()
        .map(|(number, _)| number)
        .collect();

    if !async_batches {
        // Make all L1 batches present in the storage from the start.
        for &number in &batch_numbers {
            seal_l1_batch(&mut storage, number).await;
        }
    }

    let client = MockMainNodeClient::from(target_batch_stages.clone());
    let (updater, mut changes_receiver) = mock_updater(client, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let updater_task = tokio::spawn(updater.run(stop_receiver));

    let batches_task = if async_batches {
        let pool = pool.clone();
        tokio::spawn(async move {
            let mut storage = pool.connection().await.unwrap();
            for &number in &batch_numbers {
                seal_l1_batch(&mut storage, number).await;
                tokio::time::sleep(Duration::from_millis(15)).await;
            }
        })
    } else {
        tokio::spawn(future::ready(()))
    };

    let mut observed_batch_stages =
        L1BatchStagesMap::empty(first_batch_number, target_batch_stages.stages.len());
    loop {
        let changes = changes_receiver.recv().await.unwrap();
        observed_batch_stages.update(&changes);
        if observed_batch_stages == target_batch_stages {
            break;
        }
    }

    batches_task.await.unwrap();
    target_batch_stages.assert_storage(&mut storage).await;
    stop_sender.send_replace(true);
    updater_task.await.unwrap().expect("updater failed");
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn updater_with_gradual_main_node_updates(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let first_batch_number = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await;
        L1BatchNumber(24)
    } else {
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        L1BatchNumber(1)
    };

    let target_batch_stages = L1BatchStagesMap::new(
        first_batch_number,
        vec![
            L1BatchStage::Executed,
            L1BatchStage::Proven,
            L1BatchStage::Proven,
            L1BatchStage::Committed,
            L1BatchStage::Committed,
            L1BatchStage::Open,
        ],
    );
    let mut observed_batch_stages =
        L1BatchStagesMap::empty(first_batch_number, target_batch_stages.stages.len());

    for (number, _) in target_batch_stages.iter() {
        seal_l1_batch(&mut storage, number).await;
    }

    let client = MockMainNodeClient::from(observed_batch_stages.clone());

    // Gradually update information provided by the main node.
    let client_map = Arc::clone(&client.0);
    let final_stages = target_batch_stages.clone();
    let storage_task = tokio::spawn(async move {
        for max_stage in [
            L1BatchStage::Committed,
            L1BatchStage::Proven,
            L1BatchStage::Executed,
        ] {
            let mut client_map = client_map.lock().await;
            for (stage, &final_stage) in client_map.stages.iter_mut().zip(&final_stages.stages) {
                *stage = final_stage.min(max_stage);
            }
            drop(client_map);
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    });

    let (updater, mut changes_receiver) = mock_updater(client, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let updater_task = tokio::spawn(updater.run(stop_receiver));

    loop {
        let changes = changes_receiver.recv().await.unwrap();
        observed_batch_stages.update(&changes);
        if observed_batch_stages == target_batch_stages {
            break;
        }
    }

    storage_task.await.unwrap();
    target_batch_stages.assert_storage(&mut storage).await;
    stop_sender.send_replace(true);
    updater_task.await.unwrap().expect("updater failed");

    drop(storage);
    test_resuming_updater(pool, target_batch_stages).await;
}

async fn test_resuming_updater(pool: ConnectionPool<Core>, initial_batch_stages: L1BatchStagesMap) {
    let target_batch_stages = L1BatchStagesMap::new(
        initial_batch_stages.first_batch_number,
        vec![L1BatchStage::Executed; 6],
    );

    let client = MockMainNodeClient::from(target_batch_stages.clone());
    let (updater, mut changes_receiver) = mock_updater(client, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let updater_task = tokio::spawn(updater.run(stop_receiver));

    let mut observed_batch_stages = initial_batch_stages;
    loop {
        let changes = changes_receiver.recv().await.unwrap();
        observed_batch_stages.update(&changes);
        if observed_batch_stages == target_batch_stages {
            break;
        }
    }

    let mut storage = pool.connection().await.unwrap();
    target_batch_stages.assert_storage(&mut storage).await;
    stop_sender.send_replace(true);
    updater_task.await.unwrap().expect("updater failed");
}
