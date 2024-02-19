use std::{
    collections::HashMap,
    convert::{Into, TryInto},
    ops,
};

use anyhow::Context as _;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use sqlx::Row;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::{BlockGasCount, L1BatchHeader, L1BatchTreeData, MiniblockHeader},
    circuit::CircuitStatistic,
    commitment::{L1BatchCommitmentArtifacts, L1BatchWithMetadata},
    zk_evm_types::LogQuery,
    Address, L1BatchNumber, MiniblockNumber, ProtocolVersionId, H256, U256,
};

use crate::{
    instrument::InstrumentExt,
    models::storage_block::{StorageL1Batch, StorageL1BatchHeader, StorageMiniblockHeader},
    StorageProcessor,
};

#[derive(Debug)]
pub struct BlocksDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl BlocksDal<'_, '_> {
    pub async fn is_genesis_needed(&mut self) -> sqlx::Result<bool> {
        let count = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                l1_batches
            "#
        )
        .fetch_one(self.storage.conn())
        .await?
        .count;
        Ok(count == 0)
    }

    pub async fn get_sealed_l1_batch_number(&mut self) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                l1_batches
            "#
        )
        .instrument("get_sealed_block_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_sealed_miniblock_number(&mut self) -> sqlx::Result<Option<MiniblockNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                miniblocks
            "#
        )
        .instrument("get_sealed_miniblock_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|number| MiniblockNumber(number as u32)))
    }

    /// Returns the number of the earliest L1 batch present in the DB, or `None` if there are no L1 batches.
    pub async fn get_earliest_l1_batch_number(&mut self) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS "number"
            FROM
                l1_batches
            "#
        )
        .instrument("get_earliest_l1_batch_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_last_l1_batch_number_with_metadata(
        &mut self,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                l1_batches
            WHERE
                hash IS NOT NULL
            "#
        )
        .instrument("get_last_block_number_with_metadata")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_next_l1_batch_ready_for_commitment_generation(
        &mut self,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
            WHERE
                hash IS NOT NULL
                AND commitment IS NULL
            ORDER BY
                number
            LIMIT
                1
            "#
        )
        .instrument("get_next_l1_batch_ready_for_commitment_generation")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| L1BatchNumber(row.number as u32)))
    }

    /// Returns the number of the earliest L1 batch with metadata (= state hash) present in the DB,
    /// or `None` if there are no such L1 batches.
    pub async fn get_earliest_l1_batch_number_with_metadata(
        &mut self,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS "number"
            FROM
                l1_batches
            WHERE
                hash IS NOT NULL
            "#
        )
        .instrument("get_earliest_l1_batch_number_with_metadata")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_l1_batches_for_eth_tx_id(
        &mut self,
        eth_tx_id: u32,
    ) -> sqlx::Result<Vec<L1BatchHeader>> {
        let l1_batches = sqlx::query_as!(
            StorageL1BatchHeader,
            r#"
            SELECT
                number,
                l1_tx_count,
                l2_tx_count,
                timestamp,
                l2_to_l1_logs,
                l2_to_l1_messages,
                bloom,
                priority_ops_onchain_data,
                used_contract_hashes,
                bootloader_code_hash,
                default_aa_code_hash,
                protocol_version,
                system_logs,
                compressed_state_diffs,
                pubdata_input
            FROM
                l1_batches
            WHERE
                eth_commit_tx_id = $1
                OR eth_prove_tx_id = $1
                OR eth_execute_tx_id = $1
            "#,
            eth_tx_id as i32
        )
        .instrument("get_l1_batches_for_eth_tx_id")
        .with_arg("eth_tx_id", &eth_tx_id)
        .fetch_all(self.storage)
        .await?;

        Ok(l1_batches.into_iter().map(Into::into).collect())
    }

    pub async fn get_storage_l1_batch(
        &mut self,
        number: L1BatchNumber,
    ) -> sqlx::Result<Option<StorageL1Batch>> {
        sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                bootloader_code_hash,
                default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                system_logs,
                compressed_state_diffs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .instrument("get_storage_l1_batch")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await
    }

    pub async fn get_l1_batch_header(
        &mut self,
        number: L1BatchNumber,
    ) -> sqlx::Result<Option<L1BatchHeader>> {
        Ok(sqlx::query_as!(
            StorageL1BatchHeader,
            r#"
            SELECT
                number,
                l1_tx_count,
                l2_tx_count,
                timestamp,
                l2_to_l1_logs,
                l2_to_l1_messages,
                bloom,
                priority_ops_onchain_data,
                used_contract_hashes,
                bootloader_code_hash,
                default_aa_code_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                pubdata_input
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .instrument("get_l1_batch_header")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        .map(Into::into))
    }

    /// Returns initial bootloader heap content for the specified L1 batch.
    pub async fn get_initial_bootloader_heap(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<(usize, U256)>>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                initial_bootloader_heap_content
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .instrument("get_initial_bootloader_heap")
        .report_latency()
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        let heap = serde_json::from_value(row.initial_bootloader_heap_content)
            .context("invalid value for initial_bootloader_heap_content in the DB")?;
        Ok(Some(heap))
    }

    pub async fn get_storage_refunds(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<u32>>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                storage_refunds
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .instrument("get_storage_refunds")
        .report_latency()
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let Some(storage_refunds) = row.storage_refunds else {
            return Ok(None);
        };

        let storage_refunds: Vec<_> = storage_refunds.into_iter().map(|n| n as u32).collect();
        Ok(Some(storage_refunds))
    }

    pub async fn get_events_queue(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<LogQuery>>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                serialized_events_queue
            FROM
                events_queue
            WHERE
                l1_batch_number = $1
            "#,
            number.0 as i64
        )
        .instrument("get_events_queue")
        .report_latency()
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        let events = serde_json::from_value(row.serialized_events_queue)
            .context("invalid value for serialized_events_queue in the DB")?;
        Ok(Some(events))
    }

    pub async fn set_eth_tx_id(
        &mut self,
        number_range: ops::RangeInclusive<L1BatchNumber>,
        eth_tx_id: u32,
        aggregation_type: AggregatedActionType,
    ) -> sqlx::Result<()> {
        match aggregation_type {
            AggregatedActionType::Commit => {
                sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_commit_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                    "#,
                    eth_tx_id as i32,
                    number_range.start().0 as i64,
                    number_range.end().0 as i64
                )
                .execute(self.storage.conn())
                .await?;
            }
            AggregatedActionType::PublishProofOnchain => {
                sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_prove_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                    "#,
                    eth_tx_id as i32,
                    number_range.start().0 as i64,
                    number_range.end().0 as i64
                )
                .execute(self.storage.conn())
                .await?;
            }
            AggregatedActionType::Execute => {
                sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_execute_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                    "#,
                    eth_tx_id as i32,
                    number_range.start().0 as i64,
                    number_range.end().0 as i64
                )
                .execute(self.storage.conn())
                .await?;
            }
        }
        Ok(())
    }

    pub async fn insert_l1_batch(
        &mut self,
        header: &L1BatchHeader,
        initial_bootloader_contents: &[(usize, U256)],
        predicted_block_gas: BlockGasCount,
        events_queue: &[LogQuery],
        storage_refunds: &[u32],
        predicted_circuits_by_type: CircuitStatistic, // predicted number of circuits for each circuit type
    ) -> anyhow::Result<()> {
        let priority_onchain_data: Vec<Vec<u8>> = header
            .priority_ops_onchain_data
            .iter()
            .map(|data| data.clone().into())
            .collect();
        let l2_to_l1_logs: Vec<_> = header
            .l2_to_l1_logs
            .iter()
            .map(|log| log.0.to_bytes().to_vec())
            .collect();
        let system_logs = header
            .system_logs
            .iter()
            .map(|log| log.0.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>();
        let pubdata_input = header.pubdata_input.clone();

        // Serialization should always succeed.
        let initial_bootloader_contents = serde_json::to_value(initial_bootloader_contents)
            .expect("failed to serialize initial_bootloader_contents to JSON value");
        let events_queue = serde_json::to_value(events_queue)
            .expect("failed to serialize events_queue to JSON value");
        // Serialization should always succeed.
        let used_contract_hashes = serde_json::to_value(&header.used_contract_hashes)
            .expect("failed to serialize used_contract_hashes to JSON value");
        let storage_refunds: Vec<_> = storage_refunds.iter().map(|n| *n as i64).collect();

        let mut transaction = self.storage.start_transaction().await?;
        sqlx::query!(
            r#"
            INSERT INTO
                l1_batches (
                    number,
                    l1_tx_count,
                    l2_tx_count,
                    timestamp,
                    l2_to_l1_logs,
                    l2_to_l1_messages,
                    bloom,
                    priority_ops_onchain_data,
                    predicted_commit_gas_cost,
                    predicted_prove_gas_cost,
                    predicted_execute_gas_cost,
                    initial_bootloader_heap_content,
                    used_contract_hashes,
                    bootloader_code_hash,
                    default_aa_code_hash,
                    protocol_version,
                    system_logs,
                    storage_refunds,
                    pubdata_input,
                    predicted_circuits_by_type,
                    created_at,
                    updated_at
                )
            VALUES
                (
                    $1,
                    $2,
                    $3,
                    $4,
                    $5,
                    $6,
                    $7,
                    $8,
                    $9,
                    $10,
                    $11,
                    $12,
                    $13,
                    $14,
                    $15,
                    $16,
                    $17,
                    $18,
                    $19,
                    $20,
                    NOW(),
                    NOW()
                )
            "#,
            header.number.0 as i64,
            header.l1_tx_count as i32,
            header.l2_tx_count as i32,
            header.timestamp as i64,
            &l2_to_l1_logs,
            &header.l2_to_l1_messages,
            header.bloom.as_bytes(),
            &priority_onchain_data,
            predicted_block_gas.commit as i64,
            predicted_block_gas.prove as i64,
            predicted_block_gas.execute as i64,
            initial_bootloader_contents,
            used_contract_hashes,
            header.base_system_contracts_hashes.bootloader.as_bytes(),
            header.base_system_contracts_hashes.default_aa.as_bytes(),
            header.protocol_version.map(|v| v as i32),
            &system_logs,
            &storage_refunds,
            pubdata_input,
            serde_json::to_value(predicted_circuits_by_type).unwrap(),
        )
        .execute(transaction.conn())
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO
                events_queue (l1_batch_number, serialized_events_queue)
            VALUES
                ($1, $2)
            "#,
            header.number.0 as i64,
            events_queue
        )
        .execute(transaction.conn())
        .await?;
        transaction.commit().await?;

        Ok(())
    }

    pub async fn insert_miniblock(
        &mut self,
        miniblock_header: &MiniblockHeader,
    ) -> anyhow::Result<()> {
        let base_fee_per_gas = BigDecimal::from_u64(miniblock_header.base_fee_per_gas)
            .context("base_fee_per_gas should fit in u64")?;

        sqlx::query!(
            r#"
            INSERT INTO
                miniblocks (
                    number,
                    timestamp,
                    hash,
                    l1_tx_count,
                    l2_tx_count,
                    fee_account_address,
                    base_fee_per_gas,
                    l1_gas_price,
                    l2_fair_gas_price,
                    gas_per_pubdata_limit,
                    bootloader_code_hash,
                    default_aa_code_hash,
                    protocol_version,
                    virtual_blocks,
                    fair_pubdata_price,
                    created_at,
                    updated_at
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, NOW(), NOW())
            "#,
            miniblock_header.number.0 as i64,
            miniblock_header.timestamp as i64,
            miniblock_header.hash.as_bytes(),
            miniblock_header.l1_tx_count as i32,
            miniblock_header.l2_tx_count as i32,
            miniblock_header.fee_account_address.as_bytes(),
            base_fee_per_gas,
            miniblock_header.batch_fee_input.l1_gas_price() as i64,
            miniblock_header.batch_fee_input.fair_l2_gas_price() as i64,
            miniblock_header.gas_per_pubdata_limit as i64,
            miniblock_header
                .base_system_contracts_hashes
                .bootloader
                .as_bytes(),
            miniblock_header
                .base_system_contracts_hashes
                .default_aa
                .as_bytes(),
            miniblock_header.protocol_version.map(|v| v as i32),
            miniblock_header.virtual_blocks as i64,
            miniblock_header.batch_fee_input.fair_pubdata_price() as i64,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_last_sealed_miniblock_header(
        &mut self,
    ) -> sqlx::Result<Option<MiniblockHeader>> {
        let header = sqlx::query_as!(
            StorageMiniblockHeader,
            r#"
            SELECT
                number,
                timestamp,
                hash,
                l1_tx_count,
                l2_tx_count,
                fee_account_address AS "fee_account_address!",
                base_fee_per_gas,
                l1_gas_price,
                l2_fair_gas_price,
                gas_per_pubdata_limit,
                bootloader_code_hash,
                default_aa_code_hash,
                protocol_version,
                virtual_blocks,
                fair_pubdata_price
            FROM
                miniblocks
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        let Some(header) = header else {
            return Ok(None);
        };
        let mut header = MiniblockHeader::from(header);
        // FIXME (PLA-728): remove after 2nd phase of `fee_account_address` migration
        #[allow(deprecated)]
        self.maybe_load_fee_address(&mut header.fee_account_address, header.number)
            .await?;

        Ok(Some(header))
    }

    pub async fn get_miniblock_header(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<Option<MiniblockHeader>> {
        let header = sqlx::query_as!(
            StorageMiniblockHeader,
            r#"
            SELECT
                number,
                timestamp,
                hash,
                l1_tx_count,
                l2_tx_count,
                fee_account_address AS "fee_account_address!",
                base_fee_per_gas,
                l1_gas_price,
                l2_fair_gas_price,
                gas_per_pubdata_limit,
                bootloader_code_hash,
                default_aa_code_hash,
                protocol_version,
                virtual_blocks,
                fair_pubdata_price
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            miniblock_number.0 as i64,
        )
        .fetch_optional(self.storage.conn())
        .await?;

        let Some(header) = header else {
            return Ok(None);
        };
        let mut header = MiniblockHeader::from(header);
        // FIXME (PLA-728): remove after 2nd phase of `fee_account_address` migration
        #[allow(deprecated)]
        self.maybe_load_fee_address(&mut header.fee_account_address, header.number)
            .await?;

        Ok(Some(header))
    }

    pub async fn mark_miniblocks_as_executed_in_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                l1_batch_number = $1
            WHERE
                l1_batch_number IS NULL
            "#,
            l1_batch_number.0 as i32,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn save_l1_batch_tree_data(
        &mut self,
        number: L1BatchNumber,
        tree_data: &L1BatchTreeData,
    ) -> anyhow::Result<()> {
        let update_result = sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                hash = $1,
                merkle_root_hash = $1,
                rollup_last_leaf_index = $2,
                updated_at = NOW()
            WHERE
                number = $3
                AND hash IS NULL
            "#,
            tree_data.hash.as_bytes(),
            tree_data.rollup_last_leaf_index as i64,
            number.0 as i64,
        )
        .instrument("save_batch_tree_data")
        .with_arg("number", &number)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!("L1 batch #{number}: tree data wasn't updated as it's already present");

            // Batch was already processed. Verify that existing hash matches
            let matched: i64 = sqlx::query!(
                r#"
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    l1_batches
                WHERE
                    number = $1
                    AND hash = $2
                "#,
                number.0 as i64,
                tree_data.hash.as_bytes(),
            )
            .instrument("get_matching_batch_hash")
            .with_arg("number", &number)
            .report_latency()
            .fetch_one(self.storage)
            .await?
            .count;

            anyhow::ensure!(
                matched == 1,
                "Root hash verification failed. Hash for L1 batch #{} does not match the expected value \
                 (expected root hash: {:?})",
                number,
                tree_data.hash,
            );
        }
        Ok(())
    }

    pub async fn save_l1_batch_commitment_artifacts(
        &mut self,
        number: L1BatchNumber,
        commitment_artifacts: &L1BatchCommitmentArtifacts,
    ) -> anyhow::Result<()> {
        let mut transaction = self.storage.start_transaction().await?;

        let update_result = sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                commitment = $1,
                aux_data_hash = $2,
                pass_through_data_hash = $3,
                meta_parameters_hash = $4,
                l2_l1_merkle_root = $5,
                zkporter_is_available = $6,
                compressed_state_diffs = $7,
                compressed_initial_writes = $8,
                compressed_repeated_writes = $9,
                updated_at = NOW()
            WHERE
                number = $10
                AND commitment IS NULL
            "#,
            commitment_artifacts.commitment_hash.commitment.as_bytes(),
            commitment_artifacts.commitment_hash.aux_output.as_bytes(),
            commitment_artifacts
                .commitment_hash
                .pass_through_data
                .as_bytes(),
            commitment_artifacts
                .commitment_hash
                .meta_parameters
                .as_bytes(),
            commitment_artifacts.l2_l1_merkle_root.as_bytes(),
            commitment_artifacts.zkporter_is_available,
            commitment_artifacts.compressed_state_diffs,
            commitment_artifacts.compressed_initial_writes,
            commitment_artifacts.compressed_repeated_writes,
            number.0 as i64,
        )
        .instrument("save_l1_batch_commitment_artifacts")
        .with_arg("number", &number)
        .report_latency()
        .execute(&mut transaction)
        .await?;
        if update_result.rows_affected() == 0 {
            tracing::debug!(
                "L1 batch #{number}: commitment info wasn't updated as it's already present"
            );

            // Batch was already processed. Verify that existing commitment matches
            let matched: i64 = sqlx::query!(
                r#"
                SELECT
                    COUNT(*) AS "count!"
                FROM
                    l1_batches
                WHERE
                    number = $1
                    AND commitment = $2
                "#,
                number.0 as i64,
                commitment_artifacts.commitment_hash.commitment.as_bytes(),
            )
            .instrument("get_matching_batch_commitment")
            .with_arg("number", &number)
            .report_latency()
            .fetch_one(&mut transaction)
            .await?
            .count;

            anyhow::ensure!(
                matched == 1,
                "Commitment verification failed. Commitment for L1 batch #{} does not match the expected value \
                 (expected commitment: {:?})",
                number,
                commitment_artifacts.commitment_hash.commitment
            );
        }

        sqlx::query!(
            r#"
            INSERT INTO
                commitments (l1_batch_number, events_queue_commitment, bootloader_initial_content_commitment)
            VALUES
                ($1, $2, $3)
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            number.0 as i64,
            commitment_artifacts.aux_commitments.map(|a| a.events_queue_commitment.0.to_vec()),
            commitment_artifacts.aux_commitments
                .map(|a| a.bootloader_initial_content_commitment.0.to_vec()),
        )
        .instrument("save_batch_aux_commitments")
        .with_arg("number", &number)
        .report_latency()
        .execute(&mut transaction)
        .await?;

        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_last_committed_to_eth_l1_batch(
        &mut self,
    ) -> anyhow::Result<Option<L1BatchWithMetadata>> {
        // We can get 0 block for the first transaction
        let block = sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                bootloader_code_hash,
                default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            WHERE
                number = 0
                OR eth_commit_tx_id IS NOT NULL
                AND commitment IS NOT NULL
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_last_committed_to_eth_l1_batch")
        .fetch_one(self.storage)
        .await?;
        // genesis block is first generated without commitment, we should wait for the tree to set it.
        if block.commitment.is_none() {
            return Ok(None);
        }

        self.get_l1_batch_with_metadata(block)
            .await
            .context("get_l1_batch_with_metadata()")
    }

    /// Returns the number of the last L1 batch for which an Ethereum commit tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_committed_on_eth(
        &mut self,
    ) -> Result<Option<L1BatchNumber>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
                LEFT JOIN eth_txs_history AS commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id)
            WHERE
                commit_tx.confirmed_at IS NOT NULL
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| L1BatchNumber(row.number as u32)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum prove tx exists in the database.
    pub async fn get_last_l1_batch_with_prove_tx(&mut self) -> sqlx::Result<L1BatchNumber> {
        let row = sqlx::query!(
            r#"
            SELECT
                COALESCE(MAX(number), 0) AS "number!"
            FROM
                l1_batches
            WHERE
                eth_prove_tx_id IS NOT NULL
            "#
        )
        .fetch_one(self.storage.conn())
        .await?;

        Ok(L1BatchNumber(row.number as u32))
    }

    pub async fn get_eth_commit_tx_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u64>> {
        let row = sqlx::query!(
            r#"
            SELECT
                eth_commit_tx_id
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?;

        Ok(row.and_then(|row| row.eth_commit_tx_id.map(|n| n as u64)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum prove tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_proven_on_eth(
        &mut self,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
                LEFT JOIN eth_txs_history AS prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id)
            WHERE
                prove_tx.confirmed_at IS NOT NULL
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|record| L1BatchNumber(record.number as u32)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum execute tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_executed_on_eth(
        &mut self,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
                LEFT JOIN eth_txs_history AS execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id)
            WHERE
                execute_tx.confirmed_at IS NOT NULL
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| L1BatchNumber(row.number as u32)))
    }

    /// This method returns batches that are confirmed on L1. That is, it doesn't wait for the proofs to be generated.
    pub async fn get_ready_for_dummy_proof_l1_batches(
        &mut self,
        limit: usize,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let raw_batches = sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                bootloader_code_hash,
                default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            WHERE
                eth_commit_tx_id IS NOT NULL
                AND eth_prove_tx_id IS NULL
            ORDER BY
                number
            LIMIT
                $1
            "#,
            limit as i32
        )
        .instrument("get_ready_for_dummy_proof_l1_batches")
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    async fn map_l1_batches(
        &mut self,
        raw_batches: Vec<StorageL1Batch>,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let mut l1_batches = Vec::with_capacity(raw_batches.len());
        for raw_batch in raw_batches {
            let block = self
                .get_l1_batch_with_metadata(raw_batch)
                .await
                .context("get_l1_batch_with_metadata()")?
                .context("Block should be complete")?;
            l1_batches.push(block);
        }
        Ok(l1_batches)
    }

    pub async fn set_skip_proof_for_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                skip_proof = TRUE
            WHERE
                number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    /// This method returns batches that are committed on L1 and witness jobs for them are skipped.
    pub async fn get_skipped_for_proof_l1_batches(
        &mut self,
        limit: usize,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let last_proved_block_number = self
            .get_last_l1_batch_with_prove_tx()
            .await
            .context("get_last_l1_batch_with_prove_tx()")?;
        // Witness jobs can be processed out of order, so `WHERE l1_batches.number - row_number = $1`
        // is used to avoid having gaps in the list of blocks to send dummy proofs for.
        let raw_batches = sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                bootloader_code_hash,
                default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                system_logs,
                compressed_state_diffs,
                protocol_version,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                (
                    SELECT
                        l1_batches.*,
                        ROW_NUMBER() OVER (
                            ORDER BY
                                number ASC
                        ) AS ROW_NUMBER
                    FROM
                        l1_batches
                    WHERE
                        eth_commit_tx_id IS NOT NULL
                        AND l1_batches.skip_proof = TRUE
                        AND l1_batches.number > $1
                    ORDER BY
                        number
                    LIMIT
                        $2
                ) inn
                LEFT JOIN commitments ON commitments.l1_batch_number = inn.number
            WHERE
                number - ROW_NUMBER = $1
            "#,
            last_proved_block_number.0 as i32,
            limit as i32
        )
        .instrument("get_skipped_for_proof_l1_batches")
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    pub async fn get_ready_for_execute_l1_batches(
        &mut self,
        limit: usize,
        max_l1_batch_timestamp_millis: Option<u64>,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let raw_batches = match max_l1_batch_timestamp_millis {
            None => {
                sqlx::query_as!(
                    StorageL1Batch,
                    r#"
                    SELECT
                        number,
                        timestamp,
                        l1_tx_count,
                        l2_tx_count,
                        bloom,
                        priority_ops_onchain_data,
                        hash,
                        commitment,
                        eth_prove_tx_id,
                        eth_commit_tx_id,
                        eth_execute_tx_id,
                        merkle_root_hash,
                        l2_to_l1_logs,
                        l2_to_l1_messages,
                        used_contract_hashes,
                        compressed_initial_writes,
                        compressed_repeated_writes,
                        l2_l1_merkle_root,
                        rollup_last_leaf_index,
                        zkporter_is_available,
                        bootloader_code_hash,
                        default_aa_code_hash,
                        aux_data_hash,
                        pass_through_data_hash,
                        meta_parameters_hash,
                        protocol_version,
                        compressed_state_diffs,
                        system_logs,
                        events_queue_commitment,
                        bootloader_initial_content_commitment,
                        pubdata_input
                    FROM
                        l1_batches
                        LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                    WHERE
                        eth_prove_tx_id IS NOT NULL
                        AND eth_execute_tx_id IS NULL
                    ORDER BY
                        number
                    LIMIT
                        $1
                    "#,
                    limit as i32,
                )
                .instrument("get_ready_for_execute_l1_batches/no_max_timestamp")
                .with_arg("limit", &limit)
                .fetch_all(self.storage)
                .await?
            }

            Some(max_l1_batch_timestamp_millis) => {
                // Do not lose the precision here, otherwise we can skip some L1 batches.
                // Mostly needed for tests.
                let max_l1_batch_timestamp_seconds = max_l1_batch_timestamp_millis as f64 / 1_000.0;
                self.raw_ready_for_execute_l1_batches(max_l1_batch_timestamp_seconds, limit)
                    .await
                    .context("raw_ready_for_execute_l1_batches()")?
            }
        };

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    async fn raw_ready_for_execute_l1_batches(
        &mut self,
        max_l1_batch_timestamp_seconds: f64,
        limit: usize,
    ) -> anyhow::Result<Vec<StorageL1Batch>> {
        // We need to find the first L1 batch that is supposed to be executed.
        // Here we ignore the time delay, so we just take the first L1 batch that is ready for execution.
        let row = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
            WHERE
                eth_prove_tx_id IS NOT NULL
                AND eth_execute_tx_id IS NULL
            ORDER BY
                number
            LIMIT
                1
            "#
        )
        .fetch_optional(self.storage.conn())
        .await?;

        let Some(row) = row else { return Ok(vec![]) };
        let expected_started_point = row.number;

        // After Postgres 12->14 upgrade this field is now f64
        let max_l1_batch_timestamp_seconds_bd =
            BigDecimal::from_f64(max_l1_batch_timestamp_seconds)
                .context("Failed to convert f64 to BigDecimal")?;

        // Find the last L1 batch that is ready for execution.
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(l1_batches.number)
            FROM
                l1_batches
                JOIN eth_txs ON (l1_batches.eth_commit_tx_id = eth_txs.id)
                JOIN eth_txs_history AS commit_tx ON (eth_txs.confirmed_eth_tx_history_id = commit_tx.id)
            WHERE
                commit_tx.confirmed_at IS NOT NULL
                AND eth_prove_tx_id IS NOT NULL
                AND eth_execute_tx_id IS NULL
                AND EXTRACT(
                    epoch
                    FROM
                        commit_tx.confirmed_at
                ) < $1
            "#,
            max_l1_batch_timestamp_seconds_bd,
        )
        .fetch_one(self.storage.conn())
        .await?;

        Ok(if let Some(max_ready_to_send_block) = row.max {
            // If we found at least one ready to execute batch then we can simply return all blocks between
            // the expected started point and the max ready to send block because we send them to the L1 sequentially.
            assert!(max_ready_to_send_block >= expected_started_point);
            sqlx::query_as!(
                StorageL1Batch,
                r#"
                SELECT
                    number,
                    timestamp,
                    l1_tx_count,
                    l2_tx_count,
                    bloom,
                    priority_ops_onchain_data,
                    hash,
                    commitment,
                    eth_prove_tx_id,
                    eth_commit_tx_id,
                    eth_execute_tx_id,
                    merkle_root_hash,
                    l2_to_l1_logs,
                    l2_to_l1_messages,
                    used_contract_hashes,
                    compressed_initial_writes,
                    compressed_repeated_writes,
                    l2_l1_merkle_root,
                    rollup_last_leaf_index,
                    zkporter_is_available,
                    bootloader_code_hash,
                    default_aa_code_hash,
                    aux_data_hash,
                    pass_through_data_hash,
                    meta_parameters_hash,
                    protocol_version,
                    compressed_state_diffs,
                    system_logs,
                    events_queue_commitment,
                    bootloader_initial_content_commitment,
                    pubdata_input
                FROM
                    l1_batches
                    LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                WHERE
                    number BETWEEN $1 AND $2
                ORDER BY
                    number
                LIMIT
                    $3
                "#,
                expected_started_point as i32,
                max_ready_to_send_block,
                limit as i32,
            )
            .instrument("get_ready_for_execute_l1_batches")
            .with_arg(
                "numbers",
                &(expected_started_point..=max_ready_to_send_block),
            )
            .with_arg("limit", &limit)
            .fetch_all(self.storage)
            .await?
        } else {
            vec![]
        })
    }

    pub async fn pre_boojum_get_ready_for_commit_l1_batches(
        &mut self,
        limit: usize,
        bootloader_hash: H256,
        default_aa_hash: H256,
        protocol_version_id: ProtocolVersionId,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let raw_batches = sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                l1_batches.timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                l1_batches.bootloader_code_hash,
                l1_batches.default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                JOIN protocol_versions ON protocol_versions.id = l1_batches.protocol_version
            WHERE
                eth_commit_tx_id IS NULL
                AND number != 0
                AND protocol_versions.bootloader_code_hash = $1
                AND protocol_versions.default_account_code_hash = $2
                AND commitment IS NOT NULL
                AND (
                    protocol_versions.id = $3
                    OR protocol_versions.upgrade_tx_hash IS NULL
                )
            ORDER BY
                number
            LIMIT
                $4
            "#,
            bootloader_hash.as_bytes(),
            default_aa_hash.as_bytes(),
            protocol_version_id as i32,
            limit as i64,
        )
        .instrument("get_ready_for_commit_l1_batches")
        .with_arg("limit", &limit)
        .with_arg("bootloader_hash", &bootloader_hash)
        .with_arg("default_aa_hash", &default_aa_hash)
        .with_arg("protocol_version_id", &protocol_version_id)
        .fetch_all(self.storage)
        .await?;

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    pub async fn get_ready_for_commit_l1_batches(
        &mut self,
        limit: usize,
        bootloader_hash: H256,
        default_aa_hash: H256,
        protocol_version_id: ProtocolVersionId,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let raw_batches = sqlx::query_as!(
            StorageL1Batch,
            r#"
            SELECT
                number,
                l1_batches.timestamp,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                hash,
                commitment,
                eth_prove_tx_id,
                eth_commit_tx_id,
                eth_execute_tx_id,
                merkle_root_hash,
                l2_to_l1_logs,
                l2_to_l1_messages,
                used_contract_hashes,
                compressed_initial_writes,
                compressed_repeated_writes,
                l2_l1_merkle_root,
                rollup_last_leaf_index,
                zkporter_is_available,
                l1_batches.bootloader_code_hash,
                l1_batches.default_aa_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                JOIN protocol_versions ON protocol_versions.id = l1_batches.protocol_version
            WHERE
                eth_commit_tx_id IS NULL
                AND number != 0
                AND protocol_versions.bootloader_code_hash = $1
                AND protocol_versions.default_account_code_hash = $2
                AND commitment IS NOT NULL
                AND (
                    protocol_versions.id = $3
                    OR protocol_versions.upgrade_tx_hash IS NULL
                )
                AND events_queue_commitment IS NOT NULL
                AND bootloader_initial_content_commitment IS NOT NULL
            ORDER BY
                number
            LIMIT
                $4
            "#,
            bootloader_hash.as_bytes(),
            default_aa_hash.as_bytes(),
            protocol_version_id as i32,
            limit as i64,
        )
        .instrument("get_ready_for_commit_l1_batches")
        .with_arg("limit", &limit)
        .with_arg("bootloader_hash", &bootloader_hash)
        .with_arg("default_aa_hash", &default_aa_hash)
        .with_arg("protocol_version_id", &protocol_version_id)
        .fetch_all(self.storage)
        .await?;

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    pub async fn get_l1_batch_state_root(
        &mut self,
        number: L1BatchNumber,
    ) -> sqlx::Result<Option<H256>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                hash
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        .and_then(|row| row.hash)
        .map(|hash| H256::from_slice(&hash)))
    }

    pub async fn get_l1_batch_state_root_and_timestamp(
        &mut self,
        number: L1BatchNumber,
    ) -> Result<Option<(H256, u64)>, sqlx::Error> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                timestamp,
                hash
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        let Some(hash) = row.hash else {
            return Ok(None);
        };
        Ok(Some((H256::from_slice(&hash), row.timestamp as u64)))
    }

    pub async fn get_l1_batch_metadata(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<L1BatchWithMetadata>> {
        let Some(l1_batch) = self
            .get_storage_l1_batch(number)
            .await
            .context("get_storage_l1_batch()")?
        else {
            return Ok(None);
        };
        self.get_l1_batch_with_metadata(l1_batch)
            .await
            .context("get_l1_batch_with_metadata")
    }

    pub async fn get_l1_batch_tree_data(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<L1BatchTreeData>> {
        let row = sqlx::query!(
            r#"
            SELECT
                hash,
                rollup_last_leaf_index
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?;
        Ok(row.and_then(|row| {
            Some(L1BatchTreeData {
                hash: H256::from_slice(&row.hash?),
                rollup_last_leaf_index: row.rollup_last_leaf_index? as u64,
            })
        }))
    }

    pub async fn get_l1_batch_with_metadata(
        &mut self,
        storage_batch: StorageL1Batch,
    ) -> anyhow::Result<Option<L1BatchWithMetadata>> {
        let unsorted_factory_deps = self
            .get_l1_batch_factory_deps(L1BatchNumber(storage_batch.number as u32))
            .await
            .context("get_l1_batch_factory_deps()")?;
        let header: L1BatchHeader = storage_batch.clone().into();
        let Ok(metadata) = storage_batch.try_into() else {
            return Ok(None);
        };
        let raw_published_bytecode_hashes = self
            .storage
            .events_dal()
            .get_l1_batch_raw_published_bytecode_hashes(header.number)
            .await?;

        Ok(Some(L1BatchWithMetadata::new(
            header,
            metadata,
            unsorted_factory_deps,
            &raw_published_bytecode_hashes,
        )))
    }

    pub async fn get_l1_batch_factory_deps(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<HashMap<H256, Vec<u8>>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                bytecode_hash,
                bytecode
            FROM
                factory_deps
                INNER JOIN miniblocks ON miniblocks.number = factory_deps.miniblock_number
            WHERE
                miniblocks.l1_batch_number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
        .collect())
    }

    pub async fn delete_initial_writes(
        &mut self,
        last_batch_to_keep: L1BatchNumber,
    ) -> sqlx::Result<()> {
        self.delete_initial_writes_inner(Some(last_batch_to_keep))
            .await
    }

    pub async fn delete_initial_writes_inner(
        &mut self,
        last_batch_to_keep: Option<L1BatchNumber>,
    ) -> sqlx::Result<()> {
        let block_number = last_batch_to_keep.map_or(-1, |number| number.0 as i64);
        sqlx::query!(
            r#"
            DELETE FROM initial_writes
            WHERE
                l1_batch_number > $1
            "#,
            block_number
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }
    /// Deletes all L1 batches from the storage so that the specified batch number is the last one left.
    pub async fn delete_l1_batches(
        &mut self,
        last_batch_to_keep: L1BatchNumber,
    ) -> sqlx::Result<()> {
        self.delete_l1_batches_inner(Some(last_batch_to_keep)).await
    }

    async fn delete_l1_batches_inner(
        &mut self,
        last_batch_to_keep: Option<L1BatchNumber>,
    ) -> sqlx::Result<()> {
        let block_number = last_batch_to_keep.map_or(-1, |number| number.0 as i64);
        sqlx::query!(
            r#"
            DELETE FROM l1_batches
            WHERE
                number > $1
            "#,
            block_number
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    /// Deletes all miniblocks from the storage so that the specified miniblock number is the last one left.
    pub async fn delete_miniblocks(
        &mut self,
        last_miniblock_to_keep: MiniblockNumber,
    ) -> sqlx::Result<()> {
        self.delete_miniblocks_inner(Some(last_miniblock_to_keep))
            .await
    }

    async fn delete_miniblocks_inner(
        &mut self,
        last_miniblock_to_keep: Option<MiniblockNumber>,
    ) -> sqlx::Result<()> {
        let block_number = last_miniblock_to_keep.map_or(-1, |number| number.0 as i64);
        sqlx::query!(
            r#"
            DELETE FROM miniblocks
            WHERE
                number > $1
            "#,
            block_number
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    /// Returns sum of predicted gas costs on the given L1 batch range.
    /// Panics if the sum doesn't fit into `u32`.
    pub async fn get_l1_batches_predicted_gas(
        &mut self,
        number_range: ops::RangeInclusive<L1BatchNumber>,
        op_type: AggregatedActionType,
    ) -> anyhow::Result<u32> {
        let column_name = match op_type {
            AggregatedActionType::Commit => "predicted_commit_gas_cost",
            AggregatedActionType::PublishProofOnchain => "predicted_prove_gas_cost",
            AggregatedActionType::Execute => "predicted_execute_gas_cost",
        };
        let sql_query_str = format!(
            "SELECT COALESCE(SUM({column_name}), 0) AS sum FROM l1_batches \
             WHERE number BETWEEN $1 AND $2"
        );
        sqlx::query(&sql_query_str)
            .bind(number_range.start().0 as i64)
            .bind(number_range.end().0 as i64)
            .fetch_one(self.storage.conn())
            .await?
            .get::<BigDecimal, &str>("sum")
            .to_u32()
            .context("Sum of predicted gas costs should fit into u32")
    }

    pub async fn get_miniblock_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<(MiniblockNumber, MiniblockNumber)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(miniblocks.number) AS "min?",
                MAX(miniblocks.number) AS "max?"
            FROM
                miniblocks
            WHERE
                l1_batch_number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_one(self.storage.conn())
        .await?;
        let Some(min) = row.min else { return Ok(None) };
        let Some(max) = row.max else { return Ok(None) };
        Ok(Some((
            MiniblockNumber(min as u32),
            MiniblockNumber(max as u32),
        )))
    }

    /// Returns `true` if there exists a non-sealed batch (i.e. there is one+ stored miniblock that isn't assigned
    /// to any batch yet).
    pub async fn pending_batch_exists(&mut self) -> sqlx::Result<bool> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(miniblocks.number) FROM miniblocks WHERE l1_batch_number IS NULL"
        )
        .fetch_one(self.storage.conn())
        .await?
        .unwrap_or(0);

        Ok(count != 0)
    }

    // methods used for measuring Eth tx stage transition latencies
    // and emitting metrics base on these measured data
    pub async fn oldest_uncommitted_batch_timestamp(&mut self) -> sqlx::Result<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                timestamp
            FROM
                l1_batches
            WHERE
                eth_commit_tx_id IS NULL
                AND number > 0
            ORDER BY
                number
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn oldest_unproved_batch_timestamp(&mut self) -> sqlx::Result<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                timestamp
            FROM
                l1_batches
            WHERE
                eth_prove_tx_id IS NULL
                AND number > 0
            ORDER BY
                number
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn oldest_unexecuted_batch_timestamp(&mut self) -> Result<Option<u64>, sqlx::Error> {
        Ok(sqlx::query!(
            r#"
            SELECT
                timestamp
            FROM
                l1_batches
            WHERE
                eth_execute_tx_id IS NULL
                AND number > 0
            ORDER BY
                number
            LIMIT
                1
            "#,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn get_batch_protocol_version_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<ProtocolVersionId>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            l1_batch_number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        let Some(v) = row.protocol_version else {
            return Ok(None);
        };
        Ok(Some((v as u16).try_into()?))
    }

    pub async fn get_miniblock_protocol_version_id(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> anyhow::Result<Option<ProtocolVersionId>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            miniblock_number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };
        let Some(v) = row.protocol_version else {
            return Ok(None);
        };
        Ok(Some((v as u16).try_into()?))
    }

    pub async fn get_miniblock_timestamp(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<Option<u64>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                timestamp
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            miniblock_number.0 as i64,
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn set_protocol_version_for_pending_miniblocks(
        &mut self,
        id: ProtocolVersionId,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                protocol_version = $1
            WHERE
                l1_batch_number IS NULL
            "#,
            id as i32,
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_fee_address_for_miniblock(
        &mut self,
        number: MiniblockNumber,
    ) -> sqlx::Result<Option<Address>> {
        let Some(mut fee_account_address) = self.raw_fee_address_for_miniblock(number).await?
        else {
            return Ok(None);
        };

        // FIXME (PLA-728): remove after 2nd phase of `fee_account_address` migration
        #[allow(deprecated)]
        self.maybe_load_fee_address(&mut fee_account_address, number)
            .await?;
        Ok(Some(fee_account_address))
    }

    async fn raw_fee_address_for_miniblock(
        &mut self,
        number: MiniblockNumber,
    ) -> sqlx::Result<Option<Address>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                fee_account_address
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(Address::from_slice(&row.fee_account_address)))
    }

    pub async fn get_virtual_blocks_for_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<Option<u32>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                virtual_blocks
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            miniblock_number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.virtual_blocks as u32))
    }
}

/// Temporary methods for migrating `fee_account_address`.
#[deprecated(note = "will be removed after the fee address migration is complete")]
impl BlocksDal<'_, '_> {
    pub(crate) async fn maybe_load_fee_address(
        &mut self,
        fee_address: &mut Address,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<()> {
        if *fee_address != Address::default() {
            return Ok(());
        }

        // This clause should be triggered only for non-migrated miniblock rows. After `fee_account_address`
        // is filled for all miniblocks, it won't be called; thus, `fee_account_address` column could be removed
        // from `l1_batches` even with this code present.
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                l1_batches.fee_account_address
            FROM
                l1_batches
                INNER JOIN miniblocks ON miniblocks.l1_batch_number = l1_batches.number
            WHERE
                miniblocks.number = $1
            "#,
            miniblock_number.0 as i32
        )
        .fetch_optional(self.storage.conn())
        .await?
        else {
            return Ok(());
        };

        *fee_address = Address::from_slice(&row.fee_account_address);
        Ok(())
    }

    /// Checks whether `fee_account_address` is migrated for the specified miniblock. Returns
    /// `Ok(None)` if the miniblock doesn't exist.
    pub async fn is_fee_address_migrated(
        &mut self,
        number: MiniblockNumber,
    ) -> sqlx::Result<Option<bool>> {
        Ok(self
            .raw_fee_address_for_miniblock(number)
            .await?
            .map(|address| address != Address::default()))
    }

    /// Copies `fee_account_address` for pending miniblocks (ones without an associated L1 batch)
    /// from the last L1 batch. Returns the number of affected rows.
    pub async fn copy_fee_account_address_for_pending_miniblocks(&mut self) -> sqlx::Result<u64> {
        let execution_result = sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                fee_account_address = (
                    SELECT
                        l1_batches.fee_account_address
                    FROM
                        l1_batches
                    ORDER BY
                        l1_batches.number DESC
                    LIMIT
                        1
                )
            WHERE
                l1_batch_number IS NULL
                AND fee_account_address = '\x0000000000000000000000000000000000000000'::bytea
            "#
        )
        .execute(self.storage.conn())
        .await?;

        Ok(execution_result.rows_affected())
    }

    pub async fn check_l1_batches_have_fee_account_address(&mut self) -> sqlx::Result<bool> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*)
            FROM information_schema.columns
            WHERE table_name = 'l1_batches' AND column_name = 'fee_account_address'
            "#
        )
        .fetch_one(self.storage.conn())
        .await?
        .unwrap_or(0);

        Ok(count > 0)
    }

    /// Copies `fee_account_address` for miniblocks in the given range from the L1 batch they belong to.
    /// Returns the number of affected rows.
    pub async fn copy_fee_account_address_for_miniblocks(
        &mut self,
        numbers: ops::RangeInclusive<MiniblockNumber>,
    ) -> sqlx::Result<u64> {
        let execution_result = sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                fee_account_address = l1_batches.fee_account_address
            FROM
                l1_batches
            WHERE
                l1_batches.number = miniblocks.l1_batch_number
                AND miniblocks.number BETWEEN $1 AND $2
                AND miniblocks.fee_account_address = '\x0000000000000000000000000000000000000000'::bytea
            "#,
            numbers.start().0 as i64,
            numbers.end().0 as i64
        )
        .execute(self.storage.conn())
        .await?;

        Ok(execution_result.rows_affected())
    }

    /// Sets `fee_account_address` for an L1 batch. Should only be used in tests.
    pub async fn set_l1_batch_fee_address(
        &mut self,
        l1_batch: L1BatchNumber,
        fee_account_address: Address,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                fee_account_address = $1::bytea
            WHERE
                number = $2
            "#,
            fee_account_address.as_bytes(),
            l1_batch.0 as i64
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }
}

/// These methods should only be used for tests.
impl BlocksDal<'_, '_> {
    // The actual l1 batch hash is only set by the metadata calculator.
    pub async fn set_l1_batch_hash(
        &mut self,
        batch_num: L1BatchNumber,
        hash: H256,
    ) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                hash = $1
            WHERE
                number = $2
            "#,
            hash.as_bytes(),
            batch_num.0 as i64
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn insert_mock_l1_batch(&mut self, header: &L1BatchHeader) -> anyhow::Result<()> {
        self.insert_l1_batch(
            header,
            &[],
            Default::default(),
            &[],
            &[],
            Default::default(),
        )
        .await
    }

    /// Deletes all miniblocks and L1 batches, including the genesis ones. Should only be used in tests.
    pub async fn delete_genesis(&mut self) -> anyhow::Result<()> {
        self.delete_miniblocks_inner(None)
            .await
            .context("delete_miniblocks_inner()")?;
        self.delete_l1_batches_inner(None)
            .await
            .context("delete_l1_batches_inner()")?;
        self.delete_initial_writes_inner(None)
            .await
            .context("delete_initial_writes_inner()")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        l2_to_l1_log::{L2ToL1Log, UserL2ToL1Log},
        Address, ProtocolVersion, ProtocolVersionId,
    };

    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool};

    #[tokio::test]
    async fn loading_l1_batch_header() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let mut header = L1BatchHeader::new(
            L1BatchNumber(1),
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

        let loaded_header = conn
            .blocks_dal()
            .get_l1_batch_header(L1BatchNumber(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded_header.number, header.number);
        assert_eq!(loaded_header.timestamp, header.timestamp);
        assert_eq!(loaded_header.l1_tx_count, header.l1_tx_count);
        assert_eq!(loaded_header.l2_tx_count, header.l2_tx_count);
        assert_eq!(loaded_header.l2_to_l1_logs, header.l2_to_l1_logs);
        assert_eq!(loaded_header.l2_to_l1_messages, header.l2_to_l1_messages);

        assert!(conn
            .blocks_dal()
            .get_l1_batch_header(L1BatchNumber(2))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn getting_predicted_gas() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        let mut header = L1BatchHeader::new(
            L1BatchNumber(1),
            100,
            BaseSystemContractsHashes::default(),
            ProtocolVersionId::default(),
        );
        let mut predicted_gas = BlockGasCount {
            commit: 2,
            prove: 3,
            execute: 10,
        };
        conn.blocks_dal()
            .insert_l1_batch(&header, &[], predicted_gas, &[], &[], Default::default())
            .await
            .unwrap();

        header.number = L1BatchNumber(2);
        header.timestamp += 100;
        predicted_gas += predicted_gas;
        conn.blocks_dal()
            .insert_l1_batch(&header, &[], predicted_gas, &[], &[], Default::default())
            .await
            .unwrap();

        let action_types_and_predicted_gas = [
            (AggregatedActionType::Execute, 10),
            (AggregatedActionType::Commit, 2),
            (AggregatedActionType::PublishProofOnchain, 3),
        ];
        for (action_type, expected_gas) in action_types_and_predicted_gas {
            let gas = conn
                .blocks_dal()
                .get_l1_batches_predicted_gas(L1BatchNumber(1)..=L1BatchNumber(1), action_type)
                .await
                .unwrap();
            assert_eq!(gas, expected_gas);

            let gas = conn
                .blocks_dal()
                .get_l1_batches_predicted_gas(L1BatchNumber(2)..=L1BatchNumber(2), action_type)
                .await
                .unwrap();
            assert_eq!(gas, 2 * expected_gas);

            let gas = conn
                .blocks_dal()
                .get_l1_batches_predicted_gas(L1BatchNumber(1)..=L1BatchNumber(2), action_type)
                .await
                .unwrap();
            assert_eq!(gas, 3 * expected_gas);
        }
    }

    #[allow(deprecated)] // that's the whole point
    #[tokio::test]
    async fn checking_fee_account_address_in_l1_batches() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        assert!(conn
            .blocks_dal()
            .check_l1_batches_have_fee_account_address()
            .await
            .unwrap());
    }

    #[allow(deprecated)] // that's the whole point
    #[tokio::test]
    async fn ensuring_fee_account_address_for_miniblocks() {
        let pool = ConnectionPool::test_pool().await;
        let mut conn = pool.access_storage().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        for number in [1, 2] {
            let l1_batch = L1BatchHeader::new(
                L1BatchNumber(number),
                100,
                BaseSystemContractsHashes {
                    bootloader: H256::repeat_byte(1),
                    default_aa: H256::repeat_byte(42),
                },
                ProtocolVersionId::latest(),
            );
            let miniblock = MiniblockHeader {
                fee_account_address: Address::default(),
                ..create_miniblock_header(number)
            };
            conn.blocks_dal()
                .insert_miniblock(&miniblock)
                .await
                .unwrap();
            conn.blocks_dal()
                .insert_mock_l1_batch(&l1_batch)
                .await
                .unwrap();
            conn.blocks_dal()
                .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(number))
                .await
                .unwrap();

            assert_eq!(
                conn.blocks_dal()
                    .is_fee_address_migrated(miniblock.number)
                    .await
                    .unwrap(),
                Some(false)
            );
        }

        // Manually set `fee_account_address` for the inserted L1 batches.
        conn.blocks_dal()
            .set_l1_batch_fee_address(L1BatchNumber(1), Address::repeat_byte(0x23))
            .await
            .unwrap();
        conn.blocks_dal()
            .set_l1_batch_fee_address(L1BatchNumber(2), Address::repeat_byte(0x42))
            .await
            .unwrap();

        // Add a pending miniblock.
        let miniblock = MiniblockHeader {
            fee_account_address: Address::default(),
            ..create_miniblock_header(3)
        };
        conn.blocks_dal()
            .insert_miniblock(&miniblock)
            .await
            .unwrap();

        let rows_affected = conn
            .blocks_dal()
            .copy_fee_account_address_for_miniblocks(MiniblockNumber(0)..=MiniblockNumber(100))
            .await
            .unwrap();

        assert_eq!(rows_affected, 2);
        let first_miniblock_addr = conn
            .blocks_dal()
            .raw_fee_address_for_miniblock(MiniblockNumber(1))
            .await
            .unwrap()
            .expect("No fee address for block #1");
        assert_eq!(first_miniblock_addr, Address::repeat_byte(0x23));
        let second_miniblock_addr = conn
            .blocks_dal()
            .raw_fee_address_for_miniblock(MiniblockNumber(2))
            .await
            .unwrap()
            .expect("No fee address for block #1");
        assert_eq!(second_miniblock_addr, Address::repeat_byte(0x42));
        // The pending miniblock should not be affected.
        let pending_miniblock_addr = conn
            .blocks_dal()
            .raw_fee_address_for_miniblock(MiniblockNumber(3))
            .await
            .unwrap()
            .expect("No fee address for block #3");
        assert_eq!(pending_miniblock_addr, Address::default());
        assert_eq!(
            conn.blocks_dal()
                .is_fee_address_migrated(MiniblockNumber(3))
                .await
                .unwrap(),
            Some(false)
        );

        let rows_affected = conn
            .blocks_dal()
            .copy_fee_account_address_for_pending_miniblocks()
            .await
            .unwrap();
        assert_eq!(rows_affected, 1);

        let pending_miniblock_addr = conn
            .blocks_dal()
            .raw_fee_address_for_miniblock(MiniblockNumber(3))
            .await
            .unwrap()
            .expect("No fee address for block #3");
        assert_eq!(pending_miniblock_addr, Address::repeat_byte(0x42));

        for number in 1..=3 {
            assert_eq!(
                conn.blocks_dal()
                    .is_fee_address_migrated(MiniblockNumber(number))
                    .await
                    .unwrap(),
                Some(true)
            );
        }
    }
}
