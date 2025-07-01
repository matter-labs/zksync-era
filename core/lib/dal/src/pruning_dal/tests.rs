use std::ops;

use zksync_db_connection::connection::Connection;
use zksync_types::{
    tx::IncludedTxLocation, AccountTreeId, L1BatchNumber, L2BlockNumber, L2ChainId,
    ProtocolVersion, ProtocolVersionId, StorageKey, StorageLog, H256,
};
use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics};

use super::*;
use crate::{
    storage_logs_dal::DbStorageLog,
    tests::{
        create_l1_batch_header, create_l2_block_header, create_l2_to_l1_log, mock_execution_result,
        mock_l2_transaction, mock_vm_event,
    },
    ConnectionPool, Core, CoreDal,
};

async fn insert_l2_block(
    conn: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
    l1_batch_number: L1BatchNumber,
) {
    let l2_block1 = create_l2_block_header(l2_block_number.0);
    conn.blocks_dal()
        .insert_l2_block(&l2_block1, l1_batch_number)
        .await
        .unwrap();

    insert_events(conn, l2_block_number).await;
    insert_l2_to_l1_logs(conn, l2_block_number).await;
}

async fn insert_l2_to_l1_logs(conn: &mut Connection<'_, Core>, l2_block_number: L2BlockNumber) {
    let first_location = IncludedTxLocation {
        tx_hash: H256([1; 32]),
        tx_index_in_l2_block: 0,
    };
    let first_logs = [create_l2_to_l1_log(0, 0), create_l2_to_l1_log(0, 0)];
    let second_location = IncludedTxLocation {
        tx_hash: H256([2; 32]),
        tx_index_in_l2_block: 1,
    };
    let second_logs = vec![
        create_l2_to_l1_log(0, 0),
        create_l2_to_l1_log(0, 0),
        create_l2_to_l1_log(0, 0),
    ];
    let all_logs = vec![
        (first_location, first_logs.iter().collect()),
        (second_location, second_logs.iter().collect()),
    ];
    conn.events_dal()
        .save_user_l2_to_l1_logs(l2_block_number, &all_logs)
        .await
        .unwrap();
}

async fn insert_events(conn: &mut Connection<'_, Core>, l2_block_number: L2BlockNumber) {
    let first_location = IncludedTxLocation {
        tx_hash: H256([1; 32]),
        tx_index_in_l2_block: 0,
    };
    let first_events = [mock_vm_event(0), mock_vm_event(1)];
    let second_location = IncludedTxLocation {
        tx_hash: H256([2; 32]),
        tx_index_in_l2_block: 1,
    };
    let second_events = vec![mock_vm_event(2), mock_vm_event(3), mock_vm_event(4)];
    let all_events = vec![
        (first_location, first_events.iter().collect()),
        (second_location, second_events.iter().collect()),
    ];
    conn.events_dal()
        .save_events(l2_block_number, &all_events)
        .await
        .unwrap();
}

async fn insert_l1_batch(conn: &mut Connection<'_, Core>, l1_batch_number: L1BatchNumber) {
    let mut header = create_l1_batch_header(*l1_batch_number);
    header.l1_tx_count = 3;
    header.l2_tx_count = 5;
    header.l2_to_l1_logs.push(create_l2_to_l1_log(2, 2));
    header.l2_to_l1_messages.push(vec![22; 22]);
    header.l2_to_l1_messages.push(vec![33; 33]);

    conn.blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    conn.blocks_dal()
        .set_l1_batch_hash(
            l1_batch_number,
            H256::from_low_u64_be(l1_batch_number.0.into()),
        )
        .await
        .unwrap();
}

async fn insert_realistic_l1_batches(conn: &mut Connection<'_, Core>, l1_batches_count: u32) {
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    for l1_batch_number in 0..l1_batches_count {
        insert_l1_batch(conn, L1BatchNumber(l1_batch_number)).await;
        insert_l2_block(
            conn,
            L2BlockNumber(l1_batch_number * 2),
            L1BatchNumber(l1_batch_number),
        )
        .await;
        insert_l2_block(
            conn,
            L2BlockNumber(l1_batch_number * 2 + 1),
            L1BatchNumber(l1_batch_number),
        )
        .await;
    }
}

async fn assert_l1_batches_exist(
    conn: &mut Connection<'_, Core>,
    l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
) {
    for l1_batch_number in l1_batches_range.start().0..=l1_batches_range.end().0 {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        assert!(conn
            .blocks_dal()
            .get_l2_block_header(L2BlockNumber(l1_batch_number.0 * 2))
            .await
            .unwrap()
            .is_some());

        assert!(conn
            .blocks_dal()
            .get_l2_block_header(L2BlockNumber(l1_batch_number.0 * 2 + 1))
            .await
            .unwrap()
            .is_some());

        assert!(conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()
            .is_some());
    }
}

async fn assert_l1_batches_not_exist(
    conn: &mut Connection<'_, Core>,
    l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
) {
    let all_logs = conn
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;

    for l1_batch_number in l1_batches_range.start().0..=l1_batches_range.end().0 {
        let l1_batch_number = L1BatchNumber(l1_batch_number);
        let mut l2_block_number = L2BlockNumber(l1_batch_number.0 * 2);
        assert!(conn
            .blocks_dal()
            .get_l2_block_header(l2_block_number)
            .await
            .unwrap()
            .is_none());
        let l2_block_logs: Vec<_> = all_logs
            .iter()
            .filter(|log| log.l2_block_number == l2_block_number)
            .collect();
        assert!(l2_block_logs.is_empty(), "{l2_block_logs:?}");

        l2_block_number += 1;
        assert!(conn
            .blocks_dal()
            .get_l2_block_header(l2_block_number)
            .await
            .unwrap()
            .is_none());
        let l2_block_logs: Vec<_> = all_logs
            .iter()
            .filter(|log| log.l2_block_number == l2_block_number)
            .collect();
        assert!(l2_block_logs.is_empty(), "{l2_block_logs:?}");

        assert!(conn
            .blocks_dal()
            .get_l1_batch_header(l1_batch_number)
            .await
            .unwrap()
            .is_none());
    }
}

#[tokio::test]
async fn soft_pruning_works() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    let mut transaction = conn.start_transaction().await.unwrap();

    assert_eq!(
        PruningInfo {
            last_soft_pruned: None,
            last_hard_pruned: None,
        },
        transaction.pruning_dal().get_pruning_info().await.unwrap()
    );

    transaction
        .pruning_dal()
        .insert_soft_pruning_log(L1BatchNumber(5), L2BlockNumber(11))
        .await
        .unwrap();
    assert_eq!(
        PruningInfo {
            last_soft_pruned: Some(SoftPruningInfo {
                l2_block: L2BlockNumber(11),
                l1_batch: L1BatchNumber(5),
            }),
            last_hard_pruned: None,
        },
        transaction.pruning_dal().get_pruning_info().await.unwrap()
    );

    transaction
        .pruning_dal()
        .insert_soft_pruning_log(L1BatchNumber(10), L2BlockNumber(21))
        .await
        .unwrap();
    assert_eq!(
        PruningInfo {
            last_soft_pruned: Some(SoftPruningInfo {
                l2_block: L2BlockNumber(21),
                l1_batch: L1BatchNumber(10),
            }),
            last_hard_pruned: None,
        },
        transaction.pruning_dal().get_pruning_info().await.unwrap()
    );

    transaction
        .pruning_dal()
        .insert_hard_pruning_log(L1BatchNumber(10), L2BlockNumber(21), H256::repeat_byte(23))
        .await
        .unwrap();
    assert_eq!(
        PruningInfo {
            last_soft_pruned: Some(SoftPruningInfo {
                l2_block: L2BlockNumber(21),
                l1_batch: L1BatchNumber(10),
            }),
            last_hard_pruned: Some(HardPruningInfo {
                l2_block: L2BlockNumber(21),
                l1_batch: L1BatchNumber(10),
                l1_batch_root_hash: Some(H256::repeat_byte(23)),
            }),
        },
        transaction.pruning_dal().get_pruning_info().await.unwrap()
    );
}

fn random_storage_log(hashed_key_seed: u8, value_seed: u8) -> StorageLog {
    let key = StorageKey::new(
        AccountTreeId::from_fixed_bytes([hashed_key_seed; 20]),
        H256([hashed_key_seed; 32]),
    );
    StorageLog::new_write_log(key, H256([value_seed; 32]))
}

async fn insert_l2_block_storage_logs(
    conn: &mut Connection<'_, Core>,
    l2_block_number: L2BlockNumber,
    storage_logs: Vec<StorageLog>,
) {
    conn.storage_logs_dal()
        .insert_storage_logs(l2_block_number, &storage_logs)
        .await
        .unwrap();
}

fn assert_l2_block_storage_logs_equal(
    l2_block_number: L2BlockNumber,
    actual_logs: &[DbStorageLog],
    expected_logs: &[StorageLog],
) {
    let actual_logs_for_l2_block: Vec<(H256, H256)> = actual_logs
        .iter()
        .filter_map(|log| {
            (log.l2_block_number == l2_block_number).then_some((log.hashed_key, log.value))
        })
        .collect();
    let expected_logs: Vec<(H256, H256)> = expected_logs
        .iter()
        .map(|log| (log.key.hashed_key(), log.value))
        .collect();
    assert_eq!(
        expected_logs, actual_logs_for_l2_block,
        "logs don't match at L2 block {l2_block_number}"
    )
}

#[tokio::test]
async fn storage_logs_pruning_works_correctly() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    let mut conn = pool.connection().await.unwrap();
    let mut transaction = conn.start_transaction().await.unwrap();
    insert_realistic_l1_batches(&mut transaction, 10).await;
    insert_l2_block_storage_logs(
        &mut transaction,
        L2BlockNumber(1),
        vec![random_storage_log(1, 1)],
    )
    .await;

    insert_l2_block_storage_logs(
        &mut transaction,
        L2BlockNumber(0),
        // first storage will be overwritten in 1st L2 block,
        // the second one should be kept throughout the pruning
        // the third one will be overwritten in 10th L2 block
        vec![
            random_storage_log(1, 2),
            random_storage_log(2, 3),
            random_storage_log(3, 4),
        ],
    )
    .await;

    insert_l2_block_storage_logs(
        &mut transaction,
        L2BlockNumber(15),
        // this storage log overrides log from block 0
        vec![random_storage_log(3, 5)],
    )
    .await;

    insert_l2_block_storage_logs(
        &mut transaction,
        L2BlockNumber(17),
        // there are two logs with the same hashed key, the second one should be overwritten
        vec![random_storage_log(5, 5), random_storage_log(5, 7)],
    )
    .await;

    let stats = transaction
        .pruning_dal()
        .hard_prune_batches_range(L1BatchNumber(4), L2BlockNumber(9))
        .await
        .unwrap();
    let actual_logs = transaction
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;

    assert_l2_block_storage_logs_equal(
        L2BlockNumber(0),
        &actual_logs,
        &[random_storage_log(2, 3), random_storage_log(3, 4)],
    );
    assert_l2_block_storage_logs_equal(L2BlockNumber(1), &actual_logs, &[random_storage_log(1, 1)]);
    assert_eq!(stats.deleted_storage_logs, 1);

    let stats = transaction
        .pruning_dal()
        .hard_prune_batches_range(L1BatchNumber(9), L2BlockNumber(19))
        .await
        .unwrap();
    let actual_logs = transaction
        .storage_logs_dal()
        .dump_all_storage_logs_for_tests()
        .await;

    assert_l2_block_storage_logs_equal(L2BlockNumber(0), &actual_logs, &[random_storage_log(2, 3)]);
    assert_l2_block_storage_logs_equal(L2BlockNumber(1), &actual_logs, &[random_storage_log(1, 1)]);
    assert_l2_block_storage_logs_equal(
        L2BlockNumber(15),
        &actual_logs,
        &[random_storage_log(3, 5)],
    );
    assert_l2_block_storage_logs_equal(
        L2BlockNumber(17),
        &actual_logs,
        &[random_storage_log(5, 7)],
    );
    assert_eq!(stats.deleted_storage_logs, 2);
}

#[tokio::test]
async fn l1_batches_can_be_hard_pruned() {
    let pool = ConnectionPool::<Core>::test_pool().await;

    let mut conn = pool.connection().await.unwrap();
    let mut transaction = conn.start_transaction().await.unwrap();
    insert_realistic_l1_batches(&mut transaction, 10).await;

    assert_l1_batches_exist(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(9)).await;
    assert!(transaction
        .pruning_dal()
        .get_pruning_info()
        .await
        .unwrap()
        .last_hard_pruned
        .is_none());

    transaction
        .pruning_dal()
        .hard_prune_batches_range(L1BatchNumber(5), L2BlockNumber(11))
        .await
        .unwrap();

    assert_l1_batches_not_exist(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(5)).await;
    assert_l1_batches_exist(&mut transaction, L1BatchNumber(6)..=L1BatchNumber(9)).await;

    let stats = transaction
        .pruning_dal()
        .hard_prune_batches_range(L1BatchNumber(9), L2BlockNumber(19))
        .await
        .unwrap();
    assert_eq!(stats.deleted_l1_batches, 4);
    assert_eq!(stats.deleted_l2_blocks, 8);
    assert_eq!(stats.deleted_events, 40);
    assert_eq!(stats.deleted_l2_to_l1_logs, 40);

    assert_l1_batches_not_exist(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(9)).await;
}

#[tokio::test]
async fn transactions_are_handled_correctly_after_pruning() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut conn = pool.connection().await.unwrap();
    conn.protocol_versions_dal()
        .save_protocol_version_with_tx(&ProtocolVersion::default())
        .await
        .unwrap();

    // Add an L2 block with a transaction end emulate its pruning.
    let l2_block_header = create_l2_block_header(1);
    let tx = mock_l2_transaction();
    let tx_hash = tx.hash();
    conn.transactions_dal()
        .insert_transaction_l2(
            &tx,
            TransactionExecutionMetrics::default(),
            ValidationTraces::default(),
        )
        .await
        .unwrap();
    conn.blocks_dal()
        .insert_l2_block(&l2_block_header, L1BatchNumber(1))
        .await
        .unwrap();
    conn.transactions_dal()
        .mark_txs_as_executed_in_l2_block(
            L2BlockNumber(1),
            &[mock_execution_result(tx.clone())],
            1.into(),
            ProtocolVersionId::latest(),
            false,
        )
        .await
        .unwrap();

    let affected_count = conn
        .pruning_dal()
        .clear_transaction_fields(L2BlockNumber(1)..=L2BlockNumber(1))
        .await
        .unwrap();
    assert_eq!(affected_count, 1);

    let api_transactions = conn
        .transactions_web3_dal()
        .get_transactions(&[tx_hash], L2ChainId::default())
        .await
        .unwrap();
    assert!(api_transactions.is_empty(), "{api_transactions:?}");

    let transaction_receipts = conn
        .transactions_web3_dal()
        .get_transaction_receipts(&[tx_hash])
        .await
        .unwrap();
    assert!(transaction_receipts.is_empty(), "{transaction_receipts:?}");

    let transaction_details = conn
        .transactions_web3_dal()
        .get_transaction_details(tx_hash)
        .await
        .unwrap();
    assert!(transaction_details.is_none(), "{transaction_details:?}");
}
