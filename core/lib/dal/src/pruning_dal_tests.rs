use crate::pruning_dal::PruningInfo;

#[cfg(test)]
mod tests {
    use std::ops;

    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_db_connection::connection::Connection;
    use zksync_types::{
        block::L1BatchHeader,
        l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
        tx::IncludedTxLocation,
        AccountTreeId, Address, L1BatchNumber, L2BlockNumber, ProtocolVersion, ProtocolVersionId,
        StorageKey, StorageLog, H256,
    };

    use super::*;
    use crate::{
        tests::{create_l2_block_header, mock_l2_to_l1_log, mock_vm_event},
        ConnectionPool, Core, CoreDal,
    };

    async fn insert_miniblock(
        conn: &mut Connection<'_, Core>,
        miniblock_number: L2BlockNumber,
        l1_batch_number: L1BatchNumber,
    ) {
        let miniblock1 = create_l2_block_header(miniblock_number.0);
        conn.blocks_dal()
            .insert_l2_block(&miniblock1)
            .await
            .unwrap();

        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_number)
            .await
            .unwrap();

        insert_events(conn, miniblock_number).await;
        insert_l2_to_l1_logs(conn, miniblock_number).await;
    }

    async fn insert_l2_to_l1_logs(
        conn: &mut Connection<'_, Core>,
        miniblock_number: L2BlockNumber,
    ) {
        let first_location = IncludedTxLocation {
            tx_hash: H256([1; 32]),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::default(),
        };
        let first_logs = vec![mock_l2_to_l1_log(), mock_l2_to_l1_log()];
        let second_location = IncludedTxLocation {
            tx_hash: H256([2; 32]),
            tx_index_in_miniblock: 1,
            tx_initiator_address: Address::default(),
        };
        let second_logs = vec![
            mock_l2_to_l1_log(),
            mock_l2_to_l1_log(),
            mock_l2_to_l1_log(),
        ];
        let all_logs = vec![
            (first_location, first_logs.iter().collect()),
            (second_location, second_logs.iter().collect()),
        ];
        conn.events_dal()
            .save_user_l2_to_l1_logs(miniblock_number, &all_logs)
            .await
            .unwrap();
    }

    async fn insert_events(conn: &mut Connection<'_, Core>, miniblock_number: L2BlockNumber) {
        let first_location = IncludedTxLocation {
            tx_hash: H256([1; 32]),
            tx_index_in_miniblock: 0,
            tx_initiator_address: Address::default(),
        };
        let first_events = vec![mock_vm_event(0), mock_vm_event(1)];
        let second_location = IncludedTxLocation {
            tx_hash: H256([2; 32]),
            tx_index_in_miniblock: 1,
            tx_initiator_address: Address::default(),
        };
        let second_events = vec![mock_vm_event(2), mock_vm_event(3), mock_vm_event(4)];
        let all_events = vec![
            (first_location, first_events.iter().collect()),
            (second_location, second_events.iter().collect()),
        ];
        conn.events_dal()
            .save_events(miniblock_number, &all_events)
            .await
            .unwrap();
    }

    async fn insert_l1_batch(conn: &mut Connection<'_, Core>, l1_batch_number: L1BatchNumber) {
        let mut header = L1BatchHeader::new(
            l1_batch_number,
            100,
            BaseSystemContractsHashes {
                bootloader: H256::repeat_byte(1),
                default_aa: H256::repeat_byte(42),
            },
            ProtocolVersionId::latest(),
        );
        header.l1_tx_count = 3;
        header.l2_tx_count = 5;
        header.l2_to_l1_logs.push(UserL2ToL1Log(L2ToL1Log {
            shard_id: 0,
            is_service: false,
            tx_number_in_block: 2,
            sender: Address::repeat_byte(2),
            key: H256::repeat_byte(3),
            value: H256::zero(),
        }));
        header.l2_to_l1_messages.push(vec![22; 22]);
        header.l2_to_l1_messages.push(vec![33; 33]);

        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
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
            insert_miniblock(
                conn,
                L2BlockNumber(l1_batch_number * 2),
                L1BatchNumber(l1_batch_number),
            )
            .await;
            insert_miniblock(
                conn,
                L2BlockNumber(l1_batch_number * 2 + 1),
                L1BatchNumber(l1_batch_number),
            )
            .await;
        }
    }

    async fn assert_l1_batch_objects_exists(
        conn: &mut Connection<'_, Core>,
        l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
    ) {
        for l1_batch_number in l1_batches_range.start().0..l1_batches_range.end().0 {
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

    async fn assert_l1_batch_objects_dont_exist(
        conn: &mut Connection<'_, Core>,
        l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
    ) {
        for l1_batch_number in l1_batches_range.start().0..l1_batches_range.end().0 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            assert!(conn
                .blocks_dal()
                .get_l2_block_header(L2BlockNumber(l1_batch_number.0 * 2))
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                0,
                conn.storage_logs_dal()
                    .get_miniblock_storage_logs(L2BlockNumber(l1_batch_number.0 * 2))
                    .await
                    .len()
            );

            assert!(conn
                .blocks_dal()
                .get_l2_block_header(L2BlockNumber(l1_batch_number.0 * 2 + 1))
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                0,
                conn.storage_logs_dal()
                    .get_miniblock_storage_logs(L2BlockNumber(l1_batch_number.0 * 2 + 1))
                    .await
                    .len()
            );

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
                last_soft_pruned_miniblock: None,
                last_soft_pruned_l1_batch: None,
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            transaction.pruning_dal().get_pruning_info().await.unwrap()
        );

        transaction
            .pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(5), L2BlockNumber(11))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(L2BlockNumber(11)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(5)),
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            transaction.pruning_dal().get_pruning_info().await.unwrap()
        );

        transaction
            .pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(10), L2BlockNumber(21))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(L2BlockNumber(21)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(10)),
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            transaction.pruning_dal().get_pruning_info().await.unwrap()
        );

        transaction
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), L2BlockNumber(21))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(L2BlockNumber(21)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(10)),
                last_hard_pruned_miniblock: Some(L2BlockNumber(21)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(10))
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
    async fn insert_miniblock_storage_logs(
        conn: &mut Connection<'_, Core>,
        miniblock_number: L2BlockNumber,
        storage_logs: Vec<StorageLog>,
    ) {
        conn.storage_logs_dal()
            .insert_storage_logs(miniblock_number, &[(H256::zero(), storage_logs)])
            .await
            .unwrap();
    }

    async fn assert_miniblock_storage_logs_equal(
        conn: &mut Connection<'_, Core>,
        miniblock_number: L2BlockNumber,
        expected_logs: Vec<StorageLog>,
    ) {
        let actual_logs: Vec<(H256, H256)> = conn
            .storage_logs_dal()
            .get_miniblock_storage_logs(miniblock_number)
            .await
            .iter()
            .map(|log| (log.0, log.1))
            .collect();
        let expected_logs: Vec<(H256, H256)> = expected_logs
            .iter()
            .enumerate()
            .map(|(_enumeration_number, log)| (log.key.hashed_key(), log.value))
            .collect();
        assert_eq!(
            expected_logs, actual_logs,
            "logs don't match at miniblock {miniblock_number}"
        )
    }

    #[tokio::test]
    async fn storage_logs_pruning_works_correctly() {
        let pool = ConnectionPool::<Core>::test_pool().await;

        let mut conn = pool.connection().await.unwrap();
        let mut transaction = conn.start_transaction().await.unwrap();
        insert_realistic_l1_batches(&mut transaction, 10).await;
        insert_miniblock_storage_logs(
            &mut transaction,
            L2BlockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut transaction,
            L2BlockNumber(0),
            // first storage will be overwritten in 1st miniblock,
            // the second one should be kept throughout the pruning
            // the third one will be overwritten in 10th miniblock
            vec![
                random_storage_log(1, 2),
                random_storage_log(2, 3),
                random_storage_log(3, 4),
            ],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut transaction,
            L2BlockNumber(15),
            // this storage log overrides log from block 0
            vec![random_storage_log(3, 5)],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut transaction,
            L2BlockNumber(17),
            // there are two logs with the same hashed key, the second one should be overwritten
            vec![random_storage_log(5, 5), random_storage_log(5, 7)],
        )
        .await;

        transaction
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(4), L2BlockNumber(9))
            .await
            .unwrap();

        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(0),
            vec![random_storage_log(2, 3), random_storage_log(3, 4)],
        )
        .await;
        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        transaction
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), L2BlockNumber(21))
            .await
            .unwrap();

        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(0),
            vec![random_storage_log(2, 3)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(15),
            vec![random_storage_log(3, 5)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut transaction,
            L2BlockNumber(17),
            vec![random_storage_log(5, 7)],
        )
        .await;
    }

    #[tokio::test]
    async fn l1_batches_can_be_hard_pruned() {
        let pool = ConnectionPool::<Core>::test_pool().await;

        let mut conn = pool.connection().await.unwrap();
        let mut transaction = conn.start_transaction().await.unwrap();
        insert_realistic_l1_batches(&mut transaction, 10).await;

        assert_l1_batch_objects_exists(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(10))
            .await;
        assert!(transaction
            .pruning_dal()
            .get_pruning_info()
            .await
            .unwrap()
            .last_hard_pruned_l1_batch
            .is_none());

        transaction
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(5), L2BlockNumber(11))
            .await
            .unwrap();

        assert_l1_batch_objects_dont_exist(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(5))
            .await;
        assert_l1_batch_objects_exists(&mut transaction, L1BatchNumber(6)..=L1BatchNumber(10))
            .await;
        assert_eq!(
            Some(L1BatchNumber(5)),
            transaction
                .pruning_dal()
                .get_pruning_info()
                .await
                .unwrap()
                .last_hard_pruned_l1_batch
        );

        transaction
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), L2BlockNumber(21))
            .await
            .unwrap();

        assert_l1_batch_objects_dont_exist(&mut transaction, L1BatchNumber(1)..=L1BatchNumber(10))
            .await;
        assert_eq!(
            Some(L1BatchNumber(10)),
            transaction
                .pruning_dal()
                .get_pruning_info()
                .await
                .unwrap()
                .last_hard_pruned_l1_batch
        );
    }
}
