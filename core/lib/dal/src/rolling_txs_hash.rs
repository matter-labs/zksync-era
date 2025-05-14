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
    pub async fn get_ready_rolling_txs_hashes(
        &mut self,
        l1_batch_number: Option<L1BatchNumber>,
    ) -> DalResult<Vec<(L2BlockNumber, H256, bool)>> {
        let mut tx = self.storage.start_transaction().await?;

        let txs = sqlx::query!(
            r#"
            SELECT hash, error, miniblock_number AS "miniblock_number!" FROM transactions
            WHERE
                miniblock_number IN (
                    SELECT number FROM miniblocks
                    WHERE
                        rolling_txs_hash IS NOT NULL
                        AND
                        eth_tx_id IS NULL
                        AND
                        l1_batch_number = $1
                    ORDER BY number DESC

                )
            ORDER BY miniblock_number, index_in_block
            "#,
            l1_batch_number.map(|l1| i64::from(l1.0))
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

        Ok(txs)
    }
}
