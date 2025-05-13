use std::convert::Into;

use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BatchNumber, L2BlockNumber, H256};

pub use crate::models::storage_block::{L1BatchMetadataError, L1BatchWithOptionalMetadata};
use crate::Core;

#[derive(Debug)]
pub struct RollingTxsHashDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl RollingTxsHashDal<'_, '_> {
    pub async fn seal_rolling_txs_hash(
        &mut self,
        l1_batch_number: L1BatchNumber,
        from_l2_block_number: L2BlockNumber,
        to_l2_block_number: L2BlockNumber,
        rolling_hash: H256,
        final_hash: bool,
    ) -> DalResult<()> {
        let mut tx = self.storage.start_transaction().await?;
        let rolling_tx_hash_id = sqlx::query!(
            r#"
            INSERT INTO
            rolling_tx_hashes
            (l1_batch_number, rolling_hash, final)
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
            i64::from(l1_batch_number.0),
            rolling_hash.as_bytes(),
            final_hash,
        )
        .instrument("insert_rolling_txs_hash")
        .report_latency()
        .fetch_one(&mut tx)
        .await?
        .id;
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                rolling_txs_id = $1,
                updated_at = NOW()
            WHERE
                number >= $2 AND number <= $3
            "#,
            rolling_tx_hash_id,
            i64::from(from_l2_block_number.0),
            i64::from(to_l2_block_number.0),
        )
        .instrument("set_rolling_txs_hash")
        .report_latency()
        .execute(&mut tx)
        .await?;
        Ok(())
    }

    pub async fn get_last_l2_block_with_rolling_txs_hash(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<L2BlockNumber>> {
        let mut tx = self.storage.start_transaction().await?;

        let last_miniblock = sqlx::query!(
            r#"
            SELECT number FROM miniblocks
            WHERE
                rolling_txs_id = (
                    SELECT MAX(id) FROM rolling_tx_hashes
                    WHERE l1_batch_number = $1
                )
            ORDER BY number DESC
            LIMIT 1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("get_last_miniblock_for_rolling_txs_hash")
        .report_latency()
        .fetch_optional(&mut tx)
        .await?
        .map(|row| L2BlockNumber(row.number as u32));

        Ok(last_miniblock)
    }

    pub async fn get_ready_rolling_txs_hashes(
        &mut self,
        last_sent_l1_batch: L1BatchNumber,
    ) -> DalResult<(Vec<(L2BlockNumber, H256, bool)>, L1BatchNumber)> {
        let mut tx = self.storage.start_transaction().await?;

        let l1_batch_number = last_sent_l1_batch + 1;

        let rolling_tx_hash_id = sqlx::query!(
            r#"
            SELECT id FROM rolling_tx_hashes
            WHERE
                l1_batch_number = $1
                AND
                eth_tx_id IS NULL
            ORDER BY id
            LIMIT 1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("get_ready_rolling_txs_hashes")
        .report_latency()
        .fetch_optional(&mut tx)
        .await?;
        let rolling_tx_hash_id = match rolling_tx_hash_id {
            Some(row) => row.id,
            None => return Ok((Vec::new(), l1_batch_number)),
        };

        let txs = sqlx::query!(
            r#"
            SELECT hash, error, miniblock_number AS "miniblock_number!" FROM transactions
            WHERE
                miniblock_number IN (
                    SELECT number
                    FROM miniblocks
                    WHERE
                        rolling_txs_id = $1
                )
            ORDER BY miniblock_number, index_in_block
            "#,
            rolling_tx_hash_id
        )
        .instrument("get_ready_rolling_txs_hashes")
        .report_latency()
        .fetch_all(&mut tx)
        .await?
        .into_iter()
        .map(|row| {
            (
                L2BlockNumber(row.miniblock_number as u32),
                H256::from_slice(&row.hash),
                row.error.is_some(),
            )
        })
        .collect();

        Ok((txs, l1_batch_number))
    }
}
