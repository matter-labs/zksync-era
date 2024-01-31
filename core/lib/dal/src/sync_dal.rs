use zksync_types::{api::en, Address, MiniblockNumber};

use crate::{
    instrument::InstrumentExt,
    metrics::MethodLatency,
    models::storage_sync::{StorageSyncBlock, SyncBlock},
    StorageProcessor,
};

/// DAL subset dedicated to the EN synchronization.
#[derive(Debug)]
pub struct SyncDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl SyncDal<'_, '_> {
    pub(super) async fn sync_block_inner(
        &mut self,
        block_number: MiniblockNumber,
    ) -> anyhow::Result<Option<SyncBlock>> {
        let Some(block) = sqlx::query_as!(
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
                    )
                ) AS "l1_batch_number!",
                (
                    SELECT
                        MAX(m2.number)
                    FROM
                        miniblocks m2
                    WHERE
                        miniblocks.l1_batch_number = m2.l1_batch_number
                ) AS "last_batch_miniblock?",
                miniblocks.timestamp,
                miniblocks.l1_gas_price,
                miniblocks.l2_fair_gas_price,
                miniblocks.fair_pubdata_price,
                miniblocks.bootloader_code_hash,
                miniblocks.default_aa_code_hash,
                miniblocks.virtual_blocks,
                miniblocks.hash,
                miniblocks.protocol_version AS "protocol_version!",
                l1_batches.fee_account_address AS "fee_account_address?"
            FROM
                miniblocks
                LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
            WHERE
                miniblocks.number = $1
            "#,
            block_number.0 as i64
        )
        .instrument("sync_dal_sync_block.block")
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(block.try_into()?))
    }

    pub async fn sync_block(
        &mut self,
        block_number: MiniblockNumber,
        current_operator_address: Address,
        include_transactions: bool,
    ) -> anyhow::Result<Option<en::SyncBlock>> {
        let _latency = MethodLatency::new("sync_dal_sync_block");
        let Some(block) = self.sync_block_inner(block_number).await? else {
            return Ok(None);
        };
        let transactions = if include_transactions {
            Some(
                self.storage
                    .transactions_web3_dal()
                    .get_raw_miniblock_transactions(block_number)
                    .await?,
            )
        } else {
            None
        };
        Ok(Some(block.into_api(current_operator_address, transactions)))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::L1BatchHeader, fee::TransactionExecutionMetrics, L1BatchNumber, ProtocolVersion,
        ProtocolVersionId, Transaction,
    };

    use super::*;
    use crate::{
        tests::{create_miniblock_header, mock_execution_result, mock_l2_transaction},
        ConnectionPool,
    };

    #[tokio::test]
    async fn sync_block_basics() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();

        // Simulate genesis.
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();
        let mut l1_batch_header = L1BatchHeader::new(
            L1BatchNumber(0),
            0,
            Address::repeat_byte(0x42),
            Default::default(),
            ProtocolVersionId::latest(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();

        let operator_address = Address::repeat_byte(1);
        assert!(conn
            .sync_dal()
            .sync_block(MiniblockNumber(1), operator_address, false)
            .await
            .unwrap()
            .is_none());

        // Insert another block in the store.
        let miniblock_header = create_miniblock_header(1);
        let tx = mock_l2_transaction();
        conn.transactions_dal()
            .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&miniblock_header)
            .await
            .unwrap();
        conn.transactions_dal()
            .mark_txs_as_executed_in_miniblock(
                MiniblockNumber(1),
                &[mock_execution_result(tx.clone())],
                1.into(),
            )
            .await;

        let block = conn
            .sync_dal()
            .sync_block(MiniblockNumber(1), operator_address, false)
            .await
            .unwrap()
            .expect("no sync block");
        assert_eq!(block.number, MiniblockNumber(1));
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
        assert_eq!(block.operator_address, operator_address);
        assert!(block.transactions.is_none());

        let block = conn
            .sync_dal()
            .sync_block(MiniblockNumber(1), operator_address, true)
            .await
            .unwrap()
            .expect("no sync block");
        let transactions = block.transactions.unwrap();
        assert_eq!(transactions, [Transaction::from(tx)]);

        l1_batch_header.number = L1BatchNumber(1);
        l1_batch_header.timestamp = 1;
        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();

        let block = conn
            .sync_dal()
            .sync_block(MiniblockNumber(1), operator_address, true)
            .await
            .unwrap()
            .expect("no sync block");
        assert_eq!(block.l1_batch_number, L1BatchNumber(1));
        assert!(block.last_in_batch);
        assert_eq!(block.operator_address, l1_batch_header.fee_account_address);
    }
}
