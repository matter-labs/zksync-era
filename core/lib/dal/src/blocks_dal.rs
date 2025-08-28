use std::{
    collections::HashMap,
    convert::{Into, TryInto},
    ops,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Context as _;
use bigdecimal::{BigDecimal, FromPrimitive};
use sqlx::types::chrono::{DateTime, Utc};
use zksync_db_connection::{
    connection::Connection,
    error::{DalResult, SqlxContext},
    instrument::{InstrumentExt, Instrumented},
    interpolate_query, match_query_as,
};
use zksync_types::{
    aggregated_operations::{
        AggregatedActionType, L1BatchAggregatedActionType, L2BlockAggregatedActionType,
    },
    block::{
        CommonBlockStatistics, CommonL1BatchHeader, L1BatchHeader, L1BatchTreeData, L2BlockHeader,
        StorageOracleInfo, UnsealedL1BatchHeader,
    },
    commitment::{L1BatchCommitmentArtifacts, L1BatchWithMetadata, PubdataParams},
    l2_to_l1_log::{BatchAndChainMerklePath, UserL2ToL1Log},
    writes::TreeWrite,
    Address, Bloom, L1BatchNumber, L2BlockNumber, ProtocolVersionId, SLChainId, H256, U256,
};
use zksync_vm_interface::CircuitStatistic;

pub use crate::models::storage_block::{L1BatchMetadataError, L1BatchWithOptionalMetadata};
use crate::{
    models::{
        parse_protocol_version,
        storage_block::{
            CommonStorageL1BatchHeader, StorageL1Batch, StorageL1BatchHeader, StorageL2BlockHeader,
            StoragePubdataParams, UnsealedStorageL1Batch,
        },
        storage_eth_tx::L2BlockWithEthTx,
        storage_event::StorageL2ToL1Log,
        storage_oracle_info::DbStorageOracleInfo,
    },
    Core, CoreDal,
};

#[derive(Debug)]
pub struct BlocksDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

#[derive(Debug, Clone, Default)]
pub struct TxForPrecommit {
    pub l1_batch_number: Option<L1BatchNumber>,
    pub l2block_number: L2BlockNumber,
    pub timestamp: i64,
    pub tx_hash: H256,
    pub is_success: bool,
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
            WHERE
                is_sealed
            "#
        )
        .instrument("is_genesis_needed")
        .fetch_one(self.storage)
        .await?
        .count;
        Ok(count == 0)
    }

    /// Returns the number of the last sealed L1 batch present in the DB, or `None` if there are no L1 batches.
    pub async fn get_sealed_l1_batch_number(&mut self) -> DalResult<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "number"
            FROM
                l1_batches
            WHERE
                is_sealed
            "#
        )
        .instrument("get_sealed_l1_batch_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    /// Returns the number and the timestamp of the last sealed L1 batch present in the DB, or `None` if there are no L1 batches.
    pub async fn get_sealed_l1_batch_number_and_timestamp(
        &mut self,
    ) -> DalResult<Option<(L1BatchNumber, u64)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                number,
                timestamp
            FROM
                l1_batches
            WHERE
                is_sealed
            ORDER BY number DESC
            LIMIT 1
            "#
        )
        .instrument("get_sealed_l1_batch_number_and_timestamp")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| (L1BatchNumber(row.number as u32), row.timestamp as u64)))
    }

    /// Returns latest L1 batch's header (could be unsealed). The header contains fields that are
    /// common for both unsealed and sealed batches. Returns `None` if there are no L1 batches.
    pub async fn get_latest_l1_batch_header(&mut self) -> DalResult<Option<CommonL1BatchHeader>> {
        let Some(header) = sqlx::query_as!(
            CommonStorageL1BatchHeader,
            r#"
            SELECT
                number,
                is_sealed,
                timestamp,
                protocol_version,
                fee_address,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_latest_l1_batch_header")
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(header.into()))
    }

    /// Returns common L1 batch's header (could be unsealed) by L1 batch number. The header contains fields that are
    /// common for both unsealed and sealed batches. Returns `None` if there is no L1 batch with the provided number.
    pub async fn get_common_l1_batch_header(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<CommonL1BatchHeader>> {
        let Some(header) = sqlx::query_as!(
            CommonStorageL1BatchHeader,
            r#"
            SELECT
                number,
                is_sealed,
                timestamp,
                protocol_version,
                fee_address,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            WHERE number = $1
            "#,
            i64::from(number.0),
        )
        .instrument("get_common_l1_batch_header")
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(header.into()))
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
            WHERE
                is_sealed
            "#
        )
        .instrument("get_earliest_l1_batch_number")
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    /// Returns the first validium batch with a number higher than the provided one.
    /// The query might look inefficient, but it is slow only when there is a large range of
    /// "Rollup" batches, i.e. during the first lookup of Rollup->Validium transition, otherwise it
    /// is only 2 index scans.
    pub async fn get_first_validium_l1_batch_number(
        &mut self,
        last_processed_batch: L1BatchNumber,
    ) -> DalResult<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS "number"
            FROM
                l1_batches
            WHERE
                is_sealed
                AND number > $1
                AND (
                    SELECT pubdata_type
                    FROM miniblocks
                    WHERE l1_batch_number = l1_batches.number
                    ORDER BY miniblocks.number
                    LIMIT 1
                ) != 'Rollup'
            "#,
            last_processed_batch.0 as i32
        )
        .instrument("get_earliest_l1_batch_number")
        .with_arg("last_processed_batch", &last_processed_batch)
        .report_latency()
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L1BatchNumber(num as u32)))
    }

    pub async fn get_earliest_l2_block_number(&mut self) -> DalResult<Option<L2BlockNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(number) AS "number"
            FROM
                miniblocks
            "#
        )
        .instrument("get_earliest_l2_block_number")
        .fetch_one(self.storage)
        .await?;

        Ok(row.number.map(|num| L2BlockNumber(num as u32)))
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

    pub async fn get_last_miniblock_with_precommit(&mut self) -> DalResult<Option<L2BlockNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                miniblocks
            WHERE
                eth_precommit_tx_id IS NOT NULL
            ORDER BY
                number DESC
            LIMIT
                1
            "#
        )
        .instrument("get_last_miniblock_with_precommit")
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| L2BlockNumber(row.number as u32)))
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

    pub async fn get_l2_blocks_statistics_for_eth_tx_id(
        &mut self,
        eth_tx_id: u32,
    ) -> DalResult<Vec<CommonBlockStatistics>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number,
                l1_tx_count,
                l2_tx_count,
                timestamp
            FROM
                miniblocks
            WHERE
                eth_precommit_tx_id = $1
            "#,
            eth_tx_id as i32
        )
        .instrument("get_l1_batch_statistics_for_eth_tx_id")
        .with_arg("eth_tx_id", &eth_tx_id)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| CommonBlockStatistics {
            number: row.number as u32,
            timestamp: row.timestamp as u64,
            l2_tx_count: row.l2_tx_count as u32,
            l1_tx_count: row.l1_tx_count as u32,
        })
        .collect())
    }

    pub async fn get_l1_batches_statistics_for_eth_tx_id(
        &mut self,
        eth_tx_id: u32,
    ) -> DalResult<Vec<CommonBlockStatistics>> {
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
        .map(|row| CommonBlockStatistics {
            number: row.number as u32,
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
                evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                system_logs,
                compressed_state_diffs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            LEFT JOIN
                data_availability
                ON data_availability.l1_batch_number = l1_batches.number
            WHERE
                is_sealed
                AND number = $1
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
                evm_emulator_code_hash,
                protocol_version,
                system_logs,
                pubdata_input,
                fee_address,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            WHERE
                is_sealed
                AND number = $1
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
                is_sealed
                AND number = $1
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
                is_sealed
                AND number = $1
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

    /// Get Last L2 blocks in batch with their rolling txs hash and precommit eth transactions ids,
    /// grouped by batches.
    pub async fn get_last_l2_block_rolling_txs_hashes_by_batches(
        &mut self,
    ) -> DalResult<HashMap<L1BatchNumber, Vec<L2BlockWithEthTx>>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                l1_batches.number AS l1_batch_number,
                miniblocks.eth_precommit_tx_id,
                miniblocks.rolling_txs_hash AS "rolling_txs_hash!",
                miniblocks.number AS miniblock_number
            FROM
                l1_batches
            JOIN LATERAL (
                SELECT
                    eth_precommit_tx_id,
                    rolling_txs_hash,
                    number
                FROM miniblocks
                LEFT JOIN
                    eth_txs_history
                    ON eth_txs_history.eth_tx_id = miniblocks.eth_precommit_tx_id
                WHERE
                    l1_batch_number = l1_batches.number
                    AND (
                        eth_txs_history.finality_status != 'pending'
                        OR eth_precommit_tx_id IS NULL
                    )
                ORDER BY number DESC
                LIMIT 2
            ) miniblocks ON TRUE
            WHERE
                l1_batches.number > 0
                AND l1_batches.is_sealed
                AND l1_batches.eth_commit_tx_id IS NULL
                AND l1_batches.final_precommit_eth_tx_id IS NULL
                AND miniblocks.rolling_txs_hash IS NOT NULL
            ORDER BY l1_batches.number, miniblocks.number DESC;
            "#
        )
        .instrument("get_last_l2_block_rolling_txs_hashes_by_batches")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        // Group by l1_batch_number
        let mut l2_blocks_by_batch: HashMap<_, Vec<_>> = HashMap::new();
        for row in rows {
            l2_blocks_by_batch
                .entry(L1BatchNumber(row.l1_batch_number as u32))
                .or_default()
                .push(L2BlockWithEthTx {
                    l1_batch_number: L1BatchNumber(row.l1_batch_number as u32),
                    l2_block_number: L2BlockNumber(row.miniblock_number as u32),
                    rolling_txs_hash: H256::from_slice(&row.rolling_txs_hash),
                    precommit_eth_tx_id: row.eth_precommit_tx_id,
                });
        }

        Ok(l2_blocks_by_batch)
    }

    pub async fn set_eth_tx_id_for_l2_blocks(
        &mut self,
        number_range: ops::RangeInclusive<L2BlockNumber>,
        eth_tx_id: u32,
        aggregation_type: L2BlockAggregatedActionType,
    ) -> DalResult<()> {
        match aggregation_type {
            L2BlockAggregatedActionType::Precommit => {
                let instrumentation = Instrumented::new("set_eth_tx_id#precommit")
                    .with_arg("number_range", &number_range)
                    .with_arg("eth_tx_id", &eth_tx_id);

                let query = sqlx::query!(
                    r#"
                    UPDATE miniblocks
                    SET
                        eth_precommit_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                        AND eth_precommit_tx_id IS NULL
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

                if result.rows_affected() as u32
                    != number_range.end().0 - number_range.start().0 + 1
                {
                    let err = instrumentation.constraint_error(anyhow::anyhow!(
                        "Update eth_precommit_tx_id that is is not null is not allowed"
                    ));
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    pub async fn set_eth_tx_id_for_l1_batches(
        &mut self,
        number_range: ops::RangeInclusive<L1BatchNumber>,
        eth_tx_id: u32,
        aggregation_type: AggregatedActionType,
    ) -> DalResult<()> {
        match aggregation_type {
            AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit) => {
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
            AggregatedActionType::L1Batch(L1BatchAggregatedActionType::PublishProofOnchain) => {
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
            AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Execute) => {
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
            AggregatedActionType::L2Block(L2BlockAggregatedActionType::Precommit) => {
                let instrumentation = Instrumented::new("set_eth_tx_id#precommit")
                    .with_arg("number_range", &number_range)
                    .with_arg("eth_tx_id", &eth_tx_id);

                let query = sqlx::query!(
                    r#"
                    UPDATE l1_batches
                    SET
                        final_precommit_eth_tx_id = $1,
                        updated_at = NOW()
                    WHERE
                        number BETWEEN $2 AND $3
                        AND final_precommit_eth_tx_id IS NULL
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
                        "Update final_precommit_eth_tx_id that is is not null is not allowed"
                    ));
                    return Err(err);
                }
            }
        }
        Ok(())
    }

    /// Inserts an unsealed L1 batch with some basic information (i.e. runtime related data is either
    /// null or set to default value for the corresponding type).
    pub async fn insert_l1_batch(
        &mut self,
        unsealed_batch_header: UnsealedL1BatchHeader,
    ) -> DalResult<()> {
        Self::insert_l1_batch_inner(unsealed_batch_header, self.storage).await
    }

    async fn insert_l1_batch_inner(
        unsealed_batch_header: UnsealedL1BatchHeader,
        conn: &mut Connection<'_, Core>,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
            l1_batches (
                number,
                timestamp,
                protocol_version,
                fee_address,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit,
                l1_tx_count,
                l2_tx_count,
                bloom,
                priority_ops_onchain_data,
                initial_bootloader_heap_content,
                used_contract_hashes,
                created_at,
                updated_at,
                is_sealed
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
                0,
                0,
                ''::bytea,
                '{}'::bytea [],
                '{}'::jsonb,
                '{}'::jsonb,
                NOW(),
                NOW(),
                FALSE
            )
            "#,
            i64::from(unsealed_batch_header.number.0),
            unsealed_batch_header.timestamp as i64,
            unsealed_batch_header.protocol_version.map(|v| v as i32),
            unsealed_batch_header.fee_address.as_bytes(),
            unsealed_batch_header.fee_input.l1_gas_price() as i64,
            unsealed_batch_header.fee_input.fair_l2_gas_price() as i64,
            unsealed_batch_header.fee_input.fair_pubdata_price() as i64,
            unsealed_batch_header.pubdata_limit.map(|l| l as i64),
        )
        .instrument("insert_l1_batch")
        .with_arg("number", &unsealed_batch_header.number)
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn ensure_unsealed_l1_batch_exists(
        &mut self,
        unsealed_batch: UnsealedL1BatchHeader,
    ) -> anyhow::Result<()> {
        let mut transaction = self.storage.start_transaction().await?;
        let unsealed_batch_fetched = Self::get_unsealed_l1_batch_inner(&mut transaction).await?;

        match unsealed_batch_fetched {
            None => {
                tracing::info!(
                    "Unsealed batch #{} could not be found; inserting",
                    unsealed_batch.number
                );
                Self::insert_l1_batch_inner(unsealed_batch, &mut transaction).await?;
            }
            Some(unsealed_batch_fetched) => {
                if unsealed_batch_fetched.number != unsealed_batch.number {
                    anyhow::bail!(
                        "fetched unsealed L1 batch #{} does not conform to expected L1 batch #{}",
                        unsealed_batch_fetched.number,
                        unsealed_batch.number
                    )
                }
            }
        }

        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_ready_for_precommit_txs(
        &mut self,
        l1_batch: L1BatchNumber,
    ) -> DalResult<Vec<TxForPrecommit>> {
        let mut tx = self.storage.start_transaction().await?;
        // Miniblocks belongs to the non sealed batches don't have batch number,
        // so for the pending batch we use NULl
        let txs = sqlx::query!(
            r#"
            SELECT
                miniblocks.l1_batch_number,
                transactions.hash, transactions.error,
                miniblock_number AS "miniblock_number!",
                miniblocks.timestamp
            FROM miniblocks
            JOIN transactions ON miniblocks.number = transactions.miniblock_number
            WHERE
                (
                    miniblocks.l1_batch_number IS NULL
                    OR miniblocks.l1_batch_number = $1
                )
                AND
                miniblocks.rolling_txs_hash IS NOT NULL
                AND
                miniblocks.eth_precommit_tx_id IS NULL
            ORDER BY miniblock_number, index_in_block
            "#,
            i64::from(l1_batch.0)
        )
        .instrument("get_ready_for_precommit_txs")
        .report_latency()
        .fetch_all(&mut tx)
        .await?
        .into_iter()
        .map(|row| TxForPrecommit {
            l1_batch_number: row.l1_batch_number.map(|a| L1BatchNumber(a as u32)),
            l2block_number: L2BlockNumber(row.miniblock_number as u32),
            timestamp: row.timestamp,
            tx_hash: H256::from_slice(&row.hash),
            is_success: row.error.is_none(),
        })
        .collect();

        Ok(txs)
    }

    pub async fn any_precommit_txs_after_batch(
        &mut self,
        l1_batch: L1BatchNumber,
    ) -> DalResult<bool> {
        let mut tx = self.storage.start_transaction().await?;
        let block_number = sqlx::query!(
            r#"
            SELECT
                number
            FROM miniblocks
            WHERE
                (
                    l1_batch_number > $1
                    OR
                    l1_batch_number IS NULL
                )
                AND
                eth_precommit_tx_id IS NOT NULL
            LIMIT 1
            "#,
            i64::from(l1_batch.0)
        )
        .instrument("precommit_txs_after_l1_batch")
        .report_latency()
        .fetch_optional(&mut tx)
        .await?;
        Ok(block_number.is_some())
    }

    /// Marks provided L1 batch as sealed and populates it with all the runtime information.
    ///
    /// Errors if the batch does not exist.
    pub async fn mark_l1_batch_as_sealed(
        &mut self,
        header: &L1BatchHeader,
        initial_bootloader_contents: &[(usize, U256)],
        storage_refunds: &[u32],
        pubdata_costs: &[i32],
        predicted_circuits_by_type: CircuitStatistic, // predicted number of circuits for each circuit type
    ) -> anyhow::Result<()> {
        let initial_bootloader_contents_len = initial_bootloader_contents.len();
        let instrumentation = Instrumented::new("mark_l1_batch_as_sealed")
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
            UPDATE l1_batches
            SET
                l1_tx_count = $2,
                l2_tx_count = $3,
                l2_to_l1_messages = $4,
                bloom = $5,
                priority_ops_onchain_data = $6,
                initial_bootloader_heap_content = $7,
                used_contract_hashes = $8,
                bootloader_code_hash = $9,
                default_aa_code_hash = $10,
                evm_emulator_code_hash = $11,
                protocol_version = $12,
                system_logs = $13,
                storage_refunds = $14,
                pubdata_costs = $15,
                pubdata_input = $16,
                predicted_circuits_by_type = $17,
                updated_at = NOW(),
                sealed_at = NOW(),
                is_sealed = TRUE
            WHERE
                number = $1
            "#,
            i64::from(header.number.0),
            i32::from(header.l1_tx_count),
            i32::from(header.l2_tx_count),
            &header.l2_to_l1_messages,
            header.bloom.as_bytes(),
            &priority_onchain_data,
            initial_bootloader_contents,
            used_contract_hashes,
            header.base_system_contracts_hashes.bootloader.as_bytes(),
            header.base_system_contracts_hashes.default_aa.as_bytes(),
            header
                .base_system_contracts_hashes
                .evm_emulator
                .as_ref()
                .map(H256::as_bytes),
            header.protocol_version.map(|v| v as i32),
            &system_logs,
            &storage_refunds,
            &pubdata_costs,
            pubdata_input,
            serde_json::to_value(predicted_circuits_by_type).unwrap(),
        );
        let update_result = instrumentation.with(query).execute(self.storage).await?;

        if update_result.rows_affected() == 0 {
            anyhow::bail!(
                "L1 batch sealing failed: batch #{} was not found",
                header.number
            );
        }

        Ok(())
    }

    pub async fn get_unsealed_l1_batch(&mut self) -> DalResult<Option<UnsealedL1BatchHeader>> {
        Self::get_unsealed_l1_batch_inner(self.storage).await
    }

    async fn get_unsealed_l1_batch_inner(
        conn: &mut Connection<'_, Core>,
    ) -> DalResult<Option<UnsealedL1BatchHeader>> {
        let batch = sqlx::query_as!(
            UnsealedStorageL1Batch,
            r#"
            SELECT
                number,
                timestamp,
                protocol_version,
                fee_address,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM (
                SELECT
                    number,
                    timestamp,
                    protocol_version,
                    fee_address,
                    l1_gas_price,
                    l2_fair_gas_price,
                    fair_pubdata_price,
                    pubdata_limit,
                    is_sealed
                FROM l1_batches
                ORDER BY number DESC
                LIMIT 1
            ) AS u
            WHERE NOT is_sealed
            "#,
        )
        .instrument("get_unsealed_l1_batch")
        .fetch_optional(conn)
        .await?;

        Ok(batch.map(|b| b.into()))
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
                evm_emulator_code_hash,
                protocol_version,
                virtual_blocks,
                fair_pubdata_price,
                gas_limit,
                logs_bloom,
                l2_da_validator_address,
                pubdata_type,
                rolling_txs_hash,
                l2_da_commitment_scheme,
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
                $21,
                $22,
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
            l2_block_header
                .base_system_contracts_hashes
                .evm_emulator
                .as_ref()
                .map(H256::as_bytes),
            l2_block_header.protocol_version.map(|v| v as i32),
            i64::from(l2_block_header.virtual_blocks),
            l2_block_header.batch_fee_input.fair_pubdata_price() as i64,
            l2_block_header.gas_limit as i64,
            l2_block_header.logs_bloom.as_bytes(),
            l2_block_header
                .pubdata_params
                .l2_da_validator_address
                .as_ref()
                .map(|addr| addr.as_bytes()),
            l2_block_header.pubdata_params.pubdata_type.to_string(),
            l2_block_header
                .rolling_txs_hash
                .map(|h| h.as_bytes().to_vec()),
            l2_block_header
                .pubdata_params
                .l2_da_commitment_scheme
                .as_ref()
                .map(|l2_da_commitment_scheme| *l2_da_commitment_scheme as i32)
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
                evm_emulator_code_hash,
                protocol_version,
                virtual_blocks,
                fair_pubdata_price,
                gas_limit,
                logs_bloom,
                l2_da_validator_address,
                l2_da_commitment_scheme,
                pubdata_type,
                rolling_txs_hash
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
                evm_emulator_code_hash,
                protocol_version,
                virtual_blocks,
                fair_pubdata_price,
                gas_limit,
                logs_bloom,
                l2_da_validator_address,
                rolling_txs_hash,
                l2_da_commitment_scheme,
                pubdata_type
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
            commitments (
                l1_batch_number,
                events_queue_commitment,
                bootloader_initial_content_commitment
            )
            VALUES
            ($1, $2, $3)
            ON CONFLICT (l1_batch_number) DO NOTHING
            "#,
            i64::from(number.0),
            commitment_artifacts
                .aux_commitments
                .map(|a| a.events_queue_commitment.0.to_vec()),
            commitment_artifacts
                .aux_commitments
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
                evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            LEFT JOIN
                data_availability
                ON data_availability.l1_batch_number = l1_batches.number
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
        .fetch_optional(self.storage)
        .await?;
        let Some(batch) = batch else {
            return Ok(None);
        };
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
            WHERE
                eth_commit_tx_id IS NOT NULL
                AND commitment IS NOT NULL
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

    /// Returns the number of the last L1 batch for which an Ethereum commit tx was sent and confirmed.
    pub async fn get_number_of_last_l1_batch_committed_finailized_on_eth(
        &mut self,
    ) -> DalResult<Option<L1BatchNumber>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
            LEFT JOIN
                eth_txs_history AS commit_tx
                ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id)
            WHERE
                commit_tx.finality_status != 'pending'
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
            LEFT JOIN
                eth_txs_history AS prove_tx
                ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id)
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
            LEFT JOIN
                eth_txs_history AS execute_tx
                ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id)
            WHERE
                execute_tx.finality_status = 'finalized'
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

    /// Returns the number of the last L1 batch for which an Ethereum commit tx exists in DB (may be unconfirmed)
    pub async fn get_number_of_last_l1_batch_with_tx(
        &mut self,
        tx_type: L1BatchAggregatedActionType,
    ) -> DalResult<Option<L1BatchNumber>> {
        struct BlockNumberRow {
            number: i64,
        }
        Ok(match_query_as!(
            BlockNumberRow,
            [r#"
            SELECT
                number
            FROM
                l1_batches
            WHERE "#,
            _,
            r#" ORDER BY
                number DESC
            LIMIT
                1
            "#],
            match (tx_type) {
                L1BatchAggregatedActionType::Commit => ("eth_commit_tx_id IS NOT NULL";),
                L1BatchAggregatedActionType::PublishProofOnchain => ("eth_prove_tx_id IS NOT NULL";),
                L1BatchAggregatedActionType::Execute => ("eth_execute_tx_id IS NOT NULL";),
            }
        )
        .instrument("get_number_of_last_l1_batch_with_tx")
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
                evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            LEFT JOIN
                data_availability
                ON data_availability.l1_batch_number = l1_batches.number
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
                evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                system_logs,
                compressed_state_diffs,
                protocol_version,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                (
                    SELECT
                        l1_batches.*,
                        ROW_NUMBER() OVER (
                            ORDER BY
                                number ASC
                        ) AS row_number
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
                number - row_number = $1
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
                        evm_emulator_code_hash,
                        aux_data_hash,
                        pass_through_data_hash,
                        meta_parameters_hash,
                        protocol_version,
                        compressed_state_diffs,
                        system_logs,
                        events_queue_commitment,
                        bootloader_initial_content_commitment,
                        pubdata_input,
                        fee_address,
                        aggregation_root,
                        local_root,
                        state_diff_hash,
                        data_availability.inclusion_data,
                        l1_gas_price,
                        l2_fair_gas_price,
                        fair_pubdata_price,
                        pubdata_limit
                    FROM
                        l1_batches
                    LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                    LEFT JOIN
                        data_availability
                        ON data_availability.l1_batch_number = l1_batches.number
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
        let Some((from_l2_block, to_l2_block)) = self
            .storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(batch_number)
            .await?
        else {
            return Ok(None);
        };
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(priority_op_id) AS "id?"
            FROM
                transactions
            WHERE
                miniblock_number BETWEEN $1 AND $2
                AND is_priority = TRUE
            "#,
            i64::from(from_l2_block.0),
            i64::from(to_l2_block.0),
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
            JOIN
                eth_txs_history AS commit_tx
                ON (eth_txs.confirmed_eth_tx_history_id = commit_tx.id)
            WHERE
                commit_tx.confirmed_at IS NOT NULL
                AND eth_prove_tx_id IS NOT NULL
                AND eth_execute_tx_id IS NULL
                AND EXTRACT(
                    EPOCH
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
                    evm_emulator_code_hash,
                    aux_data_hash,
                    pass_through_data_hash,
                    meta_parameters_hash,
                    protocol_version,
                    compressed_state_diffs,
                    system_logs,
                    events_queue_commitment,
                    bootloader_initial_content_commitment,
                    pubdata_input,
                    fee_address,
                    aggregation_root,
                    local_root,
                    state_diff_hash,
                    data_availability.inclusion_data,
                    l1_gas_price,
                    l2_fair_gas_price,
                    fair_pubdata_price,
                    pubdata_limit
                FROM
                    l1_batches
                LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
                LEFT JOIN
                    data_availability
                    ON data_availability.l1_batch_number = l1_batches.number
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
                l1_batches.evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            JOIN protocol_versions ON protocol_versions.id = l1_batches.protocol_version
            LEFT JOIN
                data_availability
                ON data_availability.l1_batch_number = l1_batches.number
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
        send_precommit_txs: bool,
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
                l1_batches.evm_emulator_code_hash,
                aux_data_hash,
                pass_through_data_hash,
                meta_parameters_hash,
                protocol_version,
                compressed_state_diffs,
                system_logs,
                events_queue_commitment,
                bootloader_initial_content_commitment,
                pubdata_input,
                fee_address,
                aggregation_root,
                local_root,
                state_diff_hash,
                data_availability.inclusion_data,
                l1_gas_price,
                l2_fair_gas_price,
                fair_pubdata_price,
                pubdata_limit
            FROM
                l1_batches
            LEFT JOIN commitments ON commitments.l1_batch_number = l1_batches.number
            LEFT JOIN
                data_availability
                ON data_availability.l1_batch_number = l1_batches.number
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
                AND (
                    final_precommit_eth_tx_id IS NOT NULL
                    OR $5 IS FALSE
                )
            ORDER BY
                number
            LIMIT
                $6
            "#,
            bootloader_hash.as_bytes(),
            default_aa_hash.as_bytes(),
            protocol_version_id as i32,
            with_da_inclusion_info,
            send_precommit_txs,
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

    pub async fn get_l1_batch_local_root(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<H256>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                local_root
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_local_root")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let Some(local_root) = row.local_root else {
            return Ok(None);
        };
        Ok(Some(H256::from_slice(&local_root)))
    }

    pub async fn get_l1_batch_l2_l1_merkle_root(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<H256>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                l2_l1_merkle_root
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_l2_l1_merkle_root")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let Some(l2_l1_merkle_root) = row.l2_l1_merkle_root else {
            return Ok(None);
        };
        Ok(Some(H256::from_slice(&l2_l1_merkle_root)))
    }

    pub async fn get_l1_batch_chain_merkle_path(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<BatchAndChainMerklePath>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                batch_chain_merkle_path
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_chain_merkle_path")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let Some(batch_chain_merkle_path) = row.batch_chain_merkle_path else {
            return Ok(None);
        };
        Ok(Some(
            bincode::deserialize(&batch_chain_merkle_path).unwrap(),
        ))
    }

    pub async fn get_batch_chain_merkle_path_until_msg_root(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<BatchAndChainMerklePath>> {
        let Some(row) = sqlx::query!(
            r#"
            SELECT
                batch_chain_merkle_path_until_msg_root
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(number.0)
        )
        .instrument("get_batch_chain_merkle_path_until_msg_root")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        else {
            return Ok(None);
        };
        let Some(batch_chain_merkle_path_until_msg_root) =
            row.batch_chain_merkle_path_until_msg_root
        else {
            return Ok(None);
        };
        Ok(Some(
            bincode::deserialize(&batch_chain_merkle_path_until_msg_root).unwrap(),
        ))
    }

    pub async fn get_l1_batch_pubdata_params(
        &mut self,
        number: L1BatchNumber,
    ) -> DalResult<Option<PubdataParams>> {
        Ok(sqlx::query_as!(
            StoragePubdataParams,
            r#"
            SELECT
                l2_da_validator_address,
                l2_da_commitment_scheme,
                pubdata_type
            FROM
                miniblocks
            WHERE
                l1_batch_number = $1
            ORDER BY number ASC
            LIMIT 1
            "#,
            i64::from(number.0)
        )
        .instrument("get_l1_batch_pubdata_params")
        .with_arg("number", &number)
        .fetch_optional(self.storage)
        .await?
        .map(|row| row.into()))
    }

    pub async fn get_executed_batch_roots_on_sl(
        &mut self,
        sl_chain_id: SLChainId,
    ) -> DalResult<Vec<(L1BatchNumber, H256)>> {
        let result = sqlx::query!(
            r#"
            SELECT
                number, l2_l1_merkle_root
            FROM
                l1_batches
            JOIN eth_txs ON eth_txs.id = l1_batches.eth_execute_tx_id
            WHERE
                batch_chain_merkle_path IS NOT NULL
                AND chain_id = $1
            ORDER BY number
            "#,
            sl_chain_id.0 as i64
        )
        .instrument("get_executed_batch_roots_on_sl")
        .with_arg("sl_chain_id", &sl_chain_id)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| {
            let number = L1BatchNumber(row.number as u32);
            let root = H256::from_slice(&row.l2_l1_merkle_root.unwrap());
            (number, root)
        })
        .collect();
        Ok(result)
    }

    pub async fn set_batch_chain_merkle_path(
        &mut self,
        number: L1BatchNumber,
        proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        let proof_bin = bincode::serialize(&proof).unwrap();
        sqlx::query!(
            r#"
            UPDATE
            l1_batches
            SET
                batch_chain_merkle_path = $2
            WHERE
                number = $1
            "#,
            i64::from(number.0),
            &proof_bin
        )
        .instrument("set_batch_chain_merkle_path")
        .with_arg("number", &number)
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn set_batch_chain_merkle_path_until_msg_root(
        &mut self,
        number: L1BatchNumber,
        proof: BatchAndChainMerklePath,
    ) -> DalResult<()> {
        let proof_bin = bincode::serialize(&proof).unwrap();
        sqlx::query!(
            r#"
            UPDATE
            l1_batches
            SET
                batch_chain_merkle_path_until_msg_root = $2
            WHERE
                number = $1
            "#,
            i64::from(number.0),
            &proof_bin
        )
        .instrument("set_batch_chain_merkle_path_until_msg_root")
        .with_arg("number", &number)
        .execute(self.storage)
        .await?;

        Ok(())
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
        let Some((from_l2_block, to_l2_block)) = self
            .storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await?
        else {
            return Ok(Default::default());
        };

        Ok(sqlx::query!(
            r#"
            SELECT
                bytecode_hash,
                bytecode
            FROM
                factory_deps
            WHERE
                miniblock_number BETWEEN $1 AND $2
            "#,
            i64::from(from_l2_block.0),
            i64::from(to_l2_block.0),
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

    /// Deletes the unsealed L1 batch from the storage. Expects the caller to make sure there are no
    /// associated L2 blocks.
    ///
    /// Accepts `batch_to_keep` as a safety mechanism.
    pub async fn delete_unsealed_l1_batch(
        &mut self,
        batch_to_keep: L1BatchNumber,
    ) -> DalResult<()> {
        let deleted_row = sqlx::query!(
            r#"
            DELETE FROM l1_batches
            WHERE
                number > $1
                AND NOT is_sealed
            RETURNING number
            "#,
            i64::from(batch_to_keep.0)
        )
        .instrument("delete_unsealed_l1_batch")
        .with_arg("batch_to_keep", &batch_to_keep)
        .fetch_optional(self.storage)
        .await?;
        if let Some(deleted_row) = deleted_row {
            tracing::info!(
                l1_batch_number = %deleted_row.number,
                "Deleted unsealed batch"
            );
        }
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
                is_sealed
                AND eth_commit_tx_id IS NULL
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
                is_sealed
                AND eth_prove_tx_id IS NULL
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
                is_sealed
                AND eth_execute_tx_id IS NULL
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
                is_sealed
                AND number = $1
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

    pub async fn get_batch_sealed_at(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<DateTime<Utc>>> {
        Ok(sqlx::query!(
            r#"
            SELECT
                sealed_at
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_batch_sealed_at")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_optional(self.storage)
        .await?
        .and_then(|row| row.sealed_at.map(|d| d.and_utc())))
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
                is_sealed
                AND protocol_version = $1
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
        let Some((from_l2_block, to_l2_block)) = self
            .storage
            .blocks_web3_dal()
            .get_l2_block_range_of_l1_batch(l1_batch_number)
            .await?
        else {
            return Ok(Vec::new());
        };
        let results = sqlx::query_as!(
            StorageL2ToL1Log,
            r#"
            SELECT
                miniblock_number,
                log_index_in_miniblock,
                log_index_in_tx,
                tx_hash,
                miniblocks.l1_batch_number,
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
                miniblock_number BETWEEN $1 AND $2
            ORDER BY
                miniblock_number,
                log_index_in_miniblock
            "#,
            i64::from(from_l2_block.0),
            i64::from(to_l2_block.0),
        )
        .instrument("get_l2_to_l1_logs_by_number")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_all(self.storage)
        .await?;

        Ok(results.into_iter().map(L::from).collect())
    }

    pub async fn has_l2_block_bloom(&mut self, l2_block_number: L2BlockNumber) -> DalResult<bool> {
        let row = sqlx::query!(
            r#"
            SELECT
                (logs_bloom IS NOT NULL) AS "logs_bloom_not_null!"
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(l2_block_number.0),
        )
        .instrument("has_l2_block_bloom")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| row.logs_bloom_not_null).unwrap_or(false))
    }

    pub async fn has_last_l2_block_bloom(&mut self) -> DalResult<bool> {
        let row = sqlx::query!(
            r#"
            SELECT
                (logs_bloom IS NOT NULL) AS "logs_bloom_not_null!"
            FROM
                miniblocks
            ORDER BY
                number DESC
            LIMIT
                1
            "#,
        )
        .instrument("has_last_l2_block_bloom")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| row.logs_bloom_not_null).unwrap_or(false))
    }

    pub async fn get_max_l2_block_without_bloom(&mut self) -> DalResult<Option<L2BlockNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MAX(number) AS "max?"
            FROM
                miniblocks
            WHERE
                logs_bloom IS NULL
            "#,
        )
        .instrument("get_max_l2_block_without_bloom")
        .fetch_one(self.storage)
        .await?;

        Ok(row.max.map(|n| L2BlockNumber(n as u32)))
    }

    pub async fn range_update_logs_bloom(
        &mut self,
        from_l2_block: L2BlockNumber,
        blooms: &[Bloom],
    ) -> DalResult<()> {
        if blooms.is_empty() {
            return Ok(());
        }

        let to_l2_block = from_l2_block + (blooms.len() - 1) as u32;
        let numbers: Vec<_> = (i64::from(from_l2_block.0)..=i64::from(to_l2_block.0)).collect();

        let blooms = blooms
            .iter()
            .map(|blooms| blooms.as_bytes())
            .collect::<Vec<_>>();
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                logs_bloom = data.logs_bloom
            FROM
                (
                    SELECT
                        UNNEST($1::BIGINT []) AS number,
                        UNNEST($2::BYTEA []) AS logs_bloom
                ) AS data
            WHERE
                miniblocks.number = data.number
            "#,
            &numbers,
            &blooms as &[&[u8]],
        )
        .instrument("range_update_logs_bloom")
        .execute(self.storage)
        .await?;

        Ok(())
    }

    pub async fn get_batch_number_of_prove_tx_id(
        &mut self,
        tx_id: u32,
    ) -> DalResult<Option<L1BatchNumber>> {
        let row = sqlx::query!(
            r#"
            SELECT
                number
            FROM
                l1_batches
            WHERE
                eth_prove_tx_id = $1
            "#,
            tx_id as i32
        )
        .instrument("get_batch_number_of_prove_tx_id")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|row| L1BatchNumber(row.number as u32)))
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

    pub async fn insert_mock_l1_batch(&mut self, header: &L1BatchHeader) -> anyhow::Result<()> {
        self.insert_l1_batch(header.to_unsealed_header()).await?;
        self.mark_l1_batch_as_sealed(header, &[], &[], &[], Default::default())
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

    pub async fn drop_l2_block_bloom(&mut self, l2_block_number: L2BlockNumber) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE miniblocks
            SET
                logs_bloom = NULL
            WHERE
                number = $1
            "#,
            i64::from(l2_block_number.0)
        )
        .instrument("drop_l2_block_bloom")
        .with_arg("l2_block_number", &l2_block_number)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        aggregated_operations::AggregatedActionType, tx::IncludedTxLocation, Address,
        ProtocolVersion,
    };

    use super::*;
    use crate::{
        tests::{create_l1_batch_header, create_l2_block_header, create_l2_to_l1_log},
        ConnectionPool, Core, CoreDal,
    };

    async fn save_mock_eth_tx(
        action_type: L1BatchAggregatedActionType,
        conn: &mut Connection<'_, Core>,
    ) {
        conn.eth_sender_dal()
            .save_eth_tx(
                1,
                vec![],
                AggregatedActionType::L1Batch(action_type),
                Address::default(),
                Some(1),
                None,
                None,
                false,
            )
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

        save_mock_eth_tx(L1BatchAggregatedActionType::Commit, &mut conn).await;
        save_mock_eth_tx(L1BatchAggregatedActionType::PublishProofOnchain, &mut conn).await;
        save_mock_eth_tx(L1BatchAggregatedActionType::Execute, &mut conn).await;

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit),
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit),
            )
            .await
            .is_err());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::PublishProofOnchain),
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::PublishProofOnchain),
            )
            .await
            .is_err());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                1,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Execute),
            )
            .await
            .is_ok());

        assert!(conn
            .blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                L1BatchNumber(1)..=L1BatchNumber(1),
                2,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Execute),
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn persisting_evm_emulator_hash() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let mut l2_block_header = create_l2_block_header(1);
        l2_block_header.base_system_contracts_hashes.evm_emulator = Some(H256::repeat_byte(0x23));
        conn.blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        let mut fetched_block_header = conn
            .blocks_dal()
            .get_last_sealed_l2_block_header()
            .await
            .unwrap()
            .expect("no block");
        // Batch fee input isn't restored exactly
        fetched_block_header.batch_fee_input = l2_block_header.batch_fee_input;

        assert_eq!(fetched_block_header, l2_block_header);
        // ...and a sanity check just to be sure
        assert!(fetched_block_header
            .base_system_contracts_hashes
            .evm_emulator
            .is_some());
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
}
