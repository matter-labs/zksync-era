use std::convert::Into;

pub use crate::models::storage_block::{L1BatchMetadataError, L1BatchWithOptionalMetadata};
use crate::Core;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{L1BatchNumber, L2BlockNumber, H256};

#[derive(Debug)]
pub struct RollingTxsHashDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

pub struct TxForPrecommit {
    pub l2block_number: L2BlockNumber,
    pub timestamp: i64,
    pub tx_hash: H256,
    pub is_success: bool,
}

impl RollingTxsHashDal<'_, '_> {
    pub async fn get_ready_rolling_txs_hashes(
        &mut self,
        l1_batch_number: Option<L1BatchNumber>,
    ) -> DalResult<Vec<TxForPrecommit>> {
        dbg!(l1_batch_number);
        let mut tx = self.storage.start_transaction().await?;

        let txs = sqlx::query!(
            r#"
            SELECT
                transactions.hash, transactions.error,
                miniblock_number AS "miniblock_number!",
                miniblocks.timestamp
            FROM transactions
            JOIN miniblocks ON transactions.miniblock_number = miniblocks.number
            WHERE
                (
                    transactions.l1_batch_number IS NULL AND $2
                    OR transactions.l1_batch_number = $1
                )
                AND
                miniblocks.rolling_txs_hash IS NOT NULL
                AND
                miniblocks.eth_tx_id IS NULL
            ORDER BY miniblock_number, index_in_block
            "#,
            l1_batch_number.map(|l1| i64::from(l1.0)),
            l1_batch_number.is_none(),
        )
        .instrument("get_ready_rolling_txs_hashes")
        .report_latency()
        .fetch_all(&mut tx)
        .await?
        .into_iter()
        .map(|row| TxForPrecommit {
            l2block_number: L2BlockNumber(row.miniblock_number as u32),
            timestamp: row.timestamp,
            tx_hash: H256::from_slice(&row.hash),
            is_success: row.error.is_none(),
        })
        .collect();

        Ok(txs)
    }
}
