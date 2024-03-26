//! High-level sync layer tests.

use std::{
    iter,
    sync::Arc,
    time::{Duration, Instant},
};

use test_casing::test_casing;
use tokio::{sync::watch, task::JoinHandle};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{
    api,
    block::MiniblockHasher,
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    snapshots::SnapshotRecoveryStatus,
    Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, Transaction, H256,
};

use super::{fetcher::FetchedTransaction, sync_action::SyncAction, *};
use crate::{
    consensus::testonly::MockMainNodeClient,
    genesis::{insert_genesis_batch, GenesisParams},
    state_keeper::{
        seal_criteria::NoopSealer, tests::TestBatchExecutorBuilder, MiniblockSealer,
        ZkSyncStateKeeper,
    },
    utils::testonly::{create_l1_batch_metadata, create_l2_transaction, prepare_recovery_snapshot},
};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const OPERATOR_ADDRESS: Address = Address::repeat_byte(1);

fn open_l1_batch(number: u32, timestamp: u64, first_miniblock_number: u32) -> SyncAction {
    SyncAction::OpenBatch {
        number: L1BatchNumber(number),
        timestamp,
        l1_gas_price: 2,
        l2_fair_gas_price: 3,
        fair_pubdata_price: Some(4),
        operator_address: OPERATOR_ADDRESS,
        protocol_version: ProtocolVersionId::latest(),
        first_miniblock_info: (MiniblockNumber(first_miniblock_number), 1),
    }
}

#[derive(Debug)]
pub(super) struct StateKeeperHandles {
    pub stop_sender: watch::Sender<bool>,
    pub sync_state: SyncState,
    pub task: JoinHandle<anyhow::Result<()>>,
}

impl StateKeeperHandles {
    /// `tx_hashes` are grouped by the L1 batch.
    pub async fn new(
        pool: ConnectionPool<Core>,
        main_node_client: MockMainNodeClient,
        actions: ActionQueue,
        tx_hashes: &[&[H256]],
    ) -> Self {
        assert!(!tx_hashes.is_empty());
        assert!(tx_hashes.iter().all(|tx_hashes| !tx_hashes.is_empty()));

        let sync_state = SyncState::default();
        let (miniblock_sealer, miniblock_sealer_handle) = MiniblockSealer::new(pool.clone(), 5);
        tokio::spawn(miniblock_sealer.run());
        let io = ExternalIO::new(
            miniblock_sealer_handle,
            pool,
            actions,
            sync_state.clone(),
            Box::new(main_node_client),
            Address::repeat_byte(1),
            u32::MAX,
            L2ChainId::default(),
        )
        .await
        .unwrap();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let mut batch_executor_base = TestBatchExecutorBuilder::default();
        for &tx_hashes_in_l1_batch in tx_hashes {
            batch_executor_base.push_successful_transactions(tx_hashes_in_l1_batch);
        }

        let state_keeper = ZkSyncStateKeeper::new(
            stop_receiver,
            Box::new(io),
            Box::new(batch_executor_base),
            Arc::new(NoopSealer),
        );
        Self {
            stop_sender,
            sync_state,
            task: tokio::spawn(state_keeper.run()),
        }
    }

    /// Waits for the given condition.
    pub async fn wait(self, mut condition: impl FnMut(&SyncState) -> bool) {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for miniblock to be sealed"
            );
            if self.task.is_finished() {
                match self.task.await {
                    Err(err) => panic!("State keeper panicked: {}", err),
                    Ok(Err(err)) => panic!("State keeper finished with an error: {}", err),
                    Ok(Ok(())) => unreachable!(),
                }
            }
            if condition(&self.sync_state) {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        self.stop_sender.send_replace(true);
        self.task.await.unwrap().unwrap();
    }
}

async fn ensure_genesis(storage: &mut Connection<'_, Core>) {
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        insert_genesis_batch(storage, &GenesisParams::mock())
            .await
            .unwrap();
    }
}

fn extract_tx_hashes<'a>(actions: impl IntoIterator<Item = &'a SyncAction>) -> Vec<H256> {
    actions
        .into_iter()
        .filter_map(|action| {
            if let SyncAction::Tx(tx) = action {
                Some(tx.hash())
            } else {
                None
            }
        })
        .collect()
}

/// Returns a mock snapshot recovery status equivalent to "recovering" from the genesis block.
fn genesis_snapshot_recovery_status() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(0),
        l1_batch_root_hash: H256::zero(), // unused
        l1_batch_timestamp: 0,
        miniblock_number: MiniblockNumber(0),
        miniblock_hash: MiniblockHasher::legacy_hash(MiniblockNumber(0)),
        miniblock_timestamp: 0,
        protocol_version: ProtocolVersionId::default(),
        storage_logs_chunks_processed: vec![],
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_basics(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };

    let open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.miniblock_timestamp + 1,
        snapshot.miniblock_number.0 + 1,
    );
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = FetchedTransaction::new(tx.into());
    let actions = vec![open_l1_batch, tx.into(), SyncAction::SealMiniblock];

    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        client,
        action_queue,
        &[&extract_tx_hashes(&actions)],
    )
    .await;
    actions_sender.push_actions(actions).await;
    // Wait until the miniblock is sealed.
    state_keeper
        .wait(|state| state.get_local_block() == snapshot.miniblock_number + 1)
        .await;

    // Check that the miniblock is persisted.
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(snapshot.miniblock_number + 1)
        .await
        .unwrap()
        .expect("New miniblock is not persisted");
    assert_eq!(miniblock.timestamp, snapshot.miniblock_timestamp + 1);

    let expected_fee_input =
        BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            fair_l2_gas_price: 3,
            fair_pubdata_price: 4,
            l1_gas_price: 2,
        });

    assert_eq!(miniblock.batch_fee_input, expected_fee_input);
    assert_eq!(miniblock.l1_tx_count, 0);
    assert_eq!(miniblock.l2_tx_count, 1);

    let tx_receipt = storage
        .transactions_web3_dal()
        .get_transaction_receipts(&[tx_hash])
        .await
        .unwrap()
        .get(0)
        .cloned()
        .expect("Transaction not persisted");
    assert_eq!(
        tx_receipt.block_number,
        (snapshot.miniblock_number.0 + 1).into()
    );
    assert_eq!(tx_receipt.transaction_index, 0.into());
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_works_without_local_protocol_version(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };

    let mut open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.miniblock_timestamp + 1,
        snapshot.miniblock_number.0 + 1,
    );
    if let SyncAction::OpenBatch {
        protocol_version, ..
    } = &mut open_l1_batch
    {
        *protocol_version = ProtocolVersionId::next();
    } else {
        unreachable!();
    };

    let tx = create_l2_transaction(10, 100);
    let tx = FetchedTransaction::new(tx.into());
    let actions = vec![open_l1_batch, tx.into(), SyncAction::SealMiniblock];

    let (actions_sender, action_queue) = ActionQueue::new();
    let mut client = MockMainNodeClient::default();
    let next_protocol_version = api::ProtocolVersion {
        version_id: ProtocolVersionId::next() as u16,
        timestamp: snapshot.miniblock_timestamp + 1,
        base_system_contracts: BaseSystemContractsHashes {
            bootloader: H256::repeat_byte(1),
            default_aa: H256::repeat_byte(2),
        },
        ..api::ProtocolVersion::default()
    };
    client.insert_protocol_version(next_protocol_version.clone());

    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        client,
        action_queue,
        &[&extract_tx_hashes(&actions)],
    )
    .await;
    actions_sender.push_actions(actions).await;
    // Wait until the miniblock is sealed.
    state_keeper
        .wait(|state| state.get_local_block() == snapshot.miniblock_number + 1)
        .await;

    // Check that the miniblock and the protocol version for it are persisted.
    let persisted_protocol_version = storage
        .protocol_versions_dal()
        .get_protocol_version(ProtocolVersionId::next())
        .await
        .expect("next protocol version not persisted");
    assert_eq!(
        persisted_protocol_version.timestamp,
        next_protocol_version.timestamp
    );
    assert_eq!(
        persisted_protocol_version.base_system_contracts_hashes,
        next_protocol_version.base_system_contracts
    );

    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(snapshot.miniblock_number + 1)
        .await
        .unwrap()
        .expect("New miniblock is not persisted");
    assert_eq!(miniblock.timestamp, snapshot.miniblock_timestamp + 1);
    assert_eq!(miniblock.protocol_version, Some(ProtocolVersionId::next()));
}

pub(super) async fn run_state_keeper_with_multiple_miniblocks(
    pool: ConnectionPool<Core>,
    snapshot_recovery: bool,
) -> (SnapshotRecoveryStatus, Vec<H256>) {
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };
    drop(storage);

    let open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.miniblock_timestamp + 1,
        snapshot.miniblock_number.0 + 1,
    );
    let txs = (0..5).map(|_| {
        let tx = create_l2_transaction(10, 100);
        FetchedTransaction::new(tx.into()).into()
    });
    let first_miniblock_actions: Vec<_> = iter::once(open_l1_batch)
        .chain(txs)
        .chain([SyncAction::SealMiniblock])
        .collect();

    let open_miniblock = SyncAction::Miniblock {
        number: snapshot.miniblock_number + 2,
        timestamp: snapshot.miniblock_timestamp + 2,
        virtual_blocks: 1,
    };
    let more_txs = (0..3).map(|_| {
        let tx = create_l2_transaction(10, 100);
        FetchedTransaction::new(tx.into()).into()
    });
    let second_miniblock_actions: Vec<_> = iter::once(open_miniblock)
        .chain(more_txs)
        .chain([SyncAction::SealMiniblock])
        .collect();

    let tx_hashes = extract_tx_hashes(
        first_miniblock_actions
            .iter()
            .chain(&second_miniblock_actions),
    );
    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper = StateKeeperHandles::new(pool, client, action_queue, &[&tx_hashes]).await;
    actions_sender.push_actions(first_miniblock_actions).await;
    actions_sender.push_actions(second_miniblock_actions).await;
    // Wait until both miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == snapshot.miniblock_number + 2)
        .await;
    (snapshot, tx_hashes)
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_with_multiple_miniblocks(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let (snapshot, tx_hashes) =
        run_state_keeper_with_multiple_miniblocks(pool.clone(), snapshot_recovery).await;
    assert_eq!(tx_hashes.len(), 8);

    // Check that both miniblocks are persisted.
    let tx_hashes_by_miniblock = [
        (snapshot.miniblock_number + 1, &tx_hashes[..5]),
        (snapshot.miniblock_number + 2, &tx_hashes[5..]),
    ];
    let mut storage = pool.connection().await.unwrap();
    for (number, expected_tx_hashes) in tx_hashes_by_miniblock {
        let miniblock = storage
            .blocks_dal()
            .get_miniblock_header(number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Miniblock #{number} is not persisted"));
        assert_eq!(miniblock.l2_tx_count, expected_tx_hashes.len() as u16);
        assert_eq!(miniblock.timestamp, u64::from(number.0));

        let sync_block = storage
            .sync_dal()
            .sync_block(number, true)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Sync block #{number} is not persisted"));

        let transactions = sync_block.transactions.unwrap();
        assert_eq!(transactions.len(), expected_tx_hashes.len());
        let tx_hashes: Vec<_> = transactions.iter().map(Transaction::hash).collect();
        assert_eq!(tx_hashes, expected_tx_hashes);
    }
    drop(storage);

    test_external_io_recovery(pool, &snapshot, tx_hashes).await;
}

async fn test_external_io_recovery(
    pool: ConnectionPool<Core>,
    snapshot: &SnapshotRecoveryStatus,
    mut tx_hashes: Vec<H256>,
) {
    let new_tx = create_l2_transaction(10, 100);
    tx_hashes.push(new_tx.hash());
    let new_tx = FetchedTransaction::new(new_tx.into());

    let (actions_sender, action_queue) = ActionQueue::new();
    let client = if snapshot.l1_batch_number > L1BatchNumber(0) {
        MockMainNodeClient::for_snapshot_recovery(snapshot)
    } else {
        MockMainNodeClient::default()
    };

    let state_keeper =
        StateKeeperHandles::new(pool.clone(), client, action_queue, &[&tx_hashes]).await;
    // Check that the state keeper state is restored.
    assert_eq!(
        state_keeper.sync_state.get_local_block(),
        snapshot.miniblock_number + 2
    );

    // Send new actions and wait until the new miniblock is sealed.
    let open_miniblock = SyncAction::Miniblock {
        number: snapshot.miniblock_number + 3,
        timestamp: snapshot.miniblock_timestamp + 3,
        virtual_blocks: 1,
    };
    let actions = vec![open_miniblock, new_tx.into(), SyncAction::SealMiniblock];
    actions_sender.push_actions(actions).await;
    state_keeper
        .wait(|state| state.get_local_block() == snapshot.miniblock_number + 3)
        .await;

    let mut storage = pool.connection().await.unwrap();
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(snapshot.miniblock_number + 3)
        .await
        .unwrap()
        .expect("New miniblock is not persisted");
    assert_eq!(miniblock.l2_tx_count, 1);
    assert_eq!(miniblock.timestamp, snapshot.miniblock_timestamp + 3);
}

pub(super) async fn mock_l1_batch_hash_computation(pool: ConnectionPool<Core>, number: u32) {
    loop {
        let mut storage = pool.connection().await.unwrap();
        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        if last_l1_batch_number < Some(L1BatchNumber(number)) {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        let metadata = create_l1_batch_metadata(number);
        storage
            .blocks_dal()
            .save_l1_batch_tree_data(L1BatchNumber(number), &metadata.tree_data())
            .await
            .unwrap();
        break;
    }
}

/// Returns tx hashes of all generated transactions, grouped by the L1 batch.
pub(super) async fn run_state_keeper_with_multiple_l1_batches(
    pool: ConnectionPool<Core>,
    snapshot_recovery: bool,
) -> (SnapshotRecoveryStatus, Vec<Vec<H256>>) {
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), MiniblockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };
    drop(storage);

    let l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.miniblock_timestamp + 1,
        snapshot.miniblock_number.0 + 1,
    );
    let first_tx = create_l2_transaction(10, 100);
    let first_tx_hash = first_tx.hash();
    let first_tx = FetchedTransaction::new(first_tx.into());
    let first_l1_batch_actions = vec![l1_batch, first_tx.into(), SyncAction::SealMiniblock];

    let fictive_miniblock = SyncAction::Miniblock {
        number: snapshot.miniblock_number + 2,
        timestamp: snapshot.miniblock_timestamp + 2,
        virtual_blocks: 0,
    };
    let seal_l1_batch = SyncAction::SealBatch { virtual_blocks: 0 };
    let fictive_miniblock_actions = vec![fictive_miniblock, seal_l1_batch];

    let l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 2,
        snapshot.miniblock_timestamp + 3,
        snapshot.miniblock_number.0 + 3,
    );
    let second_tx = create_l2_transaction(10, 100);
    let second_tx_hash = second_tx.hash();
    let second_tx = FetchedTransaction::new(second_tx.into());
    let second_l1_batch_actions = vec![l1_batch, second_tx.into(), SyncAction::SealMiniblock];

    let (actions_sender, action_queue) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        MockMainNodeClient::default(),
        action_queue,
        &[&[first_tx_hash], &[second_tx_hash]],
    )
    .await;
    actions_sender.push_actions(first_l1_batch_actions).await;
    actions_sender.push_actions(fictive_miniblock_actions).await;
    actions_sender.push_actions(second_l1_batch_actions).await;

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(
        pool.clone(),
        snapshot.l1_batch_number.0 + 1,
    ));
    // Wait until the miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == snapshot.miniblock_number + 3)
        .await;
    hash_task.await.unwrap();

    (snapshot, vec![vec![first_tx_hash], vec![second_tx_hash]])
}

#[tokio::test]
async fn external_io_with_multiple_l1_batches() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    run_state_keeper_with_multiple_l1_batches(pool.clone(), false).await;

    let mut storage = pool.connection().await.unwrap();
    let l1_batch_header = storage
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("L1 batch #1 is not persisted");
    assert_eq!(l1_batch_header.timestamp, 1);
    assert_eq!(l1_batch_header.l2_tx_count, 1);

    let (first_miniblock, last_miniblock) = storage
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("Miniblock range for L1 batch #1 is not persisted");
    assert_eq!(first_miniblock, MiniblockNumber(1));
    assert_eq!(last_miniblock, MiniblockNumber(2));

    let fictive_miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(2))
        .await
        .unwrap()
        .expect("Fictive miniblock #2 is not persisted");
    assert_eq!(fictive_miniblock.timestamp, 2);
    assert_eq!(fictive_miniblock.l2_tx_count, 0);
}
