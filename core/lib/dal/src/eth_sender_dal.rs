use std::{convert::TryFrom, str::FromStr};

use anyhow::Context as _;
use sqlx::types::chrono::{DateTime, Utc};
use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::InstrumentExt, interpolate_query,
    match_query_as,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    eth_sender::{EthTx, EthTxBlobSidecar, TxHistory},
    Address, L1BatchNumber, SLChainId, H256, U256,
};

use crate::{
    models::storage_eth_tx::{L1BatchEthSenderStats, StorageEthTx, StorageTxHistory},
    Core,
};

#[derive(Debug)]
pub struct EthSenderDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl EthSenderDal<'_, '_> {
    pub async fn get_inflight_txs(
        &mut self,
        operator_address: Address,
        consider_null_operator_address: bool, // TODO (PLA-1118): remove this parameter
        is_gateway: bool,
    ) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            r#"
            SELECT
                *
            FROM
                eth_txs
            WHERE
                (
                    from_addr = $1
                    OR
                    (from_addr IS NULL AND $2)
                )
                AND is_gateway = $3
                AND id <= COALESCE(
                    (SELECT
                        eth_tx_id
                    FROM
                        eth_txs_history
                    JOIN eth_txs ON eth_txs.id = eth_txs_history.eth_tx_id
                    WHERE
                        eth_txs_history.sent_at_block IS NOT NULL
                        AND (
                            eth_txs_history.finality_status != 'finalized'
                        )
                        AND (
                            from_addr = $1
                            OR
                            (from_addr IS NULL AND $2)
                        )
                        AND is_gateway = $3
                    ORDER BY eth_tx_id DESC LIMIT 1),
                    0
                )
            ORDER BY
                id
            "#,
            operator_address.as_bytes(),
            consider_null_operator_address,
            is_gateway,
        )
        .fetch_all(self.storage.conn())
        .await?;
        Ok(txs.into_iter().map(|tx| tx.into()).collect())
    }

    pub async fn get_inflight_txs_count_for_gateway_migration(
        &mut self,
        is_gateway: bool,
    ) -> sqlx::Result<usize> {
        let count = sqlx::query!(
            r#"
            SELECT
                COUNT(*)
            FROM
                eth_txs
            WHERE
                confirmed_eth_tx_history_id IS NULL
                AND is_gateway = $1
            "#,
            is_gateway
        )
        .fetch_one(self.storage.conn())
        .await?
        .count
        .unwrap();
        Ok(count.try_into().unwrap())
    }

    pub async fn get_chain_id_of_last_eth_tx(&mut self) -> DalResult<Option<u64>> {
        let res = sqlx::query!(
            r#"
            SELECT
                chain_id
            FROM
                eth_txs
            ORDER BY id DESC
            LIMIT 1
            "#,
        )
        .instrument("get_settlement_layer_of_last_eth_tx")
        .fetch_optional(self.storage)
        .await?
        .and_then(|row| row.chain_id.map(|a| a as u64));
        Ok(res)
    }

    pub async fn get_unconfirmed_txs_count(&mut self) -> DalResult<usize> {
        let count = sqlx::query!(
            r#"
            SELECT
                COUNT(*)
            FROM
                eth_txs
            WHERE
                confirmed_eth_tx_history_id IS NULL
            "#
        )
        .instrument("get_unconfirmed_txs_count")
        .fetch_one(self.storage)
        .await?
        .count
        .unwrap();
        Ok(count.try_into().unwrap())
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
        operator_address: Address,
        consider_null_operator_address: bool, // TODO (PLA-1118): remove this parameter
        is_gateway: bool,
    ) -> sqlx::Result<Vec<EthTx>> {
        let txs = sqlx::query_as!(
            StorageEthTx,
            r#"
            SELECT
                *
            FROM
                eth_txs
            WHERE
                (
                    from_addr = $2
                    OR
                    (from_addr IS NULL AND $3)
                )
                AND is_gateway = $4
                AND id > COALESCE(
                    (SELECT
                        eth_tx_id
                    FROM
                        eth_txs_history
                    JOIN eth_txs ON eth_txs.id = eth_txs_history.eth_tx_id
                    WHERE
                        eth_txs_history.sent_at_block IS NOT NULL
                        AND (
                            from_addr = $2
                            OR
                            (from_addr IS NULL AND $3)
                        )
                        AND is_gateway = $4
                        AND sent_successfully = TRUE
                    ORDER BY eth_tx_id DESC LIMIT 1),
                    0
                )
            ORDER BY
                id
            LIMIT
                $1
            "#,
            limit as i64,
            operator_address.as_bytes(),
            consider_null_operator_address,
            is_gateway
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
        predicted_gas_cost: Option<u64>,
        from_address: Option<Address>,
        blob_sidecar: Option<EthTxBlobSidecar>,
        is_gateway: bool,
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
                blob_sidecar,
                is_gateway
            )
            VALUES
            ($1, $2, $3, $4, $5, NOW(), NOW(), $6, $7, $8)
            RETURNING
            *
            "#,
            raw_tx,
            nonce as i64,
            tx_type.to_string(),
            address,
            predicted_gas_cost.map(|c| c as i64),
            from_address.as_ref().map(Address::as_bytes),
            blob_sidecar.map(|sidecar| bincode::serialize(&sidecar)
                .expect("can always bincode serialize EthTxBlobSidecar; qed")),
            is_gateway,
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
        max_gas_per_pubdata: Option<u64>,
        tx_hash: H256,
        raw_signed_tx: &[u8],
        sent_at_block: u32,
        predicted_gas_limit: Option<u64>,
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
                max_gas_per_pubdata,
                predicted_gas_limit,
                sent_at_block,
                sent_at,
                sent_successfully,
                finality_status
            
            )
            VALUES
            ($1, $2, $3, $4, $5, NOW(), NOW(), $6, $7, $8, $9, NOW(), FALSE, 'pending')
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
            max_gas_per_pubdata.map(|v| v as i64),
            predicted_gas_limit.map(|v| v as i64),
            sent_at_block as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.id as u32))
    }

    pub async fn tx_history_by_hash(
        &mut self,
        eth_tx_id: u32,
        tx_hash: H256,
    ) -> anyhow::Result<Option<u32>> {
        let tx_hash = format!("{:#x}", tx_hash);
        Ok(sqlx::query!(
            r#"
            SELECT id FROM eth_txs_history
            WHERE eth_tx_id = $1 AND tx_hash = $2
            "#,
            eth_tx_id as i32,
            tx_hash,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.id as u32))
    }

    pub async fn set_sent_success(&mut self, eth_txs_history_id: u32) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE eth_txs_history
            SET
                sent_successfully = TRUE,
                updated_at = NOW()
            WHERE
                id = $1
            "#,
            eth_txs_history_id as i32
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn confirm_tx(
        &mut self,
        tx_hash: H256,
        eth_tx_finality_status: EthTxFinalityStatus,
        gas_used: U256,
    ) -> anyhow::Result<()> {
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
                confirmed_at = NOW(),
                finality_status = $2,
                sent_successfully = TRUE
            WHERE
                tx_hash = $1
            RETURNING
            id,
            eth_tx_id
            "#,
            tx_hash,
            eth_tx_finality_status.to_string(),
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

    pub async fn set_chain_id(&mut self, eth_tx_id: u32, chain_id: u64) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            UPDATE eth_txs
            SET
                chain_id = $1
            WHERE
                id = $2
            "#,
            chain_id as i64,
            eth_tx_id as i32,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_batch_commit_chain_id(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<SLChainId>> {
        let row = sqlx::query!(
            r#"
            SELECT eth_txs.chain_id
            FROM l1_batches
            JOIN eth_txs ON eth_txs.id = l1_batches.eth_commit_tx_id
            WHERE
                number = $1
            "#,
            i64::from(batch_number.0),
        )
        .instrument("get_batch_commit_chain_id")
        .with_arg("batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await?;
        Ok(row.and_then(|r| r.chain_id).map(|id| SLChainId(id as u64)))
    }

    pub async fn get_batch_execute_chain_id(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<SLChainId>> {
        let row = sqlx::query!(
            r#"
            SELECT eth_txs.chain_id
            FROM l1_batches
            JOIN eth_txs ON eth_txs.id = l1_batches.eth_execute_tx_id
            WHERE
                number = $1
            "#,
            i64::from(batch_number.0),
        )
        .instrument("get_batch_execute_chain_id")
        .with_arg("batch_number", &batch_number)
        .fetch_optional(self.storage)
        .await?;
        Ok(row.and_then(|r| r.chain_id).map(|id| SLChainId(id as u64)))
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
        sl_chain_id: Option<SLChainId>,
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
                "INSERT INTO eth_txs (raw_tx, nonce, tx_type, contract_address, predicted_gas_cost, chain_id, created_at, updated_at) \
                VALUES ('\\x00', 0, $1, '', NULL, $2, now(), now()) \
                RETURNING id",
                tx_type.to_string(),
                sl_chain_id.map(|chain_id| chain_id.0 as i64)
            )
            .fetch_one(transaction.conn())
            .await?;

            // TODO mark finality status correspondingly
            // Insert a "sent transaction".
            let eth_history_id = sqlx::query_scalar!(
                "INSERT INTO eth_txs_history \
                (eth_tx_id, base_fee_per_gas, priority_fee_per_gas, tx_hash, signed_raw_tx, created_at, updated_at, confirmed_at, sent_successfully, finality_status) \
                VALUES ($1, 0, 0, $2, '\\x00', now(), now(), $3, TRUE, 'finalized') \
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
            "SELECT MIN(sent_at_block) FROM eth_txs_history WHERE eth_tx_id = $1",
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(sent_at_block.flatten().map(|block| block as u32))
    }

    pub async fn get_block_number_on_last_sent_attempt(
        &mut self,
        eth_tx_id: u32,
    ) -> sqlx::Result<Option<u32>> {
        let sent_at_block = sqlx::query_scalar!(
            "SELECT MAX(sent_at_block) FROM eth_txs_history WHERE eth_tx_id = $1",
            eth_tx_id as i32
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(sent_at_block.flatten().map(|block| block as u32))
    }

    pub async fn get_last_sent_successfully_eth_tx(
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
                eth_tx_id = $1 AND sent_successfully = TRUE
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
    pub async fn get_next_nonce(
        &mut self,
        from_address: Address,
        consider_null_operator_address: bool, // TODO (PLA-1118): remove this parameter
        is_gateway: bool,
    ) -> sqlx::Result<Option<u64>> {
        // First query nonce where `from_addr` is set.
        let row = sqlx::query!(
            r#"
            SELECT
                nonce
            FROM
                eth_txs
            WHERE
                from_addr = $1
                AND is_gateway = $2
            ORDER BY
                id DESC
            LIMIT
                1
            "#,
            from_address.as_bytes(),
            is_gateway,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        if let Some(row) = row {
            return Ok(Some(row.nonce as u64 + 1));
        }

        // Otherwise, check rows with `from_addr IS NULL`.
        if consider_null_operator_address {
            let nonce = sqlx::query!(
                r#"
                SELECT
                    nonce
                FROM
                    eth_txs
                WHERE
                    from_addr IS NULL
                    AND is_gateway = $1
                ORDER BY
                    id DESC
                LIMIT
                    1
                "#,
                is_gateway,
            )
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(nonce.map(|row| row.nonce as u64 + 1))
        } else {
            Ok(None)
        }
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

    pub async fn get_number_of_failed_transactions(&mut self) -> anyhow::Result<u64> {
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
        .map(|c| c as u64)
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

/// These methods should only be used for tests.
impl EthSenderDal<'_, '_> {
    pub async fn get_eth_txs_history_entries_max_id(&mut self) -> usize {
        sqlx::query!(
            r#"
            SELECT
                MAX(id)
            FROM
                eth_txs_history
            "#
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .max
        .unwrap()
        .try_into()
        .unwrap()
    }

    pub async fn get_last_sent_successfully_eth_tx_by_batch_and_op(
        &mut self,
        l1_batch_number: L1BatchNumber,
        op_type: AggregatedActionType,
    ) -> Option<TxHistory> {
        let row = sqlx::query!(
            r#"
            SELECT
                eth_commit_tx_id,
                eth_prove_tx_id,
                eth_execute_tx_id
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .unwrap();
        let eth_tx_id = match op_type {
            AggregatedActionType::Commit => row.eth_commit_tx_id,
            AggregatedActionType::PublishProofOnchain => row.eth_prove_tx_id,
            AggregatedActionType::Execute => row.eth_execute_tx_id,
        }
        .unwrap() as u32;
        self.get_last_sent_successfully_eth_tx(eth_tx_id)
            .await
            .unwrap()
    }
}
