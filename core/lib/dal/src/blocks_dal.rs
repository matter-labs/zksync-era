use std::collections::HashMap;
use std::convert::{Into, TryInto};
use std::time::Instant;

use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use sqlx::Row;

use zksync_types::aggregated_operations::AggregatedActionType;
use zksync_types::commitment::{BlockWithMetadata, CommitmentSerializable};

use zksync_types::MAX_GAS_PER_PUBDATA_BYTE;

use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    commitment::BlockMetadata,
    L1BatchNumber, MiniblockNumber, H256,
};

use crate::{models::storage_block::StorageBlock, StorageProcessor};

#[derive(Debug)]
pub struct BlocksDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl BlocksDal<'_, '_> {
    pub fn is_genesis_needed(&mut self) -> bool {
        async_std::task::block_on(async {
            let count: i64 = sqlx::query!(r#"SELECT COUNT(*) as "count!" FROM l1_batches"#)
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .count;
            count == 0
        })
    }

    pub fn get_sealed_block_number(&mut self) -> L1BatchNumber {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let number: i64 = sqlx::query!(
                r#"SELECT MAX(number) as "number" FROM l1_batches WHERE is_finished = TRUE"#
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .number
            .expect("DAL invocation before genesis");
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_block_number");
            L1BatchNumber(number as u32)
        })
    }

    pub fn get_sealed_miniblock_number(&mut self) -> MiniblockNumber {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let number: i64 = sqlx::query!(r#"SELECT MAX(number) as "number" FROM miniblocks"#)
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .number
                .unwrap_or(0);
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_miniblock_number");
            MiniblockNumber(number as u32)
        })
    }

    pub fn get_last_block_number_with_metadata(&mut self) -> L1BatchNumber {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let number: i64 = sqlx::query!(
                r#"SELECT MAX(number) as "number" FROM l1_batches WHERE hash IS NOT NULL"#
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .number
            .expect("DAL invocation before genesis");
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_last_block_number_with_metadata");
            L1BatchNumber(number as u32)
        })
    }

    pub fn get_blocks_for_eth_tx_id(&mut self, eth_tx_id: u32) -> Vec<L1BatchHeader> {
        async_std::task::block_on(async {
            let blocks = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches
                 WHERE eth_commit_tx_id = $1 OR eth_prove_tx_id = $1 OR eth_execute_tx_id = $1",
                eth_tx_id as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            blocks.into_iter().map(|bl| bl.into()).collect()
        })
    }

    fn get_storage_block(&mut self, number: L1BatchNumber) -> Option<StorageBlock> {
        async_std::task::block_on(async {
            sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches WHERE number = $1",
                number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
        })
    }

    pub fn get_block_header(&mut self, number: L1BatchNumber) -> Option<L1BatchHeader> {
        self.get_storage_block(number).map(Into::into)
    }

    pub fn set_eth_tx_id(
        &mut self,
        first_block: L1BatchNumber,
        last_block: L1BatchNumber,
        eth_tx_id: u32,
        aggregation_type: AggregatedActionType,
    ) {
        async_std::task::block_on(async {
            match aggregation_type {
                AggregatedActionType::CommitBlocks => {
                    sqlx::query!(
                        "UPDATE l1_batches \
                        SET eth_commit_tx_id = $1, updated_at = now() \
                        WHERE number BETWEEN $2 AND $3",
                        eth_tx_id as i32,
                        *first_block as i64,
                        *last_block as i64
                    )
                    .execute(self.storage.conn())
                    .await
                    .unwrap();
                }
                AggregatedActionType::PublishProofBlocksOnchain => {
                    sqlx::query!(
                        "UPDATE l1_batches \
                        SET eth_prove_tx_id = $1, updated_at = now() \
                        WHERE number BETWEEN $2 AND $3",
                        eth_tx_id as i32,
                        *first_block as i64,
                        *last_block as i64
                    )
                    .execute(self.storage.conn())
                    .await
                    .unwrap();
                }
                AggregatedActionType::ExecuteBlocks => {
                    sqlx::query!(
                        "UPDATE l1_batches \
                        SET eth_execute_tx_id = $1, updated_at = now() \
                        WHERE number BETWEEN $2 AND $3",
                        eth_tx_id as i32,
                        *first_block as i64,
                        *last_block as i64
                    )
                    .execute(self.storage.conn())
                    .await
                    .unwrap();
                }
            }
        })
    }

    pub fn insert_l1_batch(&mut self, block: L1BatchHeader, predicted_block_gas: BlockGasCount) {
        async_std::task::block_on(async {
            let priority_onchain_data: Vec<Vec<u8>> = block
                .priority_ops_onchain_data
                .iter()
                .map(|data| data.clone().into())
                .collect();
            let l2_to_l1_logs: Vec<Vec<u8>> = block
                .l2_to_l1_logs
                .iter()
                .map(|log| log.clone().to_bytes())
                .collect();

            let initial_bootloader_contents =
                serde_json::to_value(block.initial_bootloader_contents)
                    .expect("failed to serialize initial_bootloader_contents to JSON value");

            let used_contract_hashes = serde_json::to_value(block.used_contract_hashes)
                .expect("failed to serialize used_contract_hashes to JSON value");

            let base_fee_per_gas = BigDecimal::from_u64(block.base_fee_per_gas)
                .expect("block.base_fee_per_gas should fit in u64");

            sqlx::query!(
            "INSERT INTO l1_batches (number, l1_tx_count, l2_tx_count,
            timestamp, is_finished, fee_account_address, l2_to_l1_logs, l2_to_l1_messages, bloom, priority_ops_onchain_data,
            predicted_commit_gas_cost, predicted_prove_gas_cost, predicted_execute_gas_cost,
            initial_bootloader_heap_content, used_contract_hashes, base_fee_per_gas, l1_gas_price, l2_fair_gas_price,
                created_at, updated_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, now(), now())
            ",
            block.number.0 as i64,
            block.l1_tx_count as i32,
            block.l2_tx_count as i32,
            block.timestamp as i64,
            block.is_finished,
            block.fee_account_address.as_bytes(),
            &l2_to_l1_logs,
            &block.l2_to_l1_messages,
            block.bloom.as_bytes(),
            &priority_onchain_data,
            predicted_block_gas.commit as i64,
            predicted_block_gas.prove as i64,
            predicted_block_gas.execute as i64,
            initial_bootloader_contents,
            used_contract_hashes,
            base_fee_per_gas,
            block.l1_gas_price as i64,
            block.l2_fair_gas_price as i64
            )
                .execute(self.storage.conn())
                .await
                .unwrap();
        })
    }

    pub fn insert_miniblock(&mut self, miniblock_header: MiniblockHeader) {
        let base_fee_per_gas = BigDecimal::from_u64(miniblock_header.base_fee_per_gas)
            .expect("base_fee_per_gas should fit in u64");

        async_std::task::block_on(async {
            sqlx::query!(
                "
                    INSERT INTO miniblocks (
                        number, timestamp, hash, l1_tx_count, l2_tx_count,
                        base_fee_per_gas, l1_gas_price, l2_fair_gas_price, gas_per_pubdata_limit, created_at, updated_at
                    )
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, now(), now())
                ",
                miniblock_header.number.0 as i64,
                miniblock_header.timestamp as i64,
                miniblock_header.hash.as_bytes(),
                miniblock_header.l1_tx_count as i32,
                miniblock_header.l2_tx_count as i32,
                base_fee_per_gas,
                miniblock_header.l1_gas_price as i64,
                miniblock_header.l2_fair_gas_price as i64,
                MAX_GAS_PER_PUBDATA_BYTE as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_miniblock_header(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Option<MiniblockHeader> {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    SELECT number, timestamp, hash, l1_tx_count, l2_tx_count,
                        base_fee_per_gas, l1_gas_price, l2_fair_gas_price
                    FROM miniblocks
                    WHERE number = $1
                ",
                miniblock_number.0 as i64,
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .map(|row| MiniblockHeader {
                number: MiniblockNumber(row.number as u32),
                timestamp: row.timestamp as u64,
                hash: H256::from_slice(&row.hash),
                l1_tx_count: row.l1_tx_count as u16,
                l2_tx_count: row.l2_tx_count as u16,
                base_fee_per_gas: row
                    .base_fee_per_gas
                    .to_u64()
                    .expect("base_fee_per_gas should fit in u64"),
                l1_gas_price: row.l1_gas_price as u64,
                l2_fair_gas_price: row.l2_fair_gas_price as u64,
            })
        })
    }

    pub fn mark_miniblocks_as_executed_in_l1_batch(&mut self, l1_batch_number: L1BatchNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    UPDATE miniblocks
                    SET l1_batch_number = $1
                    WHERE l1_batch_number IS NULL
                ",
                l1_batch_number.0 as i32,
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn save_block_metadata(
        &mut self,
        block_number: L1BatchNumber,
        block_metadata: BlockMetadata,
    ) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    UPDATE l1_batches
                    SET hash = $1, merkle_root_hash = $2, commitment = $3, default_aa_code_hash = $4,
                        compressed_repeated_writes = $5, compressed_initial_writes = $6, l2_l1_compressed_messages = $7,
                        l2_l1_merkle_root = $8,
                        zkporter_is_available = $9, bootloader_code_hash = $10, rollup_last_leaf_index = $11,
                        aux_data_hash = $12, pass_through_data_hash = $13, meta_parameters_hash = $14,
                        updated_at = now()
                    WHERE number = $15
                ",
                block_metadata.root_hash.as_bytes(),
                block_metadata.merkle_root_hash.as_bytes(),
                block_metadata.commitment.as_bytes(),
                block_metadata.block_meta_params.default_aa_code_hash.as_bytes(),
                block_metadata.repeated_writes_compressed,
                block_metadata.initial_writes_compressed,
                block_metadata.l2_l1_messages_compressed,
                block_metadata.l2_l1_merkle_root.as_bytes(),
                block_metadata.block_meta_params.zkporter_is_available,
                block_metadata.block_meta_params.bootloader_code_hash.as_bytes(),
                block_metadata.rollup_last_leaf_index as i64,
                block_metadata.aux_data_hash.as_bytes(),
                block_metadata.pass_through_data_hash.as_bytes(),
                block_metadata.meta_parameters_hash.as_bytes(),
                block_number.0 as i64,
            )
                .execute(self.storage.conn())
                .await
                .unwrap();
        })
    }

    pub fn save_blocks_metadata(
        &mut self,
        block_number: L1BatchNumber,
        block_metadata: BlockMetadata,
        previous_root_hash: H256,
    ) {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let update_result = sqlx::query!(
                "
                    UPDATE l1_batches SET
                        hash = $1, merkle_root_hash = $2, commitment = $3, default_aa_code_hash = $4,
                        compressed_repeated_writes = $5, compressed_initial_writes = $6, l2_l1_compressed_messages = $7,
                        l2_l1_merkle_root = $8, zkporter_is_available = $9, 
                        bootloader_code_hash = $10, parent_hash = $11, rollup_last_leaf_index = $12, 
                        aux_data_hash = $13, pass_through_data_hash = $14, meta_parameters_hash = $15,
                        updated_at = NOW()
                    WHERE number = $16 AND hash IS NULL
                ",
                block_metadata.root_hash.as_bytes(),
                block_metadata.merkle_root_hash.as_bytes(),
                block_metadata.commitment.as_bytes(),
                block_metadata.block_meta_params.default_aa_code_hash.as_bytes(),
                block_metadata.repeated_writes_compressed,
                block_metadata.initial_writes_compressed,
                block_metadata.l2_l1_messages_compressed,
                block_metadata.l2_l1_merkle_root.as_bytes(),
                block_metadata.block_meta_params.zkporter_is_available,
                block_metadata.block_meta_params.bootloader_code_hash.as_bytes(),
                previous_root_hash.0.to_vec(),
                block_metadata.rollup_last_leaf_index as i64,
                block_metadata.aux_data_hash.as_bytes(),
                block_metadata.pass_through_data_hash.as_bytes(),
                block_metadata.meta_parameters_hash.as_bytes(),
                block_number.0 as i64,
            )
                .execute(self.storage.conn())
                .await
                .unwrap();

            if update_result.rows_affected() == 0 {
                vlog::debug!(
                    "L1 batch {} info wasn't updated. Details: root_hash: {:?}, merkle_root_hash: {:?}, parent_hash: {:?}, commitment: {:?}, l2_l1_merkle_root: {:?}",
                    block_number.0 as i64,
                    block_metadata.root_hash.0.to_vec(),
                    block_metadata.merkle_root_hash.0.to_vec(),
                    previous_root_hash.0.to_vec(),
                    block_metadata.commitment.0.to_vec(),
                    block_metadata.l2_l1_merkle_root.as_bytes()
                );

                // block was already processed. Verify that existing hashes match
                let matched: i64 = sqlx::query!(
                    r#"
                        SELECT COUNT(*) as "count!"
                        FROM l1_batches
                        WHERE number = $1
                            AND hash = $2
                           AND merkle_root_hash = $3
                           AND parent_hash = $4
                           AND l2_l1_merkle_root = $5
                    "#,
                    block_number.0 as i64,
                    block_metadata.root_hash.0.to_vec(),
                    block_metadata.merkle_root_hash.0.to_vec(),
                    previous_root_hash.0.to_vec(),
                    block_metadata.l2_l1_merkle_root.as_bytes(),
                )
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .count;

                assert_eq!(matched, 1, "Root hash verification failed. Hashes for some of previously processed blocks do not match");
            }
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "save_blocks_metadata");
        })
    }

    pub fn get_last_committed_to_eth_block(&mut self) -> Option<BlockWithMetadata> {
        async_std::task::block_on(async {
            // We can get 0 block for the first transaction
            let block = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE number = 0 OR eth_commit_tx_id IS NOT NULL AND commitment IS NOT NULL \
                ORDER BY number DESC LIMIT 1",
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            // genesis block is first generated without commitment, we should wait for the tree to set it.
            block.commitment.as_ref()?;

            self.get_block_with_metadata(block)
        })
    }

    /// Returns the number of the last block for which an Ethereum execute tx was sent and confirmed.
    pub fn get_number_of_last_block_executed_on_eth(&mut self) -> Option<L1BatchNumber> {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT number FROM l1_batches
                LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id)
                WHERE execute_tx.confirmed_at IS NOT NULL
                ORDER BY number DESC LIMIT 1"
            )
                .fetch_optional(self.storage.conn())
                .await
                .unwrap()
                .map(|record| L1BatchNumber(record.number as u32))
        })
    }

    /// This method returns blocks for which the proofs are computed
    pub fn get_ready_for_proof_blocks_real_verifier(
        &mut self,
        limit: usize,
    ) -> Vec<BlockWithMetadata> {
        async_std::task::block_on(async {
            let last_proved_block_number_row = sqlx::query!(
                r#"SELECT COALESCE(max(number), 0) as "number!" FROM l1_batches
                WHERE eth_prove_tx_id IS NOT NULL"#
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            let last_proved_block_number =
                L1BatchNumber(last_proved_block_number_row.number as u32);
            // note that the proofs can be generated out of order, so
            // `WHERE l1_batches.number - row_number = $1` is used to avoid having gaps in the list of blocks to proof
            // note that we need to manually list all the columns in `l1_batches` table here - we cannot use `*` because there is one extra column (`row_number`)
            let l1_batches = sqlx::query_as!(
                StorageBlock,
                "
                SELECT number, timestamp, is_finished, l1_tx_count, l2_tx_count, fee_account_address, bloom, priority_ops_onchain_data, hash, parent_hash, commitment, compressed_write_logs, compressed_contracts, eth_prove_tx_id, eth_commit_tx_id, eth_execute_tx_id, created_at, updated_at, merkle_root_hash, l2_to_l1_logs, l2_to_l1_messages, predicted_commit_gas_cost, predicted_prove_gas_cost, predicted_execute_gas_cost, initial_bootloader_heap_content, used_contract_hashes, compressed_initial_writes, compressed_repeated_writes, l2_l1_compressed_messages, l2_l1_merkle_root, l1_gas_price, l2_fair_gas_price, rollup_last_leaf_index, zkporter_is_available, bootloader_code_hash, default_aa_code_hash, base_fee_per_gas, aux_data_hash, pass_through_data_hash, meta_parameters_hash, skip_proof, gas_per_pubdata_byte_in_block, gas_per_pubdata_limit
                FROM
                (SELECT l1_batches.*, row_number() over (order by number ASC) as row_number
                 FROM l1_batches
                 LEFT JOIN prover_jobs ON prover_jobs.l1_batch_number = l1_batches.number
                    WHERE eth_commit_tx_id IS NOT NULL
                      AND prover_jobs.aggregation_round = 3
                      AND prover_jobs.status = 'successful'
                      AND l1_batches.number > $1
                    ORDER BY number LIMIT $2) inn
                WHERE number - row_number = $1
                ",
                last_proved_block_number.0 as i32,
                limit as i32
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap();
            l1_batches
                .into_iter()
                .map(|block| {
                    self.get_block_with_metadata(block)
                        .expect("Block should be complete")
                })
                .collect()
        })
    }

    /// This method returns blocks that are confirmed on L1. That is, it doesn't wait for the proofs to be generated.
    pub fn get_ready_for_dummy_proof_blocks(&mut self, limit: usize) -> Vec<BlockWithMetadata> {
        async_std::task::block_on(async {
            let l1_batches = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE eth_commit_tx_id IS NOT NULL AND eth_prove_tx_id IS NULL \
                ORDER BY number LIMIT $1",
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            l1_batches
                .into_iter()
                .map(|block| {
                    self.get_block_with_metadata(block)
                        .expect("Block should be complete")
                })
                .collect()
        })
    }

    pub fn set_skip_proof_for_l1_batch(&mut self, l1_batch_number: L1BatchNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    UPDATE l1_batches
                    SET skip_proof = TRUE WHERE number = $1
                ",
                l1_batch_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    /// This method returns blocks that are committed on L1 and witness jobs for them are skipped.
    pub fn get_skipped_for_proof_blocks(&mut self, limit: usize) -> Vec<BlockWithMetadata> {
        async_std::task::block_on(async {
            let last_proved_block_number_row = sqlx::query!(
                r#"SELECT COALESCE(max(number), 0) as "number!" FROM l1_batches
                WHERE eth_prove_tx_id IS NOT NULL"#
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            let last_proved_block_number =
                L1BatchNumber(last_proved_block_number_row.number as u32);
            // note that the witness jobs can be processed out of order, so
            // `WHERE l1_batches.number - row_number = $1` is used to avoid having gaps in the list of blocks to send dummy proofs for
            // note that we need to manually list all the columns in `l1_batches` table here - we cannot use `*` because there is one extra column (`row_number`)
            let l1_batches = sqlx::query_as!(
                StorageBlock,
                "
                SELECT number, timestamp, is_finished, l1_tx_count, l2_tx_count, fee_account_address, bloom, priority_ops_onchain_data, hash, parent_hash, commitment, compressed_write_logs, compressed_contracts, eth_prove_tx_id, eth_commit_tx_id, eth_execute_tx_id, created_at, updated_at, merkle_root_hash, l2_to_l1_logs, l2_to_l1_messages, predicted_commit_gas_cost, predicted_prove_gas_cost, predicted_execute_gas_cost, initial_bootloader_heap_content, used_contract_hashes, compressed_initial_writes, compressed_repeated_writes, l2_l1_compressed_messages, l2_l1_merkle_root, l1_gas_price, l2_fair_gas_price, rollup_last_leaf_index, zkporter_is_available, bootloader_code_hash, default_aa_code_hash, base_fee_per_gas, aux_data_hash, pass_through_data_hash, meta_parameters_hash, skip_proof, gas_per_pubdata_byte_in_block, gas_per_pubdata_limit
                FROM
                (SELECT l1_batches.*, row_number() over (order by number ASC) as row_number
                    FROM l1_batches
                    WHERE eth_commit_tx_id IS NOT NULL
                      AND l1_batches.skip_proof = TRUE
                      AND l1_batches.number > $1
                    ORDER BY number LIMIT $2) inn
                WHERE number - row_number = $1
                ",
                last_proved_block_number.0 as i32,
                limit as i32
            )
                .fetch_all(self.storage.conn())
                .await
                .unwrap();
            l1_batches
                .into_iter()
                .map(|block| {
                    self.get_block_with_metadata(block)
                        .expect("Block should be complete")
                })
                .collect()
        })
    }

    pub fn get_ready_for_execute_blocks(&mut self, limit: usize) -> Vec<BlockWithMetadata> {
        async_std::task::block_on(async {
            let l1_batches = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE eth_prove_tx_id IS NOT NULL AND eth_execute_tx_id IS NULL \
                ORDER BY number LIMIT $1",
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            l1_batches
                .into_iter()
                .map(|block| {
                    self.get_block_with_metadata(block)
                        .expect("Block should be complete")
                })
                .collect()
        })
    }

    pub fn get_ready_for_commit_blocks(&mut self, limit: usize) -> Vec<BlockWithMetadata> {
        async_std::task::block_on(async {
            let l1_batches = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE eth_commit_tx_id IS NULL \
                AND number != 0 \
                AND commitment IS NOT NULL \
                ORDER BY number LIMIT $1",
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            l1_batches
                .into_iter()
                .map(|block| {
                    self.get_block_with_metadata(block)
                        .expect("Block should be complete")
                })
                .collect()
        })
    }

    pub fn get_block_state_root(&mut self, number: L1BatchNumber) -> Option<H256> {
        async_std::task::block_on(async {
            let hash: Option<_> = sqlx::query!(
                "SELECT hash FROM l1_batches WHERE number = $1",
                number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap()
            .and_then(|row| row.hash)
            .map(|hash| H256::from_slice(&hash));
            hash
        })
    }

    pub fn get_merkle_state_root(&mut self, number: L1BatchNumber) -> Option<H256> {
        async_std::task::block_on(async {
            let hash: Option<Vec<u8>> = sqlx::query!(
                "SELECT merkle_root_hash FROM l1_batches WHERE number = $1",
                number.0 as i64
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .merkle_root_hash;
            hash.map(|hash| H256::from_slice(&hash))
        })
    }

    pub fn get_newest_block_header(&mut self) -> L1BatchHeader {
        async_std::task::block_on(async {
            let last_block = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches
                ORDER BY number DESC
                LIMIT 1"
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            last_block.into()
        })
    }

    pub fn get_block_metadata(&mut self, number: L1BatchNumber) -> Option<BlockWithMetadata> {
        async_std::task::block_on(async {
            let l1_batch: Option<StorageBlock> = sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches WHERE number = $1",
                number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await
            .unwrap();

            l1_batch.and_then(|bl| self.get_block_with_metadata(bl))
        })
    }

    fn get_block_with_metadata(
        &mut self,
        storage_block: StorageBlock,
    ) -> Option<BlockWithMetadata> {
        async_std::task::block_on(async {
            let unsorted_factory_deps =
                self.get_l1_batch_factory_deps(L1BatchNumber(storage_block.number as u32));
            let block_header = storage_block.clone().try_into().ok()?;
            let block_metadata = storage_block.try_into().ok()?;

            Some(BlockWithMetadata::new(
                block_header,
                block_metadata,
                unsorted_factory_deps,
            ))
        })
    }

    pub fn get_l1_batch_factory_deps(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<H256, Vec<u8>> {
        async_std::task::block_on(async {
            sqlx::query!(
                "SELECT bytecode_hash, bytecode FROM factory_deps
                INNER JOIN miniblocks ON miniblocks.number = factory_deps.miniblock_number
                WHERE miniblocks.l1_batch_number = $1",
                l1_batch_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
            .collect()
        })
    }

    pub fn delete_l1_batches(&mut self, block_number: L1BatchNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM l1_batches WHERE number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn delete_miniblocks(&mut self, block_number: MiniblockNumber) {
        async_std::task::block_on(async {
            sqlx::query!(
                "DELETE FROM miniblocks WHERE number > $1",
                block_number.0 as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    /// Returns sum of predicted gas costs or given block range.
    /// Panics if the sum doesn't fit into usize.
    pub fn get_blocks_predicted_gas(
        &mut self,
        from_block: L1BatchNumber,
        to_block: L1BatchNumber,
        op_type: AggregatedActionType,
    ) -> u32 {
        async_std::task::block_on(async {
            let column_name = match op_type {
                AggregatedActionType::CommitBlocks => "predicted_commit_gas_cost",
                AggregatedActionType::PublishProofBlocksOnchain => "predicted_prove_gas_cost",
                AggregatedActionType::ExecuteBlocks => "predicted_execute_gas_cost",
            };
            let sql_query_str = format!(
                "
                SELECT COALESCE(SUM({}),0) as sum FROM l1_batches
                WHERE number BETWEEN {} AND {}
                ",
                column_name, from_block, to_block
            );
            sqlx::query(&sql_query_str)
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .get::<BigDecimal, &str>("sum")
                .to_u32()
                .expect("Sum of predicted gas costs should fit into u32")
        })
    }

    pub fn update_predicted_block_commit_gas(
        &mut self,
        l1_batch_number: L1BatchNumber,
        predicted_gas_cost: u32,
    ) {
        async_std::task::block_on(async {
            sqlx::query!(
                "
                    UPDATE l1_batches
                    SET predicted_commit_gas_cost = $2, updated_at = now()
                    WHERE number = $1
                ",
                l1_batch_number.0 as i64,
                predicted_gas_cost as i64
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_miniblock_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<(MiniblockNumber, MiniblockNumber)> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                r#"
                    SELECT MIN(miniblocks.number) as "min?", MAX(miniblocks.number) as "max?"
                    FROM miniblocks
                    WHERE l1_batch_number = $1
                "#,
                l1_batch_number.0 as i64
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap();
            match (row.min, row.max) {
                (Some(min), Some(max)) => {
                    Some((MiniblockNumber(min as u32), MiniblockNumber(max as u32)))
                }
                (None, None) => None,
                _ => unreachable!(),
            }
        })
    }

    pub fn get_l1_batches_with_blobs_in_db(&mut self, limit: u8) -> Vec<L1BatchNumber> {
        async_std::task::block_on(async {
            let l1_batches = sqlx::query!(
                r#"
                    SELECT l1_batch_number FROM witness_inputs
                    WHERE length(merkle_tree_paths) <> 0
                    LIMIT $1;
                "#,
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            l1_batches
                .into_iter()
                .map(|row| L1BatchNumber(row.l1_batch_number as u32))
                .collect()
        })
    }

    pub fn purge_blobs_from_db(&mut self, l1_batches: Vec<L1BatchNumber>) {
        let l1_batches: Vec<i64> = l1_batches
            .iter()
            .map(|l1_batch| l1_batch.0 as i64)
            .collect();
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE witness_inputs
                SET merkle_tree_paths=''
                WHERE l1_batch_number = ANY($1);
            "#,
                &l1_batches[..]
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }

    pub fn get_merkle_tree_paths_blob_urls_to_be_cleaned(
        &mut self,
        limit: u8,
    ) -> Vec<(i64, String)> {
        async_std::task::block_on(async {
            let job_ids = sqlx::query!(
                r#"
                    SELECT l1_batch_number, merkel_tree_paths_blob_url FROM witness_inputs
                    WHERE status='successful' AND is_blob_cleaned=FALSE
                    AND merkel_tree_paths_blob_url is NOT NULL
                    AND updated_at < NOW() - INTERVAL '2 days'
                    LIMIT $1;
                "#,
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap();
            job_ids
                .into_iter()
                .map(|row| (row.l1_batch_number, row.merkel_tree_paths_blob_url.unwrap()))
                .collect()
        })
    }

    pub fn mark_gcs_blobs_as_cleaned(&mut self, l1_batch_numbers: Vec<i64>) {
        async_std::task::block_on(async {
            sqlx::query!(
                r#"
                UPDATE witness_inputs
                SET is_blob_cleaned=TRUE
                WHERE l1_batch_number = ANY($1);
            "#,
                &l1_batch_numbers[..]
            )
            .execute(self.storage.conn())
            .await
            .unwrap();
        })
    }
}
