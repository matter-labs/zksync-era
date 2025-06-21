use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::Instrumented, metrics::MethodLatency,
};
use zksync_types::{api::en, L2BlockNumber};

use crate::{
    models::storage_sync::{StorageSyncBlock, SyncBlock},
    Core, CoreDal,
};

/// DAL subset dedicated to the EN synchronization.
#[derive(Debug)]
pub struct SyncDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl SyncDal<'_, '_> {
    pub(super) async fn sync_blocks_inner(
        &mut self,
        numbers: std::ops::Range<L2BlockNumber>,
    ) -> DalResult<Vec<SyncBlock>> {
        // Check if range is non-empty, because BETWEEN in SQL in `unordered`.
        if numbers.is_empty() {
            return Ok(vec![]);
        }
        let query = sqlx::query_as!(
            StorageSyncBlock,
            r#"
            SELECT
                miniblocks.number,
                COALESCE(
                    miniblocks.l1_batch_number,
                    (
                        SELECT
                            (MAX(number) + 1)
                        FROM
                            l1_batches
                        WHERE
                            is_sealed
                    ),
                    (
                        SELECT
                            MAX(l1_batch_number) + 1
                        FROM
                            snapshot_recovery
                    )
                ) AS "l1_batch_number!",
                (miniblocks.l1_tx_count + miniblocks.l2_tx_count) AS "tx_count!",
                miniblocks.timestamp,
                miniblocks.l1_gas_price,
                miniblocks.l2_fair_gas_price,
                miniblocks.fair_pubdata_price,
                miniblocks.bootloader_code_hash,
                miniblocks.default_aa_code_hash,
                miniblocks.evm_emulator_code_hash,
                miniblocks.virtual_blocks,
                miniblocks.hash,
                miniblocks.protocol_version AS "protocol_version!",
                miniblocks.fee_account_address AS "fee_account_address!",
                miniblocks.l2_da_validator_address AS "l2_da_validator_address!",
                miniblocks.pubdata_type AS "pubdata_type!"
            FROM
                miniblocks
            WHERE
                miniblocks.number BETWEEN $1 AND $2
            "#,
            i64::from(numbers.start.0),
            i64::from(numbers.end.0 - 1),
        );
        let instrumentation =
            Instrumented::new("sync_dal_sync_blocks").with_arg("numbers", &numbers);
        let blocks = instrumentation
            .clone()
            .with(query)
            .fetch_all(self.storage)
            .await?;

        let mut sync_blocks = vec![];
        for block in &blocks {
            // Convert the block to the SyncBlock type.
            let interop_roots = self
                .storage
                .interop_root_dal()
                .get_interop_roots(L2BlockNumber(block.number as u32))
                .await?;
            sync_blocks.push(
                SyncBlock::new(block.clone(), interop_roots)
                    .map_err(|err| instrumentation.constraint_error(err.into()))?,
            );
        }

        Ok(sync_blocks)
    }

    pub async fn sync_block(
        &mut self,
        number: L2BlockNumber,
        include_transactions: bool,
    ) -> DalResult<Option<en::SyncBlock>> {
        let _latency = MethodLatency::new("sync_dal_sync_block");
        let numbers = number..number + 1;
        let Some(block) = self
            .sync_blocks_inner(numbers.clone())
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let transactions = if include_transactions {
            let mut transactions = self
                .storage
                .transactions_web3_dal()
                .get_raw_l2_blocks_transactions(numbers)
                .await?;
            // If there are no transactions in the block,
            // return `Some(vec![])`.
            Some(transactions.remove(&number).unwrap_or_default())
        } else {
            None
        };
        Ok(Some(block.into_api(transactions)))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::{L1BatchHeader, L2BlockHeader},
        Address, L1BatchNumber, ProtocolVersion, ProtocolVersionId, Transaction,
    };
    use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics};

    use super::*;
    use crate::{
        tests::{
            create_l2_block_header, create_snapshot_recovery, mock_execution_result,
            mock_l2_transaction,
        },
        ConnectionPool, Core,
    };

    #[tokio::test]
    async fn sync_block_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        // Simulate genesis.
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();
        let mut l1_batch_header = L1BatchHeader::new(
            L1BatchNumber(0),
            0,
            Default::default(),
            ProtocolVersionId::latest(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();

        assert!(conn
            .sync_dal()
            .sync_block(L2BlockNumber(1), false)
            .await
            .unwrap()
            .is_none());

        // Insert another block in the store.
        let miniblock_header = L2BlockHeader {
            fee_account_address: Address::repeat_byte(0x42),
            l2_tx_count: 1,
            ..create_l2_block_header(1)
        };
        let tx = mock_l2_transaction();
        conn.transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&miniblock_header)
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

        let block = conn
            .sync_dal()
            .sync_block(L2BlockNumber(1), false)
            .await
            .unwrap()
            .expect("no sync block");
        assert_eq!(block.number, L2BlockNumber(1));
        assert_eq!(block.l1_batch_number, L1BatchNumber(1));
        assert!(!block.last_in_batch);
        assert_eq!(block.timestamp, miniblock_header.timestamp);
        assert_eq!(
            block.protocol_version,
            miniblock_header.protocol_version.unwrap()
        );
        assert_eq!(
            block.virtual_blocks.unwrap(),
            miniblock_header.virtual_blocks
        );
        assert_eq!(
            block.l1_gas_price,
            miniblock_header.batch_fee_input.l1_gas_price()
        );
        assert_eq!(
            block.l2_fair_gas_price,
            miniblock_header.batch_fee_input.fair_l2_gas_price()
        );
        assert_eq!(block.operator_address, miniblock_header.fee_account_address);
        assert!(block.transactions.is_none());

        let block = conn
            .sync_dal()
            .sync_block(L2BlockNumber(1), true)
            .await
            .unwrap()
            .expect("no sync block");
        let transactions = block.transactions.unwrap();
        assert_eq!(transactions, [Transaction::from(tx)]);

        let miniblock_header = L2BlockHeader {
            fee_account_address: Address::repeat_byte(0x42),
            l2_tx_count: 0,
            ..create_l2_block_header(2)
        };
        conn.blocks_dal()
            .insert_l2_block(&miniblock_header)
            .await
            .unwrap();

        l1_batch_header.number = L1BatchNumber(1);
        l1_batch_header.timestamp = 1;
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();

        let block = conn
            .sync_dal()
            .sync_block(L2BlockNumber(2), true)
            .await
            .unwrap()
            .expect("no sync block");
        assert_eq!(block.l1_batch_number, L1BatchNumber(1));
        assert!(block.last_in_batch);
        assert_eq!(block.operator_address, miniblock_header.fee_account_address);
    }

    #[tokio::test]
    async fn sync_block_after_snapshot_recovery() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        // Simulate snapshot recovery.
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let snapshot_recovery = create_snapshot_recovery();
        conn.snapshot_recovery_dal()
            .insert_initial_recovery_status(&snapshot_recovery)
            .await
            .unwrap();

        assert!(conn
            .sync_dal()
            .sync_block(snapshot_recovery.l2_block_number, false)
            .await
            .unwrap()
            .is_none());

        let miniblock_header = create_l2_block_header(snapshot_recovery.l2_block_number.0 + 1);
        conn.blocks_dal()
            .insert_l2_block(&miniblock_header)
            .await
            .unwrap();

        let block = conn
            .sync_dal()
            .sync_block(miniblock_header.number, false)
            .await
            .unwrap()
            .expect("No new miniblock");
        assert_eq!(block.number, miniblock_header.number);
        assert_eq!(block.timestamp, miniblock_header.timestamp);
        assert_eq!(block.l1_batch_number, snapshot_recovery.l1_batch_number + 1);
    }
}
