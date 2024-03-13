use zksync_types::{L1BatchNumber, MiniblockNumber};

use crate::{instrument::InstrumentExt, StorageProcessor};

#[derive(Debug)]
pub struct PruningDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PruningInfo {
    pub last_soft_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_soft_pruned_miniblock: Option<MiniblockNumber>,
    pub last_hard_pruned_l1_batch: Option<L1BatchNumber>,
    pub last_hard_pruned_miniblock: Option<MiniblockNumber>,
}

impl PruningDal<'_, '_> {
    pub async fn get_pruning_info(&mut self) -> sqlx::Result<PruningInfo> {
        let row = sqlx::query!(
            r#"
            SELECT
                last_soft_pruned_l1_batch,
                last_soft_pruned_miniblock,
                last_hard_pruned_l1_batch,
                last_hard_pruned_miniblock
            FROM
                pruning_info
            "#
        )
        .instrument("get_last_soft_pruned_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(PruningInfo {
            last_soft_pruned_l1_batch: row
                .last_soft_pruned_l1_batch
                .map(|x| L1BatchNumber(x as u32)),
            last_soft_pruned_miniblock: row
                .last_soft_pruned_miniblock
                .map(|x| MiniblockNumber(x as u32)),
            last_hard_pruned_l1_batch: row
                .last_hard_pruned_l1_batch
                .map(|x| L1BatchNumber(x as u32)),
            last_hard_pruned_miniblock: row
                .last_hard_pruned_miniblock
                .map(|x| MiniblockNumber(x as u32)),
        })
    }

    pub async fn get_last_hard_pruned_batch(&mut self) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                last_hard_pruned_l1_batch
            FROM
                pruning_info
            "#
        )
        .instrument("get_last_hard_pruned_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(row
            .last_hard_pruned_l1_batch
            .map(|x| L1BatchNumber(x as u32)))
    }

    pub async fn soft_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_miniblock_to_prune: MiniblockNumber,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE pruning_info
            SET
                last_soft_pruned_l1_batch = $1,
                last_soft_pruned_miniblock = $2
            "#,
            last_l1_batch_to_prune.0 as i64,
            last_miniblock_to_prune.0 as i64
        )
        .instrument("soft_prune_batches_range")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn hard_prune_batches_range(
        &mut self,
        last_l1_batch_to_prune: L1BatchNumber,
        last_miniblock_to_prune: MiniblockNumber,
    ) -> sqlx::Result<()> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS first_miniblock_to_prune
            FROM
                miniblocks
            WHERE
                l1_batch_number <= $1
            "#,
            last_l1_batch_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#get_miniblocks_range")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        // this condition happens after snapshots recovery
        if row.first_miniblock_to_prune.is_none() {
            sqlx::query!(
                r#"
                UPDATE pruning_info
                SET
                    last_hard_pruned_l1_batch = $1,
                    last_hard_pruned_miniblock = $2
                "#,
                last_l1_batch_to_prune.0 as i64,
                last_miniblock_to_prune.0 as i64,
            )
            .instrument("hard_prune_batches_range#update_pruning_info")
            .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
            .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
            .report_latency()
            .execute(self.storage)
            .await?;

            return Ok(());
        }

        let first_miniblock_to_prune =
            MiniblockNumber(row.first_miniblock_to_prune.unwrap() as u32);

        sqlx::query!(
            r#"
            DELETE FROM events
            WHERE
                miniblock_number <= $1
            "#,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_events")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM l2_to_l1_logs
            WHERE
                miniblock_number <= $1
            "#,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_l2_to_l1_logs")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            UPDATE transactions
            SET
                l1_batch_number = NULL,
                miniblock_number = NULL
            WHERE
                miniblock_number >= $1
                AND miniblock_number <= $2
            "#,
            first_miniblock_to_prune.0 as i64,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#clear_transactions_references")
        .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        //The deleting of logs is split into two queries to make it faster,
        // only the first query has to go through all previous logs
        // and the query optimizer should be happy with it
        sqlx::query!(
            r#"
            DELETE FROM storage_logs USING (
                SELECT
                    *
                FROM
                    storage_logs
                WHERE
                    miniblock_number >= $1
                    AND miniblock_number <= $2
            ) AS batches_to_prune
            WHERE
                storage_logs.miniblock_number < $1
                AND batches_to_prune.hashed_key = storage_logs.hashed_key
            "#,
            first_miniblock_to_prune.0 as i64,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_overriden_storage_logs_from_past_batches")
        .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM storage_logs USING (
                SELECT
                    hashed_key,
                    MAX(ARRAY[miniblock_number, operation_number]::INT[]) AS op
                FROM
                    storage_logs
                WHERE
                    miniblock_number >= $1
                    AND miniblock_number <= $2
                GROUP BY
                    hashed_key
            ) AS last_storage_logs
            WHERE
                storage_logs.miniblock_number >= $1
                AND storage_logs.miniblock_number <= $2
                AND last_storage_logs.hashed_key = storage_logs.hashed_key
                AND (
                    storage_logs.miniblock_number != last_storage_logs.op[1]
                    OR storage_logs.operation_number != last_storage_logs.op[2]
                )
            "#,
            first_miniblock_to_prune.0 as i64,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_overriden_storage_logs_from_pruned_batches")
        .with_arg("first_miniblock_to_prune", &first_miniblock_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM l1_batches
            WHERE
                number <= $1
            "#,
            last_l1_batch_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_l1_batches")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM miniblocks
            WHERE
                number <= $1
            "#,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#delete_miniblocks")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        sqlx::query!(
            r#"
            UPDATE pruning_info
            SET
                last_hard_pruned_l1_batch = $1,
                last_hard_pruned_miniblock = $2
            "#,
            last_l1_batch_to_prune.0 as i64,
            last_miniblock_to_prune.0 as i64,
        )
        .instrument("hard_prune_batches_range#update_pruning_info")
        .with_arg("last_l1_batch_to_prune", &last_l1_batch_to_prune)
        .with_arg("last_miniblock_to_prune", &last_miniblock_to_prune)
        .report_latency()
        .execute(self.storage)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::ops;

    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::L1BatchHeader,
        l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
        tx::IncludedTxLocation,
        AccountTreeId, Address, ProtocolVersion, ProtocolVersionId, StorageKey, StorageLog, H256,
    };

    use super::*;
    use crate::{
        tests::{create_miniblock_header, mock_l2_to_l1_log, mock_vm_event},
        ConnectionPool,
    };

    async fn insert_miniblock(
        conn: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
        l1_batch_number: L1BatchNumber,
    ) {
        let miniblock1 = create_miniblock_header(miniblock_number.0);
        conn.blocks_dal()
            .insert_miniblock(&miniblock1)
            .await
            .unwrap();

        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(l1_batch_number)
            .await
            .unwrap();

        insert_events(conn, miniblock_number).await;
        insert_l2_to_l1_logs(conn, miniblock_number).await;
    }

    async fn insert_l2_to_l1_logs(
        conn: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
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
            .await;
    }

    async fn insert_events(conn: &mut StorageProcessor<'_>, miniblock_number: MiniblockNumber) {
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
            .await;
    }

    async fn insert_l1_batch(conn: &mut StorageProcessor<'_>, l1_batch_number: L1BatchNumber) {
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

    async fn insert_realistic_l1_batches(conn: &mut StorageProcessor<'_>, l1_batches_count: u32) {
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        for l1_batch_number in 0..l1_batches_count {
            insert_l1_batch(conn, L1BatchNumber(l1_batch_number)).await;
            insert_miniblock(
                conn,
                MiniblockNumber(l1_batch_number * 2),
                L1BatchNumber(l1_batch_number),
            )
            .await;
            insert_miniblock(
                conn,
                MiniblockNumber(l1_batch_number * 2 + 1),
                L1BatchNumber(l1_batch_number),
            )
            .await;
        }
    }

    async fn assert_l1_batch_objects_exists(
        conn: &mut StorageProcessor<'_>,
        l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
    ) {
        for l1_batch_number in l1_batches_range.start().0..l1_batches_range.end().0 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            assert!(conn
                .blocks_dal()
                .get_miniblock_header(MiniblockNumber(l1_batch_number.0 * 2))
                .await
                .unwrap()
                .is_some());

            assert!(conn
                .blocks_dal()
                .get_miniblock_header(MiniblockNumber(l1_batch_number.0 * 2 + 1))
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
        conn: &mut StorageProcessor<'_>,
        l1_batches_range: ops::RangeInclusive<L1BatchNumber>,
    ) {
        for l1_batch_number in l1_batches_range.start().0..l1_batches_range.end().0 {
            let l1_batch_number = L1BatchNumber(l1_batch_number);
            assert!(conn
                .blocks_dal()
                .get_miniblock_header(MiniblockNumber(l1_batch_number.0 * 2))
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                0,
                conn.storage_logs_dal()
                    .get_miniblock_storage_logs(MiniblockNumber(l1_batch_number.0 * 2))
                    .await
                    .len()
            );

            assert!(conn
                .blocks_dal()
                .get_miniblock_header(MiniblockNumber(l1_batch_number.0 * 2 + 1))
                .await
                .unwrap()
                .is_none());
            assert_eq!(
                0,
                conn.storage_logs_dal()
                    .get_miniblock_storage_logs(MiniblockNumber(l1_batch_number.0 * 2 + 1))
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
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: None,
                last_soft_pruned_l1_batch: None,
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        conn.pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(5), MiniblockNumber(11))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(MiniblockNumber(11)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(5)),
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        conn.pruning_dal()
            .soft_prune_batches_range(L1BatchNumber(10), MiniblockNumber(21))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(MiniblockNumber(21)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(10)),
                last_hard_pruned_miniblock: None,
                last_hard_pruned_l1_batch: None
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
        );

        conn.pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), MiniblockNumber(21))
            .await
            .unwrap();
        assert_eq!(
            PruningInfo {
                last_soft_pruned_miniblock: Some(MiniblockNumber(21)),
                last_soft_pruned_l1_batch: Some(L1BatchNumber(10)),
                last_hard_pruned_miniblock: Some(MiniblockNumber(21)),
                last_hard_pruned_l1_batch: Some(L1BatchNumber(10))
            },
            conn.pruning_dal().get_pruning_info().await.unwrap()
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
        conn: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
        storage_logs: Vec<StorageLog>,
    ) {
        conn.storage_logs_dal()
            .insert_storage_logs(miniblock_number, &[(H256::zero(), storage_logs)])
            .await
            .unwrap();
    }

    async fn assert_miniblock_storage_logs_equal(
        conn: &mut StorageProcessor<'_>,
        miniblock_number: MiniblockNumber,
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
        let pool = ConnectionPool::test_pool().await;

        let mut conn = pool.access_storage().await.unwrap();
        insert_realistic_l1_batches(&mut conn, 10).await;
        insert_miniblock_storage_logs(
            &mut conn,
            MiniblockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut conn,
            MiniblockNumber(0),
            // first storage will be overwritten in 1st miniblock,
            // the second one should be kept throught the pruning
            // the third one will be overwritten in 10th miniblock
            vec![
                random_storage_log(1, 2),
                random_storage_log(2, 3),
                random_storage_log(3, 4),
            ],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut conn,
            MiniblockNumber(15),
            // this storage log overrides log from 0rd block
            vec![random_storage_log(3, 5)],
        )
        .await;

        insert_miniblock_storage_logs(
            &mut conn,
            MiniblockNumber(17),
            // there are two logs with the same hashed key, the second one should be overwritten
            vec![random_storage_log(5, 5), random_storage_log(5, 7)],
        )
        .await;

        conn.pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(4), MiniblockNumber(9))
            .await
            .unwrap();

        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(0),
            vec![random_storage_log(2, 3), random_storage_log(3, 4)],
        )
        .await;
        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        conn.pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), MiniblockNumber(21))
            .await
            .unwrap();

        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(0),
            vec![random_storage_log(2, 3)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(1),
            vec![random_storage_log(1, 1)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(15),
            vec![random_storage_log(3, 5)],
        )
        .await;

        assert_miniblock_storage_logs_equal(
            &mut conn,
            MiniblockNumber(17),
            vec![random_storage_log(5, 7)],
        )
        .await;
    }

    #[tokio::test]
    async fn l1_batches_can_be_hard_pruned() {
        let pool = ConnectionPool::test_pool().await;

        let mut conn = pool.access_storage().await.unwrap();
        insert_realistic_l1_batches(&mut conn, 10).await;

        assert_l1_batch_objects_exists(&mut conn, L1BatchNumber(1)..=L1BatchNumber(10)).await;
        assert_eq!(
            None,
            conn.pruning_dal()
                .get_last_hard_pruned_batch()
                .await
                .unwrap()
        );

        conn.pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(5), MiniblockNumber(11))
            .await
            .unwrap();

        assert_l1_batch_objects_dont_exist(&mut conn, L1BatchNumber(1)..=L1BatchNumber(5)).await;
        assert_l1_batch_objects_exists(&mut conn, L1BatchNumber(6)..=L1BatchNumber(10)).await;
        assert_eq!(
            Some(L1BatchNumber(5)),
            conn.pruning_dal()
                .get_last_hard_pruned_batch()
                .await
                .unwrap()
        );

        conn.pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(10), MiniblockNumber(21))
            .await
            .unwrap();

        assert_l1_batch_objects_dont_exist(&mut conn, L1BatchNumber(1)..=L1BatchNumber(10)).await;
        assert_eq!(
            Some(L1BatchNumber(10)),
            conn.pruning_dal()
                .get_last_hard_pruned_batch()
                .await
                .unwrap()
        );
    }
}
