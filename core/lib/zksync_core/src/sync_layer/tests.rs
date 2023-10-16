//! High-level sync layer tests.

use tokio::sync::watch;

use std::time::{Duration, Instant};

use super::{sync_action::SyncAction, *};
use crate::{
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        tests::{create_l2_transaction, TestBatchExecutorBuilder},
        ZkSyncStateKeeper,
    },
};
use db_test_macro::db_test;
use zksync_dal::ConnectionPool;
use zksync_types::{Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, H256};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);

#[db_test]
async fn external_io_basics(pool: ConnectionPool) {
    let mut storage = pool.access_storage().await.unwrap();
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
    drop(storage);

    let (queue_sender, queue) = ActionQueue::new();
    let sync_state = SyncState::new();
    let io = ExternalIO::new(
        pool.clone(),
        queue,
        sync_state.clone(),
        String::new(), // `main_node_url`; not used
        Address::repeat_byte(1),
        u32::MAX,
        L2ChainId::default(),
    )
    .await;

    let open_l1_batch = SyncAction::OpenBatch {
        number: L1BatchNumber(1),
        timestamp: 1,
        l1_gas_price: 2,
        l2_fair_gas_price: 3,
        operator_address: Default::default(),
        protocol_version: ProtocolVersionId::latest(),
        first_miniblock_info: (MiniblockNumber(1), 1),
        prev_miniblock_hash: H256::default(),
    };
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = SyncAction::Tx(Box::new(tx.into()));
    queue_sender
        .push_actions(vec![open_l1_batch, tx, SyncAction::SealMiniblock])
        .await;

    let batch_executor_base = TestBatchExecutorBuilder::for_successful_transactions(&[tx_hash]);
    let (stop_sender, stop_receiver) = watch::channel(false);
    let state_keeper = ZkSyncStateKeeper::without_sealer(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
    );
    let state_keeper_task = tokio::spawn(state_keeper.run());

    // Wait until the miniblock is sealed.
    let started_at = Instant::now();
    loop {
        assert!(
            started_at.elapsed() <= TEST_TIMEOUT,
            "Timed out waiting for miniblock to be sealed"
        );
        if sync_state.get_local_block() == MiniblockNumber(1) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    stop_sender.send_replace(true);
    state_keeper_task.await.unwrap().unwrap();

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

// FIXME: test other scenarios (multiple miniblocks / L1 batches; recovery etc.)
