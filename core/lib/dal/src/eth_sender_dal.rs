use std::{convert::TryFrom, str::FromStr};

use anyhow::Context as _;
use sqlx::types::chrono::{DateTime, Utc};
use zksync_db_connection::{connection::Connection, interpolate_query, match_query_as};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::{EthTx, EthTxBlobSidecar, TxHistory, TxHistoryToSend},
    Address, L1BatchNumber, H256, U256,
};

use crate::{
    models::storage_eth_tx::{
        L1BatchEthSenderStats, StorageEthTx, StorageTxHistory, StorageTxHistoryToSend,
    },
    Core,
};

#[derive(Debug)]
pub struct EthSenderDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl EthSenderDal<'_, '_> {
    pub async fn get_inflight_txs(
        &mut self,
        operator_address: Option<Address>,
    ) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            r#"
            SELECT
                *
            FROM
                eth_txs
            WHERE
                from_addr IS NOT DISTINCT FROM $1 -- can't just use equality as NULL != NULL
                AND confirmed_eth_tx_history_id IS NULL
                AND id <= (
                    SELECT
                        COALESCE(MAX(eth_tx_id), 0)
                    FROM
                        eth_txs_history
                    WHERE
                        sent_at_block IS NOT NULL
                )
            ORDER BY
                id
            "#,
            operator_address.as_ref().map(|h160| h160.as_bytes()),
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn get_eth_l1_batches(&mut self) -> sqlx::Result<L1BatchEthSenderStats> {
        struct EthTxRow {
            number: i64,
            confirmed: bool,
        }

        const TX_TYPES: &[AggregatedActionType] = &[
            AggregatedActionType::Commit,
            AggregatedActionType::PublishProofOnchain,
            AggregatedActionType::Execute,
        ];

        let mut stats = L1BatchEthSenderStats::default();
        for &tx_type in TX_TYPES {
            let mut tx_rows = vec![];
            for confirmed in [true, false] {
                let query = match_query_as!(
                    EthTxRow,
                    [
                        "SELECT number AS number, ", _, " AS \"confirmed!\" FROM l1_batches ",
                        "INNER JOIN eth_txs_history ON l1_batches.", _, " = eth_txs_history.eth_tx_id ",
                        _, // WHERE clause
                        " ORDER BY number DESC LIMIT 1"
                    ],
                    match ((confirmed, tx_type)) {
                        (false, AggregatedActionType::Commit) => ("false", "eth_commit_tx_id", "";),
                        (true, AggregatedActionType::Commit) => (
                            "true", "eth_commit_tx_id", "WHERE eth_txs_history.confirmed_at IS NOT NULL";
                        ),
                        (false, AggregatedActionType::PublishProofOnchain) => ("false", "eth_prove_tx_id", "";),
                        (true, AggregatedActionType::PublishProofOnchain) => (
                            "true", "eth_prove_tx_id", "WHERE eth_txs_history.confirmed_at IS NOT NULL";
                        ),
                        (false, AggregatedActionType::Execute) => ("false", "eth_execute_tx_id", "";),
                        (true, AggregatedActionType::Execute) => (
                            "true", "eth_execute_tx_id", "WHERE eth_txs_history.confirmed_at IS NOT NULL";
                        ),
                    }
                );
                tx_rows.extend(query.fetch_all(self.storage.conn()).await?);
            }

            for row in tx_rows {
                let batch_number = L1BatchNumber(row.number as u32);
                if row.confirmed {
                    stats.mined.push((tx_type, batch_number));
                } else {
                    stats.saved.push((tx_type, batch_number));
                }
            }
        }
        Ok(stats)
    }

    pub async fn get_eth_tx(&mut self, eth_tx_id: u32) -> sqlx::Result<Option<EthTx>> {
        Ok(sqlx::query_as!(
            StorageEthTx,
            r#"
            SELECT
                *
            FROM
                eth_txs
            WHERE
                id = $1
            "#,
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(Into::into))
    }

    pub async fn get_new_eth_txs(
        &mut self,
        limit: u64,
        operator_address: &Option<Address>,
    ) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            r#"
            SELECT
                *
            FROM
                eth_txs
            WHERE
                from_addr IS NOT DISTINCT FROM $2 -- can't just use equality as NULL != NULL
                AND id > (
                    SELECT
                        COALESCE(MAX(eth_tx_id), 0)
                    FROM
                        eth_txs_history
                )
            ORDER BY
                id
            LIMIT
                $1
            "#,
            limit as i64,
            operator_address.as_ref().map(|h160| h160.as_bytes()),
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    /*



           SELECT
               id
           FROM
               eth_txs
           WHERE
               from_addr IS NOT DISTINCT FROM '\x0bbc816539a5d4bffb8cb720099512abcf26017f' -- can't just use equality as NULL != NULL
               AND id > (
                   SELECT
                       COALESCE(MAX(eth_tx_id), 0)
                   FROM
                       eth_txs_history
               )
           ORDER BY
               id
           LIMIT
               30


    */
    pub async fn get_unsent_txs(&mut self) -> sqlx::Result<Vec<TxHistoryToSend>> {
        let txs = sqlx::query_as!(
            StorageTxHistoryToSend,
            r#"
            SELECT
                eth_txs_history.id,
                eth_txs_history.eth_tx_id,
                eth_txs_history.tx_hash,
                eth_txs_history.base_fee_per_gas,
                eth_txs_history.priority_fee_per_gas,
                eth_txs_history.signed_raw_tx,
                eth_txs.nonce
            FROM
                eth_txs_history
                JOIN eth_txs ON eth_txs.id = eth_txs_history.eth_tx_id
            WHERE
                eth_txs_history.sent_at_block IS NULL
                AND eth_txs.confirmed_eth_tx_history_id IS NULL
            ORDER BY
                eth_txs_history.id DESC
            "#,
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_eth_tx(
        &mut self,
        nonce: u64,
        raw_tx: Vec<u8>,
        tx_type: AggregatedActionType,
        contract_address: Address,
        predicted_gas_cost: u32,
        from_address: Option<Address>,
        blob_sidecar: Option<EthTxBlobSidecar>,
    ) -> sqlx::Result<EthTx> {
        let address = format!("{:#x}", contract_address);
        let eth_tx = sqlx::query_as!(
            StorageEthTx,
            r#"
            INSERT INTO
                eth_txs (
                    raw_tx,
                    nonce,
                    tx_type,
                    contract_address,
                    predicted_gas_cost,
                    created_at,
                    updated_at,
                    from_addr,
                    blob_sidecar
                )
            VALUES
                ($1, $2, $3, $4, $5, NOW(), NOW(), $6, $7)
            RETURNING
                *
            "#,
            raw_tx,
            nonce as i64,
            tx_type.to_string(),
            address,
            i64::from(predicted_gas_cost),
            from_address.as_ref().map(Address::as_bytes),
            blob_sidecar.map(|sidecar| bincode::serialize(&sidecar)
                .expect("can always bincode serialize EthTxBlobSidecar; qed")),
        )
        .fetch_one(self.storage.conn())
        .await?;
        Ok(eth_tx.into())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_tx_history(
        &mut self,
        eth_tx_id: u32,
        base_fee_per_gas: u64,
        priority_fee_per_gas: u64,
        blob_base_fee_per_gas: Option<u64>,
        tx_hash: H256,
        raw_signed_tx: &[u8],
        sent_at_block: u32,
    ) -> anyhow::Result<Option<u32>> {
        let priority_fee_per_gas =
            i64::try_from(priority_fee_per_gas).context("Can't convert u64 to i64")?;
        let base_fee_per_gas =
            i64::try_from(base_fee_per_gas).context("Can't convert u64 to i64")?;
        let tx_hash = format!("{:#x}", tx_hash);

        Ok(sqlx::query!(
            r#"
            INSERT INTO
                eth_txs_history (
                    eth_tx_id,
                    base_fee_per_gas,
                    priority_fee_per_gas,
                    tx_hash,
                    signed_raw_tx,
                    created_at,
                    updated_at,
                    blob_base_fee_per_gas,
                    sent_at_block,
                    sent_at
                )
            VALUES
                ($1, $2, $3, $4, $5, NOW(), NOW(), $6, $7, NOW())
            ON CONFLICT (tx_hash) DO NOTHING
            RETURNING
                id
            "#,
            eth_tx_id as i32,
            base_fee_per_gas,
            priority_fee_per_gas,
            tx_hash,
            raw_signed_tx,
            blob_base_fee_per_gas.map(|v| v as i64),
            sent_at_block as i32
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
            r#"
            UPDATE eth_txs_history
            SET
                sent_at_block = $2,
                sent_at = NOW()
            WHERE
                id = $1
                AND sent_at_block IS NULL
            "#,
            eth_txs_history_id as i32,
            sent_at_block as i32
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn remove_tx_history(&mut self, eth_txs_history_id: u32) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM eth_txs_history
            WHERE
                id = $1
            "#,
            eth_txs_history_id as i32
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
            r#"
            UPDATE eth_txs_history
            SET
                updated_at = NOW(),
                confirmed_at = NOW()
            WHERE
                tx_hash = $1
            RETURNING
                id,
                eth_tx_id
            "#,
            tx_hash,
        )
        .fetch_one(transaction.conn())
        .await?;

        sqlx::query!(
            r#"
            UPDATE eth_txs
            SET
                gas_used = $1,
                confirmed_eth_tx_history_id = $2
            WHERE
                id = $3
            "#,
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
            r#"
            SELECT
                tx_hash
            FROM
                eth_txs_history
            WHERE
                eth_tx_id = $1
                AND confirmed_at IS NOT NULL
            "#,
            eth_tx_id as i32
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
                r#"
                UPDATE eth_txs
                SET
                    confirmed_eth_tx_history_id = $1
                WHERE
                    id = $2
                "#,
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
            r#"
            SELECT
                eth_txs_history.*,
                eth_txs.blob_sidecar
            FROM
                eth_txs_history
                LEFT JOIN eth_txs ON eth_tx_id = eth_txs.id
            WHERE
                eth_tx_id = $1
            ORDER BY
                eth_txs_history.created_at DESC
            "#,
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
            r#"
            SELECT
                eth_txs_history.*,
                eth_txs.blob_sidecar
            FROM
                eth_txs_history
                LEFT JOIN eth_txs ON eth_tx_id = eth_txs.id
            WHERE
                eth_tx_id = $1
            ORDER BY
                eth_txs_history.created_at DESC
            LIMIT
                1
            "#,
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(history_item.map(|tx| tx.into()))
    }

    /// Returns the next nonce for the operator account
    ///
    /// # Params
    /// * `from_address`: an optional value indicating that nonce must be returned for a custom
    ///   operator address which is not the "main" one. For example, a separate custom operator
    ///   sends the blob transactions. For such a case this should be `Some`. For requesting the
    ///   none of the main operator this parameter should be set to `None`.
    pub async fn get_next_nonce(
        &mut self,
        from_address: Option<Address>,
    ) -> sqlx::Result<Option<u64>> {
        struct NonceRow {
            nonce: i64,
        }

        let query = match_query_as!(
            NonceRow,
            [
                "SELECT nonce FROM eth_txs WHERE ",
                _, // WHERE condition
                " ORDER BY id DESC LIMIT 1"
            ],
            match (from_address) {
                Some(address) => ("from_addr = $1::bytea"; address.as_bytes()),
                None => ("from_addr IS NULL";),
            }
        );

        let nonce = query
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.nonce as u64);
        Ok(nonce.map(|n| n + 1))
    }

    pub async fn mark_failed_transaction(&mut self, eth_tx_id: u32) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE eth_txs
            SET
                has_failed = TRUE
            WHERE
                id = $1
            "#,
            eth_tx_id as i32
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_number_of_failed_transactions(&mut self) -> anyhow::Result<i64> {
        sqlx::query!(
            r#"
            SELECT
                COUNT(*)
            FROM
                eth_txs
            WHERE
                has_failed = TRUE
            "#
        )
        .fetch_one(self.storage.conn())
        .await?
        .count
        .context("count field is missing")
    }

    pub async fn clear_failed_transactions(&mut self) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM eth_txs
            WHERE
                id >= (
                    SELECT
                        MIN(id)
                    FROM
                        eth_txs
                    WHERE
                        has_failed = TRUE
                )
            "#
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn delete_eth_txs(&mut self, last_batch_to_keep: L1BatchNumber) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM eth_txs
            WHERE
                id IN (
                    (
                        SELECT
                            eth_commit_tx_id
                        FROM
                            l1_batches
                        WHERE
                            number > $1
                    )
                    UNION
                    (
                        SELECT
                            eth_prove_tx_id
                        FROM
                            l1_batches
                        WHERE
                            number > $1
                    )
                    UNION
                    (
                        SELECT
                            eth_execute_tx_id
                        FROM
                            l1_batches
                        WHERE
                            number > $1
                    )
                )
            "#,
            i64::from(last_batch_to_keep.0)
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }
}
