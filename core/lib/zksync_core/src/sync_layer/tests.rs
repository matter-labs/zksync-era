//! High-level sync layer tests.

use tokio::{sync::watch, task::JoinHandle};

use std::{
    iter,
    time::{Duration, Instant},
};

use super::{
    sync_action::{ActionQueueSender, SyncAction},
    *,
};
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        tests::{create_l1_batch_metadata, create_l2_transaction, TestBatchExecutorBuilder},
        ZkSyncStateKeeper,
    },
};
use db_test_macro::db_test;
use zksync_dal::ConnectionPool;
use zksync_types::{Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, H256};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

fn open_l1_batch(number: u32, timestamp: u64, first_miniblock_number: u32) -> SyncAction {
    SyncAction::OpenBatch {
        number: L1BatchNumber(number),
        timestamp,
        l1_gas_price: 2,
        l2_fair_gas_price: 3,
        operator_address: Default::default(),
        protocol_version: ProtocolVersionId::latest(),
        first_miniblock_info: (MiniblockNumber(first_miniblock_number), 1),
        prev_miniblock_hash: H256::default(),
    }
}

#[derive(Debug)]
struct StateKeeperHandles {
    actions_sender: ActionQueueSender,
    stop_sender: watch::Sender<bool>,
    sync_state: SyncState,
    task: JoinHandle<anyhow::Result<()>>,
}

impl StateKeeperHandles {
    async fn wait(self, mut condition: impl FnMut(&SyncState) -> bool) {
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

/// `tx_hashes` are grouped by the L1 batch.
async fn run_state_keeper(pool: ConnectionPool, tx_hashes: &[&[H256]]) -> StateKeeperHandles {
    assert!(!tx_hashes.is_empty());
    assert!(tx_hashes.iter().all(|tx_hashes| !tx_hashes.is_empty()));

    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    drop(storage);

    let (actions_sender, actions) = ActionQueue::new();
    let sync_state = SyncState::new();
    let io = ExternalIO::new(
        pool,
        actions,
        sync_state.clone(),
        String::new(), // `main_node_url`; not used
        Address::repeat_byte(1),
        u32::MAX,
        L2ChainId::default(),
    )
    .await;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut batch_executor_base = TestBatchExecutorBuilder::default();
    for &tx_hashes_in_l1_batch in tx_hashes {
        batch_executor_base.push_successful_transactions(tx_hashes_in_l1_batch);
    }

    let state_keeper = ZkSyncStateKeeper::without_sealer(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
    );
    StateKeeperHandles {
        actions_sender,
        stop_sender,
        sync_state,
        task: tokio::spawn(state_keeper.run()),
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

#[db_test]
async fn external_io_basics(pool: ConnectionPool) {
    let open_l1_batch = open_l1_batch(1, 1, 1);
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = SyncAction::Tx(Box::new(tx.into()));
    let actions = vec![open_l1_batch, tx, SyncAction::SealMiniblock];

    let state_keeper = run_state_keeper(pool.clone(), &[&extract_tx_hashes(&actions)]).await;
    state_keeper.actions_sender.push_actions(actions).await;
    // Wait until the miniblock is sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(1))
        .await;

    // Check that the miniblock is persisted.
    let mut storage = pool.access_storage().await.unwrap();
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(1))
        .await
        .unwrap()
        .expect("Miniblock #1 is not persisted");
    assert_eq!(miniblock.timestamp, 1);
    assert_eq!(miniblock.l1_gas_price, 2);
    assert_eq!(miniblock.l2_fair_gas_price, 3);
    assert_eq!(miniblock.l1_tx_count, 0);
    assert_eq!(miniblock.l2_tx_count, 1);

    let tx_receipt = storage
        .transactions_web3_dal()
        .get_transaction_receipt(tx_hash)
        .await
        .unwrap()
        .expect("Transaction not persisted");
    assert_eq!(tx_receipt.block_number, Some(1.into()));
    assert_eq!(tx_receipt.transaction_index, 0.into());
}

#[db_test]
async fn external_io_with_multiple_miniblocks(pool: ConnectionPool) {
    let open_l1_batch = open_l1_batch(1, 1, 1);
    let txs = (0..5).map(|_| {
        let tx = create_l2_transaction(10, 100);
        SyncAction::Tx(Box::new(tx.into()))
    });
    let first_miniblock_actions: Vec<_> = iter::once(open_l1_batch)
        .chain(txs)
        .chain([SyncAction::SealMiniblock])
        .collect();

    let open_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(2),
        timestamp: 2,
        virtual_blocks: 1,
    };
    let more_txs = (0..3).map(|_| {
        let tx = create_l2_transaction(10, 100);
        SyncAction::Tx(Box::new(tx.into()))
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
    let state_keeper = run_state_keeper(pool.clone(), &[&tx_hashes]).await;
    state_keeper
        .actions_sender
        .push_actions(first_miniblock_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(second_miniblock_actions)
        .await;
    // Wait until both miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(2))
        .await;

    // Check that both miniblocks are persisted.
    let miniblock_number_to_tx_count = [(1, 5), (2, 3)];

    let mut storage = pool.access_storage().await.unwrap();
    for (number, expected_tx_count) in miniblock_number_to_tx_count {
        let miniblock = storage
            .blocks_dal()
            .get_miniblock_header(MiniblockNumber(number))
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Miniblock #{} is not persisted", number));
        assert_eq!(miniblock.l2_tx_count, expected_tx_count);
        assert_eq!(miniblock.timestamp, u64::from(number));
    }
    drop(storage);

    test_external_io_recovery(pool, tx_hashes).await;
}

async fn test_external_io_recovery(pool: ConnectionPool, mut tx_hashes: Vec<H256>) {
    let new_tx = create_l2_transaction(10, 100);
    tx_hashes.push(new_tx.hash());
    let new_tx = SyncAction::Tx(Box::new(new_tx.into()));

    let state_keeper = run_state_keeper(pool.clone(), &[&tx_hashes]).await;
    // Check that the state keeper state is restored.
    assert_eq!(
        state_keeper.sync_state.get_local_block(),
        MiniblockNumber(2)
    );

    // Send new actions and wait until the new miniblock is sealed.
    let open_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(3),
        timestamp: 3,
        virtual_blocks: 1,
    };
    let actions = vec![open_miniblock, new_tx, SyncAction::SealMiniblock];
    state_keeper.actions_sender.push_actions(actions).await;
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(3))
        .await;

    let mut storage = pool.access_storage().await.unwrap();
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(3))
        .await
        .unwrap()
        .expect("Miniblock #3 is not persisted");
    assert_eq!(miniblock.l2_tx_count, 1);
    assert_eq!(miniblock.timestamp, 3);
}

async fn mock_l1_batch_hash_computation(pool: ConnectionPool, number: u32) {
    loop {
        let mut storage = pool.access_storage().await.unwrap();
        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        if last_l1_batch_number < L1BatchNumber(number) {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        let metadata = create_l1_batch_metadata(number);
        storage
            .blocks_dal()
            .save_l1_batch_metadata(L1BatchNumber(1), &metadata, H256::zero())
            .await
            .unwrap();
        break;
    }
}

#[db_test]
async fn external_io_with_multiple_l1_batches(pool: ConnectionPool) {
    let l1_batch = open_l1_batch(1, 1, 1);
    let first_tx = create_l2_transaction(10, 100);
    let first_tx_hash = first_tx.hash();
    let first_tx = SyncAction::Tx(Box::new(first_tx.into()));
    let first_l1_batch_actions = vec![l1_batch, first_tx, SyncAction::SealMiniblock];

    let fictive_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(2),
        timestamp: 2,
        virtual_blocks: 0,
    };
    let seal_l1_batch = SyncAction::SealBatch { virtual_blocks: 0 };
    let fictive_miniblock_actions = vec![fictive_miniblock, seal_l1_batch];

    let l1_batch = open_l1_batch(2, 3, 3);
    let second_tx = create_l2_transaction(10, 100);
    let second_tx_hash = second_tx.hash();
    let second_tx = SyncAction::Tx(Box::new(second_tx.into()));
    let second_l1_batch_actions = vec![l1_batch, second_tx, SyncAction::SealMiniblock];

    let state_keeper = run_state_keeper(pool.clone(), &[&[first_tx_hash], &[second_tx_hash]]).await;
    state_keeper
        .actions_sender
        .push_actions(first_l1_batch_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(fictive_miniblock_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(second_l1_batch_actions)
        .await;

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(pool.clone(), 1));
    // Wait until the miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(3))
        .await;
    hash_task.await.unwrap();

    let mut storage = pool.access_storage().await.unwrap();
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
