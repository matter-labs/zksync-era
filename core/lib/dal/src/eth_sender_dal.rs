use crate::models::storage_eth_tx::{
    L1BatchEthSenderStats, StorageEthTx, StorageTxHistory, StorageTxHistoryToSend,
};
use crate::StorageProcessor;
use anyhow::Context as _;
use sqlx::{
    types::chrono::{DateTime, Utc},
    Row,
};
use std::convert::TryFrom;
use std::str::FromStr;
use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::eth_sender::{EthTx, TxHistory, TxHistoryToSend};
use zksync_types::{Address, L1BatchNumber, H256, U256};

#[derive(Debug)]
pub struct EthSenderDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl EthSenderDal<'_, '_> {
    pub async fn get_inflight_txs(&mut self) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            "SELECT * FROM eth_txs WHERE confirmed_eth_tx_history_id IS NULL \
             AND id <= (SELECT COALESCE(MAX(eth_tx_id), 0) FROM eth_txs_history WHERE sent_at_block IS NOT NULL) \
             ORDER BY id"
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn get_eth_l1_batches(&mut self) -> sqlx::Result<L1BatchEthSenderStats> {
        let mut stats = L1BatchEthSenderStats::default();
        for tx_type in ["execute_tx", "commit_tx", "prove_tx"] {
            let mut records = sqlx::query(&format!(
                "SELECT number as number, true as confirmed FROM l1_batches \
                 INNER JOIN eth_txs_history ON (l1_batches.eth_{}_id = eth_txs_history.eth_tx_id) \
                 WHERE eth_txs_history.confirmed_at IS NOT NULL \
                 ORDER BY number DESC \
                 LIMIT 1",
                tx_type
            ))
            .fetch_all(self.storage.conn())
            .await?;

            records.extend(
                sqlx::query(&format!(
                    "SELECT number as number, false as confirmed FROM l1_batches \
                     INNER JOIN eth_txs_history ON (l1_batches.eth_{}_id = eth_txs_history.eth_tx_id) \
                     ORDER BY number DESC \
                     LIMIT 1",
                    tx_type
                ))
                .fetch_all(self.storage.conn())
                .await?,
            );

            for record in records {
                let batch_number = L1BatchNumber(record.get::<i64, &str>("number") as u32);
                let aggregation_action = match tx_type {
                    "execute_tx" => AggregatedActionType::Execute,
                    "commit_tx" => AggregatedActionType::Commit,
                    "prove_tx" => AggregatedActionType::PublishProofOnchain,
                    _ => unreachable!(),
                };
                if record.get::<bool, &str>("confirmed") {
                    stats.mined.push((aggregation_action, batch_number));
                } else {
                    stats.saved.push((aggregation_action, batch_number));
                }
            }
        }
        Ok(stats)
    }

    pub async fn get_eth_tx(&mut self, eth_tx_id: u32) -> sqlx::Result<Option<EthTx>> {
        Ok(sqlx::query_as!(
            StorageEthTx,
            "SELECT * FROM eth_txs WHERE id = $1",
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(Into::into))
    }

    pub async fn get_new_eth_txs(&mut self, limit: u64) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            "SELECT * FROM eth_txs \
            WHERE id > (SELECT COALESCE(MAX(eth_tx_id), 0) FROM eth_txs_history) \
            ORDER BY id \
            LIMIT $1",
            limit as i64
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn get_unsent_txs(&mut self) -> sqlx::Result<Vec<TxHistoryToSend>> {
        let txs = sqlx::query_as!(
            StorageTxHistoryToSend,
            "SELECT \
                eth_txs_history.id, \
                eth_txs_history.eth_tx_id, \
                eth_txs_history.tx_hash, \
                eth_txs_history.base_fee_per_gas, \
                eth_txs_history.priority_fee_per_gas, \
                eth_txs_history.signed_raw_tx, \
                eth_txs.nonce \
            FROM eth_txs_history \
            JOIN eth_txs ON eth_txs.id = eth_txs_history.eth_tx_id \
            WHERE eth_txs_history.sent_at_block IS NULL AND eth_txs.confirmed_eth_tx_history_id IS NULL \
            ORDER BY eth_txs_history.id DESC",
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn save_eth_tx(
        &mut self,
        nonce: u64,
        raw_tx: Vec<u8>,
        tx_type: AggregatedActionType,
        contract_address: Address,
        predicted_gas_cost: u32,
    ) -> sqlx::Result<EthTx> {
        let address = format!("{:#x}", contract_address);
        let eth_tx = sqlx::query_as!(
            StorageEthTx,
            "INSERT INTO eth_txs (raw_tx, nonce, tx_type, contract_address, predicted_gas_cost, created_at, updated_at) \
               VALUES ($1, $2, $3, $4, $5, now(), now()) \
               RETURNING *",
            raw_tx,
            nonce as i64,
            tx_type.to_string(),
            address,
            predicted_gas_cost as i64
        )
        .fetch_one(self.storage.conn())
        .await?;
        Ok(eth_tx.into())
    }

    pub async fn insert_tx_history(
        &mut self,
        eth_tx_id: u32,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        tx_hash: H256,
        raw_signed_tx: Vec<u8>,
    ) -> anyhow::Result<Option<u32>> {
        let priority_fee_per_gas =
            i64::try_from(priority_fee_per_gas).context("Can't convert u64 to i64")?;
        let base_fee_per_gas =
            i64::try_from(base_fee_per_gas).context("Can't convert u64 to i64")?;
        let tx_hash = format!("{:#x}", tx_hash);

        Ok(sqlx::query!(
            "INSERT INTO eth_txs_history \
            (eth_tx_id, base_fee_per_gas, priority_fee_per_gas, tx_hash, signed_raw_tx, created_at, updated_at) \
            VALUES ($1, $2, $3, $4, $5, now(), now()) \
            ON CONFLICT (tx_hash) DO NOTHING \
            RETURNING id",
            eth_tx_id as u32,
            base_fee_per_gas,
            priority_fee_per_gas,
            tx_hash,
            raw_signed_tx
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.id as u32))
    }

    pub async fn set_sent_at_block(
        &mut self,
        eth_txs_history_id: u32,
        sent_at_block: u32,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE eth_txs_history SET sent_at_block = $2, sent_at = now() \
            WHERE id = $1 AND sent_at_block IS NULL",
            eth_txs_history_id as i32,
            sent_at_block as i32
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn remove_tx_history(&mut self, eth_txs_history_id: u32) -> sqlx::Result<()> {
        sqlx::query!(
            "DELETE FROM eth_txs_history \
            WHERE id = $1",
            eth_txs_history_id as i64
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn confirm_tx(&mut self, tx_hash: H256, gas_used: U256) -> anyhow::Result<()> {
        let mut transaction = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction()")?;
        let gas_used = i64::try_from(gas_used)
            .map_err(|err| anyhow::anyhow!("Can't convert U256 to i64: {err}"))?;
        let tx_hash = format!("{:#x}", tx_hash);
        let ids = sqlx::query!(
            "UPDATE eth_txs_history \
            SET updated_at = now(), confirmed_at = now() \
            WHERE tx_hash = $1 \
            RETURNING id, eth_tx_id",
            tx_hash,
        )
        .fetch_one(transaction.conn())
        .await?;

        sqlx::query!(
            "UPDATE eth_txs \
            SET gas_used = $1, confirmed_eth_tx_history_id = $2 \
            WHERE id = $3",
            gas_used,
            ids.id,
            ids.eth_tx_id
        )
        .execute(transaction.conn())
        .await?;

        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_confirmed_tx_hash_by_eth_tx_id(
        &mut self,
        eth_tx_id: u32,
    ) -> anyhow::Result<Option<H256>> {
        let tx_hash = sqlx::query!(
            "SELECT tx_hash FROM eth_txs_history \
            WHERE eth_tx_id = $1 AND confirmed_at IS NOT NULL",
            eth_tx_id as i64
        )
        .fetch_optional(self.storage.conn())
        .await?;

        let Some(tx_hash) = tx_hash else {
            return Ok(None);
        };
        let tx_hash = tx_hash.tx_hash;
        let tx_hash = tx_hash.trim_start_matches("0x");
        Ok(Some(H256::from_str(tx_hash).context("invalid tx_hash")?))
    }

    /// This method inserts a fake transaction into the database that would make the corresponding L1 batch
    /// to be considered committed/proven/executed.
    ///
    /// The designed use case is the External Node usage, where we don't really care about the actual transactions apart
    /// from the hash and the fact that tx was sent.
    ///
    /// ## Warning
    ///
    /// After this method is used anywhere in the codebase, it is considered a bug to try to directly query `eth_txs_history`
    /// or `eth_txs` tables.
    pub async fn insert_bogus_confirmed_eth_tx(
        &mut self,
        l1_batch: L1BatchNumber,
        tx_type: AggregatedActionType,
        tx_hash: H256,
        confirmed_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        let mut transaction = self
            .storage
            .start_transaction()
            .await
            .context("start_transaction")?;
        let tx_hash = format!("{:#x}", tx_hash);

        let eth_tx_id = sqlx::query_scalar!(
            "SELECT eth_txs.id FROM eth_txs_history JOIN eth_txs \
            ON eth_txs.confirmed_eth_tx_history_id = eth_txs_history.id \
            WHERE eth_txs_history.tx_hash = $1",
            tx_hash
        )
        .fetch_optional(transaction.conn())
        .await?;

        // Check if the transaction with the corresponding hash already exists.
        let eth_tx_id = if let Some(eth_tx_id) = eth_tx_id {
            eth_tx_id
        } else {
            // No such transaction in the database yet, we have to insert it.

            // Insert general tx descriptor.
            let eth_tx_id = sqlx::query_scalar!(
                "INSERT INTO eth_txs (raw_tx, nonce, tx_type, contract_address, predicted_gas_cost, created_at, updated_at) \
                VALUES ('\\x00', 0, $1, '', 0, now(), now()) \
                RETURNING id",
                tx_type.to_string()
            )
            .fetch_one(transaction.conn())
            .await?;

            // Insert a "sent transaction".
            let eth_history_id = sqlx::query_scalar!(
                "INSERT INTO eth_txs_history \
                (eth_tx_id, base_fee_per_gas, priority_fee_per_gas, tx_hash, signed_raw_tx, created_at, updated_at, confirmed_at) \
                VALUES ($1, 0, 0, $2, '\\x00', now(), now(), $3) \
                RETURNING id",
                eth_tx_id,
                tx_hash,
                confirmed_at.naive_utc()
            )
            .fetch_one(transaction.conn())
            .await?;

            // Mark general entry as confirmed.
            sqlx::query!(
                "UPDATE eth_txs \
                SET confirmed_eth_tx_history_id = $1 \
                WHERE id = $2",
                eth_history_id,
                eth_tx_id
            )
            .execute(transaction.conn())
            .await?;

            eth_tx_id
        };

        // Tie the ETH tx to the L1 batch.
        super::BlocksDal {
            storage: &mut transaction,
        }
        .set_eth_tx_id(l1_batch..=l1_batch, eth_tx_id as u32, tx_type)
        .await
        .context("set_eth_tx_id()")?;

        transaction.commit().await.context("commit()")
    }

    pub async fn get_tx_history_to_check(
        &mut self,
        eth_tx_id: u32,
    ) -> sqlx::Result<Vec<TxHistory>> {
        let tx_history = sqlx::query_as!(
            StorageTxHistory,
            "SELECT * FROM eth_txs_history WHERE eth_tx_id = $1 ORDER BY created_at DESC",
            eth_tx_id as i32
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(tx_history.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn get_block_number_on_first_sent_attempt(
        &mut self,
        eth_tx_id: u32,
    ) -> sqlx::Result<Option<u32>> {
        let sent_at_block = sqlx::query_scalar!(
            "SELECT sent_at_block FROM eth_txs_history WHERE eth_tx_id = $1 AND sent_at_block IS NOT NULL ORDER BY created_at ASC LIMIT 1",
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(sent_at_block.flatten().map(|block| block as u32))
    }

    pub async fn get_last_sent_eth_tx(
        &mut self,
        eth_tx_id: u32,
    ) -> sqlx::Result<Option<TxHistory>> {
        let history_item = sqlx::query_as!(
            StorageTxHistory,
            "SELECT * FROM eth_txs_history WHERE eth_tx_id = $1 ORDER BY created_at DESC LIMIT 1",
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(history_item.map(|tx| tx.into()))
    }

    pub async fn get_next_nonce(&mut self) -> sqlx::Result<Option<u64>> {
        let row = sqlx::query!("SELECT nonce FROM eth_txs ORDER BY id DESC LIMIT 1")
            .fetch_optional(self.storage.conn())
            .await?;
        Ok(row.map(|row| row.nonce as u64 + 1))
    }

    pub async fn mark_failed_transaction(&mut self, eth_tx_id: u32) -> sqlx::Result<()> {
        sqlx::query!(
            "UPDATE eth_txs SET has_failed = TRUE WHERE id = $1",
            eth_tx_id as i32
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_number_of_failed_transactions(&mut self) -> anyhow::Result<i64> {
        sqlx::query!("SELECT COUNT(*) FROM eth_txs WHERE has_failed = TRUE")
            .fetch_one(self.storage.conn())
            .await?
            .count
            .context("count field is missing")
    }

    pub async fn clear_failed_transactions(&mut self) -> sqlx::Result<()> {
        sqlx::query!(
            "DELETE FROM eth_txs WHERE id >= \
            (SELECT MIN(id) FROM eth_txs WHERE has_failed = TRUE)"
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }
}
