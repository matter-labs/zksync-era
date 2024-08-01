use std::{
    collections::HashMap,
    convert::{Into, TryInto},
    ops,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context as _;
use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use zksync_db_connection::{
    connection::Connection,
    error::{DalResult, SqlxContext},
    instrument::{InstrumentExt, Instrumented},
    interpolate_query, match_query_as,
};
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::{
        BlockGasCount, L1BatchHeader, L1BatchStatistics, L1BatchTreeData, L2BlockHeader,
        StorageOracleInfo,
    },
    circuit::CircuitStatistic,
    commitment::{L1BatchCommitmentArtifacts, L1BatchWithMetadata},
    l2_to_l1_log::UserL2ToL1Log,
    writes::TreeWrite,
    Address, L1BatchNumber, L2BlockNumber, ProtocolVersionId, H256, U256,
};

pub use crate::models::storage_block::{L1BatchMetadataError, L1BatchWithOptionalMetadata};
use crate::{
    models::{
        parse_protocol_version,
        storage_block::{StorageL1Batch, StorageL1BatchHeader, StorageL2BlockHeader},
        storage_event::StorageL2ToL1Log,
        storage_oracle_info::DbStorageOracleInfo,
    },
    Core, CoreDal,
};

#[derive(Debug)]
pub struct BlocksDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BlocksDal<'_, '_> {
    pub async fn get_consistency_checker_last_processed_l1_batch(
        &mut self,
    ) -> DalResult<L1BatchNumber> {
        let row = sqlx::query!(
            r#"
            SELECT
                last_processed_l1_batch AS "last_processed_l1_batch!"
            FROM
                consistency_checker_info
            "#
        )
        .instrument("get_consistency_checker_last_processed_l1_batch")
        .report_latency()
        .fetch_one(self.storage)
        .await?;
        Ok(L1BatchNumber(row.last_processed_l1_batch as u32))
    }

    pub async fn set_consistency_checker_last_processed_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE consistency_checker_info
            SET
                last_processed_l1_batch = $1,
                updated_at = NOW()
            "#,
            l1_batch_number.0 as i32,
        )
        .instrument("set_consistency_checker_last_processed_l1_batch")
        .report_latency()
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn is_genesis_needed(&mut self) -> DalResult<bool> {
        let count = sqlx::query!(
            r#"
            SELECT
                COUNT(*) AS "count!"
            FROM
                l1_batches
            "#
        )
        .instrument("is_genesis_needed")
        .fetch_one(self.storage)
        .await?
        .count;
        Ok(count == 0)
    }

    pub async fn get_sealed_l1_batch_number(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                l1_batches
            "#
        )
        .instrument("get_sealed_l1_batch_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_sealed_l2_block_number(&mut self) -> DalResult<Option<L2BlockNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                miniblocks
            "#
        )
        .instrument("get_sealed_l2_block_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|number| L2BlockNumber(number as u32)))
    }

    /// Returns the number of the earliest L1 batch present in the DB, or `None` if there are no L1 batches.
    pub async fn get_earliest_l1_batch_number(&mut self) -> DalResult<Option<L1BatchNumber>> {
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

    pub async fn get_last_l1_batch_number_with_tree_data(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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
        .instrument("get_last_l1_batch_number_with_tree_data")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    /// Gets a number of the earliest L1 batch that is ready for commitment generation (i.e., doesn't have commitment
    /// yet, and has tree data).
    pub async fn get_next_l1_batch_ready_for_commitment_generation(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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

    /// Gets a number of the last L1 batch that is ready for commitment generation (i.e., doesn't have commitment
    /// yet, and has tree data).
    pub async fn get_last_l1_batch_ready_for_commitment_generation(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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
                number DESC
            LIMIT
                1
            "#
        )
        .instrument("get_last_l1_batch_ready_for_commitment_generation")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| L1BatchNumber(row.number as u32)))
    }

    /// Returns the number of the earliest L1 batch with metadata (= state hash) present in the DB,
    /// or `None` if there are no such L1 batches.
    pub async fn get_earliest_l1_batch_number_with_metadata(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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

    pub async fn get_l1_batches_statistics_for_eth_tx_id(
        &mut self,
        eth_tx_id: u32,
    ) -> DalResult<Vec<L1BatchStatistics>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number,
                l1_tx_count,
                l2_tx_count,
                timestamp
            FROM
                l1_batches
            WHERE
                eth_commit_tx_id = $1
                OR eth_prove_tx_id = $1
                OR eth_execute_tx_id = $1
            "#,
            eth_tx_id as i32
        )
        .instrument("get_l1_batch_statistics_for_eth_tx_id")
        .with_arg("eth_tx_id", &eth_tx_id)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| L1BatchStatistics {
            number: L1BatchNumber(row.number as u32),
            timestamp: row.timestamp as u64,
            l2_tx_count: row.l2_tx_count as u32,
            l1_tx_count: row.l1_tx_count as u32,
        })
        .collect())
    }

    async fn get_storage_l1_batch(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<StorageL1Batch>> {
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_storage_l1_batch")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await
    }

    pub async fn get_l1_batch_header(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<L1BatchHeader>> {
        let storage_l1_batch_header = sqlx::query_as!(
            StorageL1BatchHeader,
            r#"
            SELECT
                number,
                l1_tx_count,
                l2_tx_count,
                timestamp,
                l2_to_l1_messages,
                bloom,
                priority_ops_onchain_data,
                used_contract_hashes,
                bootloader_code_hash,
                default_aa_code_hash,
                protocol_version,
                system_logs,
                pubdata_input
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_header")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?;

        if let Some(storage_l1_batch_header) = storage_l1_batch_header {
            let l2_to_l1_logs = self
                .get_l2_to_l1_logs_for_batch::<UserL2ToL1Log>(number)
                .await?;
            return Ok(Some(
                storage_l1_batch_header.into_l1_batch_header_with_logs(l2_to_l1_logs),
            ));
        }

        Ok(None)
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
            i64::from(number.0)
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

    pub async fn get_storage_oracle_info(
        &mut self,
        number: L1BatchNumber,
    ) -> anyhow::Result<Option<StorageOracleInfo>> {
        let storage_oracle_info = sqlx::query_as!(
            DbStorageOracleInfo,
            r#"
            SELECT
                storage_refunds,
                pubdata_costs
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_storage_refunds")
        .report_latency()
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?;

        Ok(storage_oracle_info.and_then(DbStorageOracleInfo::into_optional_batch_oracle_info))
    }

    pub async fn set_eth_tx_id(
        &mut self,
        number_range: ops::RangeInclusive<L1BatchNumber>,
        eth_tx_id: u32,
        aggregation_type: AggregatedActionType,
    ) -> DalResult<()> {
        match aggregation_type {
            AggregatedActionType::Commit => {
                let instrumentation = Instrumented::new("set_eth_tx_id#commit")
                    .with_arg("number_range", &number_range)
                    .with_arg("eth_tx_id", &eth_tx_id);

                let query = sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_commit_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                        AND eth_commit_tx_id IS NULL
                    "#,
                    eth_tx_id as i32,
                    i64::from(number_range.start().0),
                    i64::from(number_range.end().0)
                );
                let result = instrumentation
                    .clone()
                    .with(query)
                    .execute(self.storage)
                    .await?;

                if result.rows_affected() == 0 {
                    let err = instrumentation.constraint_error(anyhow::anyhow!(
                        "Update eth_commit_tx_id that is is not null is not allowed"
                    ));
                    return Err(err);
                }
            }
            AggregatedActionType::PublishProofOnchain => {
                let instrumentation = Instrumented::new("set_eth_tx_id#prove")
                    .with_arg("number_range", &number_range)
                    .with_arg("eth_tx_id", &eth_tx_id);
                let query = sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_prove_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                        AND eth_prove_tx_id IS NULL
                    "#,
                    eth_tx_id as i32,
                    i64::from(number_range.start().0),
                    i64::from(number_range.end().0)
                );

                let result = instrumentation
                    .clone()
                    .with(query)
                    .execute(self.storage)
                    .await?;

                if result.rows_affected() == 0 {
                    let err = instrumentation.constraint_error(anyhow::anyhow!(
                        "Update eth_prove_tx_id that is is not null is not allowed"
                    ));
                    return Err(err);
                }
            }
            AggregatedActionType::Execute => {
                let instrumentation = Instrumented::new("set_eth_tx_id#execute")
                    .with_arg("number_range", &number_range)
                    .with_arg("eth_tx_id", &eth_tx_id);

                let query = sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        eth_execute_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                        AND eth_execute_tx_id IS NULL
                    "#,
                    eth_tx_id as i32,
                    i64::from(number_range.start().0),
                    i64::from(number_range.end().0)
                );

                let result = instrumentation
                    .clone()
                    .with(query)
                    .execute(self.storage)
                    .await?;

                if result.rows_affected() == 0 {
                    let err = instrumentation.constraint_error(anyhow::anyhow!(
                        "Update eth_execute_tx_id that is is not null is not allowed"
                    ));
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub async fn insert_l1_batch(
        &mut self,
        header: &L1BatchHeader,
        initial_bootloader_contents: &[(usize, U256)],
        predicted_block_gas: BlockGasCount,
        storage_refunds: &[u32],
        pubdata_costs: &[i32],
        predicted_circuits_by_type: CircuitStatistic, // predicted number of circuits for each circuit type
    ) -> DalResult<()> {
        let initial_bootloader_contents_len = initial_bootloader_contents.len();
        let instrumentation = Instrumented::new("insert_l1_batch")
            .with_arg("number", &header.number)
            .with_arg(
                "initial_bootloader_contents.len",
                &initial_bootloader_contents_len,
            );

        let priority_onchain_data: Vec<Vec<u8>> = header
            .priority_ops_onchain_data
            .iter()
            .map(|data| data.clone().into())
            .collect();
        let system_logs = header
            .system_logs
            .iter()
            .map(|log| log.0.to_bytes().to_vec())
            .collect::<Vec<Vec<u8>>>();
        let pubdata_input = header.pubdata_input.clone();
        let initial_bootloader_contents = serde_json::to_value(initial_bootloader_contents)
            .map_err(|err| instrumentation.arg_error("initial_bootloader_contents", err))?;
        let used_contract_hashes = serde_json::to_value(&header.used_contract_hashes)
            .map_err(|err| instrumentation.arg_error("header.used_contract_hashes", err))?;
        let storage_refunds: Vec<_> = storage_refunds.iter().copied().map(i64::from).collect();
        let pubdata_costs: Vec<_> = pubdata_costs.iter().copied().map(i64::from).collect();

        let query = sqlx::query!(
            r#"
            INSERT INTO
                l1_batches (
                    number,
                    l1_tx_count,
                    l2_tx_count,
                    timestamp,
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
                    pubdata_costs,
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
            i64::from(header.number.0),
            i32::from(header.l1_tx_count),
            i32::from(header.l2_tx_count),
            header.timestamp as i64,
            &header.l2_to_l1_messages,
            header.bloom.as_bytes(),
            &priority_onchain_data,
            i64::from(predicted_block_gas.commit),
            i64::from(predicted_block_gas.prove),
            i64::from(predicted_block_gas.execute),
            initial_bootloader_contents,
            used_contract_hashes,
            header.base_system_contracts_hashes.bootloader.as_bytes(),
            header.base_system_contracts_hashes.default_aa.as_bytes(),
            header.protocol_version.map(|v| v as i32),
            &system_logs,
            &storage_refunds,
            &pubdata_costs,
            pubdata_input,
            serde_json::to_value(predicted_circuits_by_type).unwrap(),
        );

        let mut transaction = self.storage.start_transaction().await?;
        instrumentation
            .with(query)
            .execute(&mut transaction)
            .await?;
        transaction.commit().await
    }

    pub async fn insert_l2_block(&mut self, l2_block_header: &L2BlockHeader) -> DalResult<()> {
        let instrumentation =
            Instrumented::new("insert_l2_block").with_arg("number", &l2_block_header.number);

        let base_fee_per_gas =
            BigDecimal::from_u64(l2_block_header.base_fee_per_gas).ok_or_else(|| {
                instrumentation.arg_error(
                    "header.base_fee_per_gas",
                    anyhow::anyhow!("doesn't fit in u64"),
                )
            })?;

        let query = sqlx::query!(
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
                    gas_limit,
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
                    NOW(),
                    NOW()
                )
            "#,
            i64::from(l2_block_header.number.0),
            l2_block_header.timestamp as i64,
            l2_block_header.hash.as_bytes(),
            i32::from(l2_block_header.l1_tx_count),
            i32::from(l2_block_header.l2_tx_count),
            l2_block_header.fee_account_address.as_bytes(),
            base_fee_per_gas,
            l2_block_header.batch_fee_input.l1_gas_price() as i64,
            l2_block_header.batch_fee_input.fair_l2_gas_price() as i64,
            l2_block_header.gas_per_pubdata_limit as i64,
            l2_block_header
                .base_system_contracts_hashes
                .bootloader
                .as_bytes(),
            l2_block_header
                .base_system_contracts_hashes
                .default_aa
                .as_bytes(),
            l2_block_header.protocol_version.map(|v| v as i32),
            i64::from(l2_block_header.virtual_blocks),
            l2_block_header.batch_fee_input.fair_pubdata_price() as i64,
            l2_block_header.gas_limit as i64,
        );

        instrumentation.with(query).execute(self.storage).await?;
        Ok(())
    }

    pub async fn get_last_sealed_l2_block_header(&mut self) -> DalResult<Option<L2BlockHeader>> {
        let header = sqlx::query_as!(
            StorageL2BlockHeader,
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
                fair_pubdata_price,
                gas_limit
            FROM
                miniblocks
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_last_sealed_l2_block_header")
        .fetch_optional(self.storage)
        .await?;

        Ok(header.map(Into::into))
    }

    pub async fn get_l2_block_header(
        &mut self,
        l2_block_number: L2BlockNumber,
    ) -> DalResult<Option<L2BlockHeader>> {
        let header = sqlx::query_as!(
            StorageL2BlockHeader,
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
                fair_pubdata_price,
                gas_limit
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(l2_block_number.0),
        )
        .instrument("get_l2_block_header")
        .with_arg("l2_block_number", &l2_block_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(header.map(Into::into))
    }

    pub async fn mark_l2_blocks_as_executed_in_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<()> {
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
        .instrument("mark_l2_blocks_as_executed_in_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
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
                rollup_last_leaf_index = $2,
                updated_at = NOW()
            WHERE
                number = $3
                AND hash IS NULL
            "#,
            tree_data.hash.as_bytes(),
            tree_data.rollup_last_leaf_index as i64,
            i64::from(number.0),
        )
        .instrument("save_batch_tree_data")
        .with_arg("number", &number)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!("L1 batch #{number}: tree data wasn't updated as it's already present");

            // Batch was already processed. Verify that the existing tree data matches.
            let existing_tree_data = self.get_l1_batch_tree_data(number).await?;
            anyhow::ensure!(
                existing_tree_data.as_ref() == Some(tree_data),
                "Root hash verification failed. Tree data for L1 batch #{number} does not match the expected value \
                 (expected: {tree_data:?}, existing: {existing_tree_data:?})",
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
                state_diff_hash = $10,
                aggregation_root = $11,
                local_root = $12,
                updated_at = NOW()
            WHERE
                number = $13
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
            commitment_artifacts.state_diff_hash.as_bytes(),
            commitment_artifacts.aggregation_root.as_bytes(),
            commitment_artifacts.local_root.as_bytes(),
            i64::from(number.0),
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
                i64::from(number.0),
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
            i64::from(number.0),
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
    ) -> DalResult<Option<L1BatchWithMetadata>> {
        // We can get 0 batch for the first transaction
        let batch = sqlx::query_as!(
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
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
        // genesis batch is first generated without commitment, we should wait for the tree to set it.
        if batch.commitment.is_none() {
            return Ok(None);
        }

        self.map_storage_l1_batch(batch).await
    }

    /// Returns the number of the last L1 batch for which an Ethereum commit tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_committed_on_eth(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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
        .instrument("get_number_of_last_l1_batch_committed_on_eth")
        .fetch_optional(self.storage)
        .await?
        .map(|row| L1BatchNumber(row.number as u32)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum prove tx exists in the database.
    pub async fn get_last_l1_batch_with_prove_tx(&mut self) -> DalResult<L1BatchNumber> {
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
        .instrument("get_last_l1_batch_with_prove_tx")
        .fetch_one(self.storage)
        .await?;

        Ok(L1BatchNumber(row.number as u32))
    }

    pub async fn get_eth_commit_tx_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<u64>> {
        let row = sqlx::query!(
            r#"
            SELECT
                eth_commit_tx_id
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_eth_commit_tx_id")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.and_then(|row| row.eth_commit_tx_id.map(|n| n as u64)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum prove tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_proven_on_eth(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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
        .instrument("get_number_of_last_l1_batch_proven_on_eth")
        .fetch_optional(self.storage)
        .await?
        .map(|record| L1BatchNumber(record.number as u32)))
    }

    /// Returns the number of the last L1 batch for which an Ethereum execute tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_executed_on_eth(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
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
        .instrument("get_number_of_last_l1_batch_executed_on_eth")
        .fetch_optional(self.storage)
        .await?
        .map(|row| L1BatchNumber(row.number as u32)))
    }

    /// This method returns batches that are confirmed on L1. That is, it doesn't wait for the proofs to be generated.
    ///
    /// # Params:
    /// * `commited_tx_confirmed`: whether to look for ready proofs only for txs for which
    ///   respective commit transactions have been confirmed by the network.
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
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
        let mut l1_batches_with_metadata = Vec::with_capacity(raw_batches.len());
        for raw_batch in raw_batches {
            let batch = self
                .map_storage_l1_batch(raw_batch)
                .await
                .context("map_storage_l1_batch()")?
                .context("Batch should be complete")?;
            l1_batches_with_metadata.push(batch);
        }
        Ok(l1_batches_with_metadata)
    }

    /// This method returns batches that are committed on L1 and witness jobs for them are skipped.
    pub async fn get_skipped_for_proof_l1_batches(
        &mut self,
        limit: usize,
    ) -> anyhow::Result<Vec<L1BatchWithMetadata>> {
        let last_proved_batch_number = self
            .get_last_l1_batch_with_prove_tx()
            .await
            .context("get_last_l1_batch_with_prove_tx()")?;
        // Witness jobs can be processed out of order, so `WHERE l1_batches.number - row_number = $1`
        // is used to avoid having gaps in the list of batches to send dummy proofs for.
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
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
                LEFT JOIN data_availability ON data_availability.l1_batch_number = inn.number
            WHERE
                number - ROW_NUMBER = $1
            "#,
            last_proved_batch_number.0 as i32,
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
                        pubdata_input,
                        aggregation_root,
                        local_root,
                        state_diff_hash,
                        data_availability.inclusion_data
                    FROM
                        l1_batches
                        LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                        LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
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

    pub async fn get_batch_first_priority_op_id(
        &mut self,
        batch_number: L1BatchNumber,
    ) -> DalResult<Option<usize>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(priority_op_id) AS "id?"
            FROM
                transactions
            WHERE
                l1_batch_number = $1
                AND is_priority = TRUE
            "#,
            i64::from(batch_number.0),
        )
        .instrument("get_batch_first_priority_op_id")
        .with_arg("batch_number", &batch_number)
        .fetch_one(self.storage)
        .await?;

        Ok(row.id.map(|id| id as usize))
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

        Ok(if let Some(max_ready_to_send_batch) = row.max {
            // If we found at least one ready to execute batch then we can simply return all batches between
            // the expected started point and the max ready to send batch because we send them to the L1 sequentially.
            assert!(max_ready_to_send_batch >= expected_started_point);
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
                    pubdata_input,
                    aggregation_root,
                    local_root,
                    state_diff_hash,
                    data_availability.inclusion_data
                FROM
                    l1_batches
                    LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                    LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
                WHERE
                    number BETWEEN $1 AND $2
                ORDER BY
                    number
                LIMIT
                    $3
                "#,
                expected_started_point as i32,
                max_ready_to_send_batch,
                limit as i32,
            )
            .instrument("get_ready_for_execute_l1_batches")
            .with_arg(
                "numbers",
                &(expected_started_point..=max_ready_to_send_batch),
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                JOIN protocol_versions ON protocol_versions.id = l1_batches.protocol_version
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
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

    /// When `with_da_inclusion_info` is true, only batches for which custom DA inclusion
    /// information has already been provided will be included
    pub async fn get_ready_for_commit_l1_batches(
        &mut self,
        limit: usize,
        bootloader_hash: H256,
        default_aa_hash: H256,
        protocol_version_id: ProtocolVersionId,

        with_da_inclusion_info: bool,
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
                pubdata_input,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data
            FROM
                l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
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
                AND (
                    data_availability.inclusion_data IS NOT NULL
                    OR $4 IS FALSE
                )
            ORDER BY
                number
            LIMIT
                $5
            "#,
            bootloader_hash.as_bytes(),
            default_aa_hash.as_bytes(),
            protocol_version_id as i32,
            with_da_inclusion_info,
            limit as i64,
        )
        .instrument("get_ready_for_commit_l1_batches")
        .with_arg("limit", &limit)
        .with_arg("bootloader_hash", &bootloader_hash)
        .with_arg("default_aa_hash", &default_aa_hash)
        .with_arg("protocol_version_id", &protocol_version_id)
        .with_arg("with_da_inclusion_info", &with_da_inclusion_info)
        .fetch_all(self.storage)
        .await?;

        self.map_l1_batches(raw_batches)
            .await
            .context("map_l1_batches()")
    }

    pub async fn get_l1_batch_state_root(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<H256>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                hash
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_state_root")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        .and_then(|row| row.hash)
        .map(|hash| H256::from_slice(&hash)))
    }

    pub async fn get_l1_batch_state_root_and_timestamp(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<(H256, u64)>> {
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
            i64::from(number.0)
        )
        .instrument("get_l1_batch_state_root_and_timestamp")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
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
    ) -> DalResult<Option<L1BatchWithMetadata>> {
        let Some(l1_batch) = self.get_storage_l1_batch(number).await? else {
            return Ok(None);
        };
        self.map_storage_l1_batch(l1_batch).await
    }

    /// Returns the header and optional metadata for an L1 batch with the specified number. If a batch exists
    /// but does not have all metadata, it's possible to inspect which metadata is missing.
    pub async fn get_optional_l1_batch_metadata(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<L1BatchWithOptionalMetadata>> {
        let Some(l1_batch) = self.get_storage_l1_batch(number).await? else {
            return Ok(None);
        };

        let l2_to_l1_logs = self
            .get_l2_to_l1_logs_for_batch::<UserL2ToL1Log>(number)
            .await?;
        Ok(Some(L1BatchWithOptionalMetadata {
            header: l1_batch
                .clone()
                .into_l1_batch_header_with_logs(l2_to_l1_logs),
            metadata: l1_batch.try_into(),
        }))
    }

    pub async fn get_l1_batch_tree_data(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<L1BatchTreeData>> {
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
            i64::from(number.0)
        )
        .instrument("get_l1_batch_tree_data")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.and_then(|row| {
            Some(L1BatchTreeData {
                hash: H256::from_slice(&row.hash?),
                rollup_last_leaf_index: row.rollup_last_leaf_index? as u64,
            })
        }))
    }

    async fn map_storage_l1_batch(
        &mut self,
        storage_batch: StorageL1Batch,
    ) -> DalResult<Option<L1BatchWithMetadata>> {
        let unsorted_factory_deps = self
            .get_l1_batch_factory_deps(L1BatchNumber(storage_batch.number as u32))
            .await?;

        let l2_to_l1_logs = self
            .get_l2_to_l1_logs_for_batch::<UserL2ToL1Log>(L1BatchNumber(
                storage_batch.number as u32,
            ))
            .await?;

        let Ok(metadata) = storage_batch.clone().try_into() else {
            return Ok(None);
        };

        let header: L1BatchHeader = storage_batch.into_l1_batch_header_with_logs(l2_to_l1_logs);

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
    ) -> DalResult<HashMap<H256, Vec<u8>>> {
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
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l1_batch_factory_deps")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
        .collect())
    }

    pub async fn delete_initial_writes(
        &mut self,
        last_batch_to_keep: L1BatchNumber,
    ) -> DalResult<()> {
        self.delete_initial_writes_inner(Some(last_batch_to_keep))
            .await
    }

    async fn delete_initial_writes_inner(
        &mut self,
        last_batch_to_keep: Option<L1BatchNumber>,
    ) -> DalResult<()> {
        let l1_batch_number = last_batch_to_keep.map_or(-1, |number| i64::from(number.0));
        sqlx::query!(
            r#"
            DELETE FROM initial_writes
            WHERE
                l1_batch_number > $1
            "#,
            l1_batch_number
        )
        .instrument("delete_initial_writes")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Deletes all L1 batches from the storage so that the specified batch number is the last one left.
    pub async fn delete_l1_batches(&mut self, last_batch_to_keep: L1BatchNumber) -> DalResult<()> {
        self.delete_l1_batches_inner(Some(last_batch_to_keep)).await
    }

    async fn delete_l1_batches_inner(
        &mut self,
        last_batch_to_keep: Option<L1BatchNumber>,
    ) -> DalResult<()> {
        let l1_batch_number = last_batch_to_keep.map_or(-1, |number| i64::from(number.0));
        sqlx::query!(
            r#"
            DELETE FROM l1_batches
            WHERE
                number > $1
            "#,
            l1_batch_number
        )
        .instrument("delete_l1_batches")
        .with_arg("l1_batch_number", &l1_batch_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    /// Deletes all L2 blocks from the storage so that the specified L2 block number is the last one left.
    pub async fn delete_l2_blocks(
        &mut self,
        last_l2_block_to_keep: L2BlockNumber,
    ) -> DalResult<()> {
        self.delete_l2_blocks_inner(Some(last_l2_block_to_keep))
            .await
    }

    async fn delete_l2_blocks_inner(
        &mut self,
        last_l2_block_to_keep: Option<L2BlockNumber>,
    ) -> DalResult<()> {
        let block_number = last_l2_block_to_keep.map_or(-1, |number| i64::from(number.0));
        sqlx::query!(
            r#"
            DELETE FROM miniblocks
            WHERE
                number > $1
            "#,
            block_number
        )
        .instrument("delete_l2_blocks")
        .with_arg("block_number", &block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    async fn delete_logs_inner(&mut self) -> DalResult<()> {
        sqlx::query!(
            r#"
            DELETE FROM storage_logs
            "#,
        )
        .instrument("delete_logs")
        .execute(self.storage)
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
        #[derive(Debug)]
        struct SumRow {
            sum: BigDecimal,
        }

        let start = i64::from(number_range.start().0);
        let end = i64::from(number_range.end().0);
        let query = match_query_as!(
            SumRow,
            [
                "SELECT COALESCE(SUM(", _, r#"), 0) AS "sum!" FROM l1_batches WHERE number BETWEEN $1 AND $2"#
            ],
            match (op_type) {
                AggregatedActionType::Commit => ("predicted_commit_gas_cost"; start, end),
                AggregatedActionType::PublishProofOnchain => ("predicted_prove_gas_cost"; start, end),
                AggregatedActionType::Execute => ("predicted_execute_gas_cost"; start, end),
            }
        );

        query
            .fetch_one(self.storage.conn())
            .await?
            .sum
            .to_u32()
            .context("Sum of predicted gas costs should fit into u32")
    }

    pub async fn get_l2_block_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<(L2BlockNumber, L2BlockNumber)>> {
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
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l2_block_range_of_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_one(self.storage)
        .await?;

        let Some(min) = row.min else { return Ok(None) };
        let Some(max) = row.max else { return Ok(None) };
        Ok(Some((L2BlockNumber(min as u32), L2BlockNumber(max as u32))))
    }

    /// Returns `true` if there exists a non-sealed batch (i.e. there is one+ stored L2 block that isn't assigned
    /// to any batch yet).
    pub async fn pending_batch_exists(&mut self) -> DalResult<bool> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(miniblocks.number) FROM miniblocks WHERE l1_batch_number IS NULL"
        )
        .instrument("pending_batch_exists")
        .fetch_one(self.storage)
        .await?
        .unwrap_or(0);

        Ok(count != 0)
    }

    // methods used for measuring Eth tx stage transition latencies
    // and emitting metrics base on these measured data
    pub async fn oldest_uncommitted_batch_timestamp(&mut self) -> DalResult<Option<u64>> {
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
        .instrument("oldest_uncommitted_batch_timestamp")
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn oldest_unproved_batch_timestamp(&mut self) -> DalResult<Option<u64>> {
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
        .instrument("oldest_unproved_batch_timestamp")
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn oldest_unexecuted_batch_timestamp(&mut self) -> DalResult<Option<u64>> {
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
        .instrument("oldest_unexecuted_batch_timestamp")
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.timestamp as u64))
    }

    pub async fn get_batch_protocol_version_id(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<ProtocolVersionId>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                protocol_version
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .try_map(|row| row.protocol_version.map(parse_protocol_version).transpose())
        .instrument("get_batch_protocol_version_id")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    pub async fn set_protocol_version_for_pending_l2_blocks(
        &mut self,
        id: ProtocolVersionId,
    ) -> DalResult<()> {
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
        .instrument("set_protocol_version_for_pending_l2_blocks")
        .with_arg("id", &id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_fee_address_for_l2_block(
        &mut self,
        number: L2BlockNumber,
    ) -> DalResult<Option<Address>> {
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
        .instrument("get_fee_address_for_l2_block")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(Address::from_slice(&row.fee_account_address)))
    }

    pub async fn get_first_l1_batch_number_for_version(
        &mut self,
        protocol_version: ProtocolVersionId,
    ) -> DalResult<Option<L1BatchNumber>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                MIN(number) AS "min?"
            FROM
                l1_batches
            WHERE
                protocol_version = $1
            "#,
            protocol_version as i32
        )
        .instrument("get_first_l1_batch_number_for_version")
        .with_arg("protocol_version", &protocol_version)
        .fetch_optional(self.storage)
        .await?
        .and_then(|row| row.min)
        .map(|min| L1BatchNumber(min as u32)))
    }

    pub async fn reset_protocol_version_for_l1_batches(
        &mut self,
        l1_batch_range: ops::RangeInclusive<L1BatchNumber>,
        protocol_version: ProtocolVersionId,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                protocol_version = $1
            WHERE
                number BETWEEN $2 AND $3
            "#,
            protocol_version as i32,
            i64::from(l1_batch_range.start().0),
            i64::from(l1_batch_range.end().0),
        )
        .instrument("reset_protocol_version_for_l1_batches")
        .with_arg("l1_batch_range", &l1_batch_range)
        .with_arg("protocol_version", &protocol_version)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn reset_protocol_version_for_l2_blocks(
        &mut self,
        l2_block_range: ops::RangeInclusive<L2BlockNumber>,
        protocol_version: ProtocolVersionId,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                protocol_version = $1
            WHERE
                number BETWEEN $2 AND $3
            "#,
            protocol_version as i32,
            i64::from(l2_block_range.start().0),
            i64::from(l2_block_range.end().0),
        )
        .instrument("reset_protocol_version_for_l2_blocks")
        .with_arg("l2_block_range", &l2_block_range)
        .with_arg("protocol_version", &protocol_version)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn set_tree_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
        tree_writes: Vec<TreeWrite>,
    ) -> DalResult<()> {
        let instrumentation =
            Instrumented::new("set_tree_writes").with_arg("l1_batch_number", &l1_batch_number);
        let tree_writes = bincode::serialize(&tree_writes)
            .map_err(|err| instrumentation.arg_error("tree_writes", err))?;

        let query = sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                tree_writes = $1
            WHERE
                number = $2
            "#,
            &tree_writes,
            i64::from(l1_batch_number.0),
        );

        instrumentation.with(query).execute(self.storage).await?;

        Ok(())
    }

    pub async fn get_tree_writes(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<Vec<TreeWrite>>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                tree_writes
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0),
        )
        .try_map(|row| {
            row.tree_writes
                .map(|data| bincode::deserialize(&data).decode_column("tree_writes"))
                .transpose()
        })
        .instrument("get_tree_writes")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .flatten())
    }

    pub async fn check_tree_writes_presence(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<bool> {
        Ok(sqlx::query!(
            r#"
            SELECT
                (tree_writes IS NOT NULL) AS "tree_writes_are_present!"
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0),
        )
        .instrument("check_tree_writes_presence")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.tree_writes_are_present)
        .unwrap_or(false))
    }

    pub(crate) async fn get_l2_to_l1_logs_for_batch<L>(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Vec<L>>
    where
        L: From<StorageL2ToL1Log>,
    {
        let results = sqlx::query_as!(
            StorageL2ToL1Log,
            r#"
            SELECT
                miniblock_number,
                log_index_in_miniblock,
                log_index_in_tx,
                tx_hash,
                l1_batch_number,
                shard_id,
                is_service,
                tx_index_in_miniblock,
                tx_index_in_l1_batch,
                sender,
                key,
                value
            FROM
                l2_to_l1_logs
                JOIN miniblocks ON l2_to_l1_logs.miniblock_number = miniblocks.number
            WHERE
                l1_batch_number = $1
            ORDER BY
                miniblock_number,
                log_index_in_miniblock
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l2_to_l1_logs_by_number")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?;

        Ok(results.into_iter().map(L::from).collect())
    }
}

/// These methods should only be used for tests.
impl BlocksDal<'_, '_> {
    // The actual l1 batch hash is only set by the metadata calculator.
    pub async fn set_l1_batch_hash(
        &mut self,
        batch_number: L1BatchNumber,
        hash: H256,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE l1_batches
            SET
                hash = $1
            WHERE
                number = $2
            "#,
            hash.as_bytes(),
            i64::from(batch_number.0)
        )
        .instrument("set_l1_batch_hash")
        .with_arg("batch_number", &batch_number)
        .with_arg("hash", &hash)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn insert_mock_l1_batch(&mut self, header: &L1BatchHeader) -> DalResult<()> {
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

    /// Deletes all L2 blocks and L1 batches, including the genesis ones. Should only be used in tests.
    pub async fn delete_genesis(&mut self) -> DalResult<()> {
        self.delete_l2_blocks_inner(None).await?;
        self.delete_l1_batches_inner(None).await?;
        self.delete_initial_writes_inner(None).await?;
        self.delete_logs_inner().await?;
        Ok(())
    }

    /// Obtains a protocol version projected to be applied for the next L2 block. This is either the version used by the last
    /// sealed L2 block, or (if there are no L2 blocks), one referenced in the snapshot recovery record.
    pub async fn pending_protocol_version(&mut self) -> anyhow::Result<ProtocolVersionId> {
        static WARNED_ABOUT_NO_VERSION: AtomicBool = AtomicBool::new(false);

        let last_l2_block = self
            .storage
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await?;
        if let Some(last_l2_block) = last_l2_block {
            return Ok(last_l2_block.protocol_version.unwrap_or_else(|| {
                // Protocol version should be set for the most recent L2 block even in cases it's not filled
                // for old L2 blocks, hence the warning. We don't want to rely on this assumption, so we treat
                // the lack of it as in other similar places, replacing with the default value.
                if !WARNED_ABOUT_NO_VERSION.fetch_or(true, Ordering::Relaxed) {
                    tracing::warn!(
                        "Protocol version not set for recent L2 block: {last_l2_block:?}"
                    );
                }
                ProtocolVersionId::last_potentially_undefined()
            }));
        }
        // No L2 blocks in the storage; use snapshot recovery information.
        let snapshot_recovery = self
            .storage
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await?
            .context("storage contains neither L2 blocks, nor snapshot recovery info")?;
        Ok(snapshot_recovery.protocol_version)
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{tx::IncludedTxLocation, Address, ProtocolVersion, ProtocolVersionId};

    use super::*;
    use crate::{
        tests::{create_l1_batch_header, create_l2_block_header, create_l2_to_l1_log},
        ConnectionPool, Core, CoreDal,
    };

    async fn save_mock_eth_tx(action_type: AggregatedActionType, conn: &mut Connection<'_, Core>) {
        conn.eth_sender_dal()
            .save_eth_tx(1, vec![], action_type, Address::default(), 1, None, None)
            .await
            .unwrap();
    }

    fn mock_l1_batch_header() -> L1BatchHeader {
        let mut header = create_l1_batch_header(1);
        header.l1_tx_count = 3;
        header.l2_tx_count = 5;
        header.l2_to_l1_logs.push(create_l2_to_l1_log(0, 0));
        header.l2_to_l1_messages.push(vec![22; 22]);
        header.l2_to_l1_messages.push(vec![33; 33]);

        header
    }

    async fn insert_mock_l1_batch_header(conn: &mut Connection<'_, Core>, header: &L1BatchHeader) {
        conn.blocks_dal()
            .insert_mock_l1_batch(header)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn set_tx_id_works_correctly() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let header = mock_l1_batch_header();

        insert_mock_l1_batch_header(&mut conn, &header).await;

        save_mock_eth_tx(AggregatedActionType::Commit, &mut conn).await;
        save_mock_eth_tx(AggregatedActionType::PublishProofOnchain, &mut conn).await;
        save_mock_eth_tx(AggregatedActionType::Execute, &mut conn).await;

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::Commit,
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::Commit,
            )
            .await
            .is_err());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::PublishProofOnchain,
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::PublishProofOnchain,
            )
            .await
            .is_err());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::Execute,
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::Execute,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn loading_l1_batch_header() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let header = mock_l1_batch_header();

        insert_mock_l1_batch_header(&mut conn, &header).await;

        let l2_block_header = create_l2_block_header(1);

        conn.blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();

        let first_location = IncludedTxLocation {
            tx_hash: H256([1; 32]),
            tx_index_in_l2_block: 0,
            tx_initiator_address: Address::repeat_byte(2),
        };
        let first_logs = [create_l2_to_l1_log(0, 0)];

        let all_logs = vec![(first_location, first_logs.iter().collect())];
        conn.events_dal()
            .save_user_l2_to_l1_logs(L2BlockNumber(1), &all_logs)
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
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
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
}
