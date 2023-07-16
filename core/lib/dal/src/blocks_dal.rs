use std::{
    collections::HashMap,
    convert::{Into, TryInto},
    time::Instant,
};

use bigdecimal::{BigDecimal, FromPrimitive, ToPrimitive};
use sqlx::Row;

use zksync_types::{
    aggregated_operations::AggregatedActionType,
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    commitment::{BlockMetadata, BlockWithMetadata},
    L1BatchNumber, MiniblockNumber, H256, MAX_GAS_PER_PUBDATA_BYTE,
};

use crate::{
    models::storage_block::{StorageBlock, StorageMiniblockHeader},
    StorageProcessor,
};

#[derive(Debug)]
pub struct BlocksDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl BlocksDal<'_, '_> {
    pub async fn is_genesis_needed(&mut self) -> bool {
        let count = sqlx::query!("SELECT COUNT(*) as \"count!\" FROM l1_batches")
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count;
        count == 0
    }

    pub async fn get_sealed_block_number(&mut self) -> L1BatchNumber {
        let started_at = Instant::now();
        let number = sqlx::query!(
            "SELECT MAX(number) as \"number\" FROM l1_batches WHERE is_finished = TRUE"
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .number
        .expect("DAL invocation before genesis");

        metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_block_number");
        L1BatchNumber(number as u32)
    }

    pub async fn get_sealed_miniblock_number(&mut self) -> MiniblockNumber {
        let started_at = Instant::now();
        let number: i64 = sqlx::query!("SELECT MAX(number) as \"number\" FROM miniblocks")
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .number
            .unwrap_or(0);

        metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_miniblock_number");
        MiniblockNumber(number as u32)
    }

    pub async fn get_last_block_number_with_metadata(&mut self) -> L1BatchNumber {
        let started_at = Instant::now();
        let number: i64 =
            sqlx::query!("SELECT MAX(number) as \"number\" FROM l1_batches WHERE hash IS NOT NULL")
                .fetch_one(self.storage.conn())
                .await
                .unwrap()
                .number
                .expect("DAL invocation before genesis");

        metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_last_block_number_with_metadata");
        L1BatchNumber(number as u32)
    }

    pub async fn get_blocks_for_eth_tx_id(&mut self, eth_tx_id: u32) -> Vec<L1BatchHeader> {
        let blocks = sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches \
            WHERE eth_commit_tx_id = $1 OR eth_prove_tx_id = $1 OR eth_execute_tx_id = $1",
            eth_tx_id as i32
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        blocks.into_iter().map(Into::into).collect()
    }

    pub async fn get_storage_block(&mut self, number: L1BatchNumber) -> Option<StorageBlock> {
        sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches WHERE number = $1",
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
    }

    pub async fn get_block_header(&mut self, number: L1BatchNumber) -> Option<L1BatchHeader> {
        self.get_storage_block(number).await.map(Into::into)
    }

    pub async fn set_eth_tx_id(
        &mut self,
        first_block: L1BatchNumber,
        last_block: L1BatchNumber,
        eth_tx_id: u32,
        aggregation_type: AggregatedActionType,
    ) {
        match aggregation_type {
            AggregatedActionType::CommitBlocks => {
                sqlx::query!(
                    "UPDATE l1_batches \
                    SET eth_commit_tx_id = $1, updated_at = now() \
                    WHERE number BETWEEN $2 AND $3",
                    eth_tx_id as i32,
                    first_block.0 as i64,
                    last_block.0 as i64
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
                    first_block.0 as i64,
                    last_block.0 as i64
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
                    first_block.0 as i64,
                    last_block.0 as i64
                )
                .execute(self.storage.conn())
                .await
                .unwrap();
            }
        }
    }

    pub async fn insert_l1_batch(
        &mut self,
        block: &L1BatchHeader,
        predicted_block_gas: BlockGasCount,
    ) {
        let priority_onchain_data: Vec<Vec<u8>> = block
            .priority_ops_onchain_data
            .iter()
            .map(|data| data.clone().into())
            .collect();
        let l2_to_l1_logs: Vec<_> = block
            .l2_to_l1_logs
            .iter()
            .map(|log| log.to_bytes().to_vec())
            .collect();

        let initial_bootloader_contents = serde_json::to_value(&block.initial_bootloader_contents)
            .expect("failed to serialize initial_bootloader_contents to JSON value");
        let used_contract_hashes = serde_json::to_value(&block.used_contract_hashes)
            .expect("failed to serialize used_contract_hashes to JSON value");
        let base_fee_per_gas = BigDecimal::from_u64(block.base_fee_per_gas)
            .expect("block.base_fee_per_gas should fit in u64");

        sqlx::query!(
            "INSERT INTO l1_batches (\
                number, l1_tx_count, l2_tx_count, \
                timestamp, is_finished, fee_account_address, l2_to_l1_logs, l2_to_l1_messages, \
                bloom, priority_ops_onchain_data, \
                predicted_commit_gas_cost, predicted_prove_gas_cost, predicted_execute_gas_cost, \
                initial_bootloader_heap_content, used_contract_hashes, base_fee_per_gas, \
                l1_gas_price, l2_fair_gas_price, bootloader_code_hash, default_aa_code_hash, \
                created_at, updated_at\
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, now(), now())",
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
            block.l2_fair_gas_price as i64,
            block
                .base_system_contracts_hashes
                .bootloader
                .as_bytes(),
            block
                .base_system_contracts_hashes
                .default_aa
                .as_bytes()
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn insert_miniblock(&mut self, miniblock_header: &MiniblockHeader) {
        let base_fee_per_gas = BigDecimal::from_u64(miniblock_header.base_fee_per_gas)
            .expect("base_fee_per_gas should fit in u64");

        sqlx::query!(
            "INSERT INTO miniblocks (\
                number, timestamp, hash, l1_tx_count, l2_tx_count, \
                base_fee_per_gas, l1_gas_price, l2_fair_gas_price, gas_per_pubdata_limit, \
                bootloader_code_hash, default_aa_code_hash, \
                created_at, updated_at\
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, now(), now())",
            miniblock_header.number.0 as i64,
            miniblock_header.timestamp as i64,
            miniblock_header.hash.as_bytes(),
            miniblock_header.l1_tx_count as i32,
            miniblock_header.l2_tx_count as i32,
            base_fee_per_gas,
            miniblock_header.l1_gas_price as i64,
            miniblock_header.l2_fair_gas_price as i64,
            MAX_GAS_PER_PUBDATA_BYTE as i64,
            miniblock_header
                .base_system_contracts_hashes
                .bootloader
                .as_bytes(),
            miniblock_header
                .base_system_contracts_hashes
                .default_aa
                .as_bytes(),
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_last_sealed_miniblock_header(&mut self) -> Option<MiniblockHeader> {
        sqlx::query_as!(
            StorageMiniblockHeader,
            "SELECT number, timestamp, hash, l1_tx_count, l2_tx_count, \
                base_fee_per_gas, l1_gas_price, l2_fair_gas_price, \
                bootloader_code_hash, default_aa_code_hash \
            FROM miniblocks \
            ORDER BY number DESC \
            LIMIT 1",
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(Into::into)
    }

    pub async fn get_miniblock_header(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Option<MiniblockHeader> {
        sqlx::query_as!(
            StorageMiniblockHeader,
            "SELECT number, timestamp, hash, l1_tx_count, l2_tx_count, \
                base_fee_per_gas, l1_gas_price, l2_fair_gas_price, \
                bootloader_code_hash, default_aa_code_hash \
            FROM miniblocks \
            WHERE number = $1",
            miniblock_number.0 as i64,
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(Into::into)
    }

    pub async fn mark_miniblocks_as_executed_in_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) {
        sqlx::query!(
            "UPDATE miniblocks \
            SET l1_batch_number = $1 \
            WHERE l1_batch_number IS NULL",
            l1_batch_number.0 as i32,
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn save_block_metadata(
        &mut self,
        block_number: L1BatchNumber,
        block_metadata: &BlockMetadata,
    ) {
        sqlx::query!(
            "UPDATE l1_batches \
            SET hash = $1, merkle_root_hash = $2, commitment = $3, default_aa_code_hash = $4, \
                compressed_repeated_writes = $5, compressed_initial_writes = $6, \
                l2_l1_compressed_messages = $7, l2_l1_merkle_root = $8, \
                zkporter_is_available = $9, bootloader_code_hash = $10, rollup_last_leaf_index = $11, \
                aux_data_hash = $12, pass_through_data_hash = $13, meta_parameters_hash = $14, \
                updated_at = now() \
            WHERE number = $15",
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
    }

    pub async fn save_blocks_metadata(
        &mut self,
        block_number: L1BatchNumber,
        block_metadata: &BlockMetadata,
        previous_root_hash: H256,
    ) {
        let started_at = Instant::now();
        let update_result = sqlx::query!(
            "UPDATE l1_batches \
            SET hash = $1, merkle_root_hash = $2, commitment = $3, \
                compressed_repeated_writes = $4, compressed_initial_writes = $5, \
                l2_l1_compressed_messages = $6, l2_l1_merkle_root = $7, \
                zkporter_is_available = $8, parent_hash = $9, rollup_last_leaf_index = $10, \
                aux_data_hash = $11, pass_through_data_hash = $12, meta_parameters_hash = $13, \
                updated_at = now() \
            WHERE number = $14 AND hash IS NULL",
            block_metadata.root_hash.as_bytes(),
            block_metadata.merkle_root_hash.as_bytes(),
            block_metadata.commitment.as_bytes(),
            block_metadata.repeated_writes_compressed,
            block_metadata.initial_writes_compressed,
            block_metadata.l2_l1_messages_compressed,
            block_metadata.l2_l1_merkle_root.as_bytes(),
            block_metadata.block_meta_params.zkporter_is_available,
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
                "L1 batch {} info wasn't updated. Details: root_hash: {:?}, merkle_root_hash: {:?}, \
                 parent_hash: {:?}, commitment: {:?}, l2_l1_merkle_root: {:?}",
                block_number.0 as i64,
                block_metadata.root_hash,
                block_metadata.merkle_root_hash,
                previous_root_hash,
                block_metadata.commitment,
                block_metadata.l2_l1_merkle_root
            );

            // block was already processed. Verify that existing hashes match
            let matched: i64 = sqlx::query!(
                "SELECT COUNT(*) as \"count!\" \
                FROM l1_batches \
                WHERE number = $1 AND hash = $2 AND merkle_root_hash = $3 \
                   AND parent_hash = $4 AND l2_l1_merkle_root = $5",
                block_number.0 as i64,
                block_metadata.root_hash.as_bytes(),
                block_metadata.merkle_root_hash.as_bytes(),
                previous_root_hash.as_bytes(),
                block_metadata.l2_l1_merkle_root.as_bytes(),
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count;

            assert!(
                matched == 1,
                "Root hash verification failed. Hashes for L1 batch #{} do not match the expected values \
                 (expected state hash: {:?}, L2 to L1 logs hash: {:?})",
                block_number,
                block_metadata.root_hash,
                block_metadata.l2_l1_merkle_root
            );
        }
        metrics::histogram!("dal.request", started_at.elapsed(), "method" => "save_blocks_metadata");
    }

    pub async fn get_last_committed_to_eth_block(&mut self) -> Option<BlockWithMetadata> {
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

        self.get_block_with_metadata(block).await
    }

    /// Returns the number of the last block for which an Ethereum commit tx was sent and confirmed.
    pub async fn get_number_of_last_block_committed_on_eth(&mut self) -> Option<L1BatchNumber> {
        sqlx::query!(
            "SELECT number FROM l1_batches \
            LEFT JOIN eth_txs_history AS commit_tx \
                ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id) \
            WHERE commit_tx.confirmed_at IS NOT NULL \
            ORDER BY number DESC LIMIT 1"
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.number as u32))
    }

    /// Returns the number of the last block for which an Ethereum prove tx exists in the database.
    pub async fn get_last_l1_batch_with_prove_tx(&mut self) -> L1BatchNumber {
        let row = sqlx::query!(
            "SELECT COALESCE(MAX(number), 0) AS \"number!\" \
            FROM l1_batches \
            WHERE eth_prove_tx_id IS NOT NULL"
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        L1BatchNumber(row.number as u32)
    }

    /// Returns the number of the last block for which an Ethereum prove tx was sent and confirmed.
    pub async fn get_number_of_last_block_proven_on_eth(&mut self) -> Option<L1BatchNumber> {
        sqlx::query!(
            "SELECT number FROM l1_batches \
            LEFT JOIN eth_txs_history AS prove_tx \
                ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id) \
            WHERE prove_tx.confirmed_at IS NOT NULL \
            ORDER BY number DESC LIMIT 1"
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|record| L1BatchNumber(record.number as u32))
    }

    /// Returns the number of the last block for which an Ethereum execute tx was sent and confirmed.
    pub async fn get_number_of_last_block_executed_on_eth(&mut self) -> Option<L1BatchNumber> {
        sqlx::query!(
            "SELECT number FROM l1_batches \
            LEFT JOIN eth_txs_history as execute_tx \
                ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id) \
            WHERE execute_tx.confirmed_at IS NOT NULL \
            ORDER BY number DESC LIMIT 1"
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| L1BatchNumber(row.number as u32))
    }

    /// This method returns blocks that are confirmed on L1. That is, it doesn't wait for the proofs to be generated.
    pub async fn get_ready_for_dummy_proof_blocks(
        &mut self,
        limit: usize,
    ) -> Vec<BlockWithMetadata> {
        let raw_batches = sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches \
            WHERE eth_commit_tx_id IS NOT NULL AND eth_prove_tx_id IS NULL \
            ORDER BY number LIMIT $1",
            limit as i32
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        self.map_l1_batches(raw_batches).await
    }

    async fn map_l1_batches(&mut self, raw_batches: Vec<StorageBlock>) -> Vec<BlockWithMetadata> {
        let mut l1_batches = Vec::with_capacity(raw_batches.len());
        for raw_batch in raw_batches {
            let block = self
                .get_block_with_metadata(raw_batch)
                .await
                .expect("Block should be complete");
            l1_batches.push(block);
        }
        l1_batches
    }

    pub async fn set_skip_proof_for_l1_batch(&mut self, l1_batch_number: L1BatchNumber) {
        sqlx::query!(
            "UPDATE l1_batches SET skip_proof = TRUE WHERE number = $1",
            l1_batch_number.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    /// This method returns blocks that are committed on L1 and witness jobs for them are skipped.
    pub async fn get_skipped_for_proof_blocks(&mut self, limit: usize) -> Vec<BlockWithMetadata> {
        let last_proved_block_number = self.get_last_l1_batch_with_prove_tx().await;
        // Witness jobs can be processed out of order, so `WHERE l1_batches.number - row_number = $1`
        // is used to avoid having gaps in the list of blocks to send dummy proofs for.
        // We need to manually list all the columns in `l1_batches` table here - we cannot use `*`
        // because there is one extra column (`row_number`).
        let raw_batches = sqlx::query_as!(
            StorageBlock,
            "SELECT number, timestamp, is_finished, l1_tx_count, l2_tx_count, fee_account_address, \
                bloom, priority_ops_onchain_data, hash, parent_hash, commitment, compressed_write_logs, \
                compressed_contracts, eth_prove_tx_id, eth_commit_tx_id, eth_execute_tx_id, created_at, \
                updated_at, merkle_root_hash, l2_to_l1_logs, l2_to_l1_messages, predicted_commit_gas_cost, \
                predicted_prove_gas_cost, predicted_execute_gas_cost, initial_bootloader_heap_content, \
                used_contract_hashes, compressed_initial_writes, compressed_repeated_writes, \
                l2_l1_compressed_messages, l2_l1_merkle_root, l1_gas_price, l2_fair_gas_price, \
                rollup_last_leaf_index, zkporter_is_available, bootloader_code_hash, \
                default_aa_code_hash, base_fee_per_gas, aux_data_hash, pass_through_data_hash, \
                meta_parameters_hash, skip_proof, gas_per_pubdata_byte_in_block, gas_per_pubdata_limit \
            FROM \
            (SELECT l1_batches.*, row_number() OVER (ORDER BY number ASC) AS row_number \
                FROM l1_batches \
                WHERE eth_commit_tx_id IS NOT NULL \
                    AND l1_batches.skip_proof = TRUE \
                    AND l1_batches.number > $1 \
                ORDER BY number LIMIT $2\
            ) inn \
            WHERE number - row_number = $1",
            last_proved_block_number.0 as i32,
            limit as i32
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        self.map_l1_batches(raw_batches).await
    }

    pub async fn get_ready_for_execute_blocks(
        &mut self,
        limit: usize,
        max_l1_batch_timestamp_millis: Option<u64>,
    ) -> Vec<BlockWithMetadata> {
        let raw_batches = match max_l1_batch_timestamp_millis {
            None => sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE eth_prove_tx_id IS NOT NULL AND eth_execute_tx_id IS NULL \
                ORDER BY number LIMIT $1",
                limit as i32,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap(),

            Some(max_l1_batch_timestamp_millis) => {
                // Do not lose the precision here, otherwise we can skip some L1 batches.
                // Mostly needed for tests.
                let max_l1_batch_timestamp_seconds = max_l1_batch_timestamp_millis as f64 / 1_000.0;
                self.raw_ready_for_execute_blocks(max_l1_batch_timestamp_seconds, limit)
                    .await
            }
        };

        self.map_l1_batches(raw_batches).await
    }

    async fn raw_ready_for_execute_blocks(
        &mut self,
        max_l1_batch_timestamp_seconds: f64,
        limit: usize,
    ) -> Vec<StorageBlock> {
        // We need to find the first L1 batch that is supposed to be executed.
        // Here we ignore the time delay, so we just take the first L1 batch that is ready for execution.
        let row = sqlx::query!(
            "SELECT number FROM l1_batches \
            WHERE eth_prove_tx_id IS NOT NULL AND eth_execute_tx_id IS NULL \
            ORDER BY number LIMIT 1"
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();

        let Some(row) = row else { return vec![] };
        let expected_started_point = row.number;

        // Find the last L1 batch that is ready for execution.
        let row = sqlx::query!(
            "SELECT max(l1_batches.number) FROM l1_batches \
            JOIN eth_txs ON (l1_batches.eth_commit_tx_id = eth_txs.id) \
            JOIN eth_txs_history AS commit_tx ON (eth_txs.confirmed_eth_tx_history_id = commit_tx.id) \
            WHERE commit_tx.confirmed_at IS NOT NULL \
                AND eth_prove_tx_id IS NOT NULL \
                AND eth_execute_tx_id IS NULL \
                AND EXTRACT(epoch FROM commit_tx.confirmed_at) < $1",
            max_l1_batch_timestamp_seconds,
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        if let Some(max_ready_to_send_block) = row.max {
            // If we found at least one ready to execute batch then we can simply return all blocks between
            // the expected started point and the max ready to send block because we send them to the L1 sequentially.
            assert!(max_ready_to_send_block >= expected_started_point);
            sqlx::query_as!(
                StorageBlock,
                "SELECT * FROM l1_batches \
                WHERE number BETWEEN $1 AND $2 \
                ORDER BY number LIMIT $3",
                expected_started_point as i32,
                max_ready_to_send_block,
                limit as i32,
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
        } else {
            vec![]
        }
    }

    pub async fn get_ready_for_commit_blocks(
        &mut self,
        limit: usize,
        bootloader_hash: H256,
        default_aa_hash: H256,
    ) -> Vec<BlockWithMetadata> {
        let raw_batches = sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches \
            WHERE eth_commit_tx_id IS NULL \
                AND number != 0 \
                AND bootloader_code_hash = $1 AND default_aa_code_hash = $2 \
                AND commitment IS NOT NULL \
            ORDER BY number LIMIT $3",
            bootloader_hash.as_bytes(),
            default_aa_hash.as_bytes(),
            limit as i64,
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        self.map_l1_batches(raw_batches).await
    }

    pub async fn get_block_state_root(&mut self, number: L1BatchNumber) -> Option<H256> {
        sqlx::query!(
            "SELECT hash FROM l1_batches WHERE number = $1",
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .and_then(|row| row.hash)
        .map(|hash| H256::from_slice(&hash))
    }

    pub async fn get_block_state_root_and_timestamp(
        &mut self,
        number: L1BatchNumber,
    ) -> Option<(H256, u64)> {
        let row = sqlx::query!(
            "SELECT timestamp, hash FROM l1_batches WHERE number = $1",
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()?;

        Some((H256::from_slice(&row.hash?), row.timestamp as u64))
    }

    pub async fn get_newest_block_header(&mut self) -> L1BatchHeader {
        let last_block = sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches ORDER BY number DESC LIMIT 1"
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        last_block.into()
    }

    pub async fn get_block_metadata(&mut self, number: L1BatchNumber) -> Option<BlockWithMetadata> {
        let l1_batch: Option<StorageBlock> = sqlx::query_as!(
            StorageBlock,
            "SELECT * FROM l1_batches WHERE number = $1",
            number.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap();

        if let Some(bl) = l1_batch {
            self.get_block_with_metadata(bl).await
        } else {
            None
        }
    }

    pub async fn get_block_with_metadata(
        &mut self,
        storage_block: StorageBlock,
    ) -> Option<BlockWithMetadata> {
        let unsorted_factory_deps = self
            .get_l1_batch_factory_deps(L1BatchNumber(storage_block.number as u32))
            .await;
        let block_header = storage_block.clone().into();
        let block_metadata = storage_block.try_into().ok()?;

        Some(BlockWithMetadata::new(
            block_header,
            block_metadata,
            unsorted_factory_deps,
        ))
    }

    pub async fn get_l1_batch_factory_deps(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> HashMap<H256, Vec<u8>> {
        sqlx::query!(
            "SELECT bytecode_hash, bytecode FROM factory_deps \
            INNER JOIN miniblocks ON miniblocks.number = factory_deps.miniblock_number \
            WHERE miniblocks.l1_batch_number = $1",
            l1_batch_number.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap()
        .into_iter()
        .map(|row| (H256::from_slice(&row.bytecode_hash), row.bytecode))
        .collect()
    }

    /// Deletes all L1 batches from the storage so that the specified batch number is the last one left.
    pub async fn delete_l1_batches(&mut self, last_batch_to_keep: L1BatchNumber) {
        self.delete_l1_batches_inner(Some(last_batch_to_keep)).await;
    }

    async fn delete_l1_batches_inner(&mut self, last_batch_to_keep: Option<L1BatchNumber>) {
        let block_number = last_batch_to_keep.map_or(-1, |number| number.0 as i64);
        sqlx::query!("DELETE FROM l1_batches WHERE number > $1", block_number)
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    /// Deletes all miniblocks from the storage so that the specified miniblock number is the last one left.
    pub async fn delete_miniblocks(&mut self, last_miniblock_to_keep: MiniblockNumber) {
        self.delete_miniblocks_inner(Some(last_miniblock_to_keep))
            .await
    }

    async fn delete_miniblocks_inner(&mut self, last_miniblock_to_keep: Option<MiniblockNumber>) {
        let block_number = last_miniblock_to_keep.map_or(-1, |number| number.0 as i64);
        sqlx::query!("DELETE FROM miniblocks WHERE number > $1", block_number)
            .execute(self.storage.conn())
            .await
            .unwrap();
    }

    /// Returns sum of predicted gas costs or given block range.
    /// Panics if the sum doesn't fit into usize.
    pub async fn get_blocks_predicted_gas(
        &mut self,
        from_block: L1BatchNumber,
        to_block: L1BatchNumber,
        op_type: AggregatedActionType,
    ) -> u32 {
        let column_name = match op_type {
            AggregatedActionType::CommitBlocks => "predicted_commit_gas_cost",
            AggregatedActionType::PublishProofBlocksOnchain => "predicted_prove_gas_cost",
            AggregatedActionType::ExecuteBlocks => "predicted_execute_gas_cost",
        };
        let sql_query_str = format!(
            "SELECT COALESCE(SUM({column_name}), 0) AS sum FROM l1_batches \
             WHERE number BETWEEN $1 AND $2"
        );
        sqlx::query(&sql_query_str)
            .bind(from_block.0 as i64)
            .bind(to_block.0 as i64)
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .get::<BigDecimal, &str>("sum")
            .to_u32()
            .expect("Sum of predicted gas costs should fit into u32")
    }

    pub async fn update_predicted_block_commit_gas(
        &mut self,
        l1_batch_number: L1BatchNumber,
        predicted_gas_cost: u32,
    ) {
        sqlx::query!(
            "UPDATE l1_batches \
            SET predicted_commit_gas_cost = $2, updated_at = now() \
            WHERE number = $1",
            l1_batch_number.0 as i64,
            predicted_gas_cost as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    pub async fn get_miniblock_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Option<(MiniblockNumber, MiniblockNumber)> {
        let row = sqlx::query!(
            "SELECT MIN(miniblocks.number) as \"min?\", MAX(miniblocks.number) as \"max?\" \
            FROM miniblocks \
            WHERE l1_batch_number = $1",
            l1_batch_number.0 as i64
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        Some((
            MiniblockNumber(row.min? as u32),
            MiniblockNumber(row.max? as u32),
        ))
    }

    /// Returns `true` if there exists a non-sealed batch (i.e. there is one+ stored miniblock that isn't assigned
    /// to any batch yet).
    pub async fn pending_batch_exists(&mut self) -> bool {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(miniblocks.number) FROM miniblocks WHERE l1_batch_number IS NULL"
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap()
        .unwrap_or(0);

        count != 0
    }

    pub async fn get_last_l1_batch_number_with_witness_inputs(&mut self) -> L1BatchNumber {
        let row = sqlx::query!(
            "SELECT MAX(l1_batch_number) FROM witness_inputs \
            WHERE merkel_tree_paths_blob_url IS NOT NULL",
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        row.max
            .map(|l1_batch_number| L1BatchNumber(l1_batch_number as u32))
            .unwrap_or_default()
    }

    pub async fn get_l1_batches_with_blobs_in_db(&mut self, limit: u8) -> Vec<L1BatchNumber> {
        let rows = sqlx::query!(
            "SELECT l1_batch_number FROM witness_inputs \
            WHERE length(merkle_tree_paths) <> 0 \
            ORDER BY l1_batch_number DESC \
            LIMIT $1",
            limit as i32
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        rows.into_iter()
            .map(|row| L1BatchNumber(row.l1_batch_number as u32))
            .collect()
    }

    pub async fn get_merkle_tree_paths_blob_urls_to_be_cleaned(
        &mut self,
        limit: u8,
    ) -> Vec<(i64, String)> {
        let rows = sqlx::query!(
            "SELECT l1_batch_number, merkel_tree_paths_blob_url \
            FROM witness_inputs \
            WHERE status = 'successful' AND is_blob_cleaned = FALSE \
                AND merkel_tree_paths_blob_url is NOT NULL \
                AND updated_at < NOW() - INTERVAL '30 days' \
            LIMIT $1",
            limit as i32
        )
        .fetch_all(self.storage.conn())
        .await
        .unwrap();

        rows.into_iter()
            .map(|row| (row.l1_batch_number, row.merkel_tree_paths_blob_url.unwrap()))
            .collect()
    }

    pub async fn mark_gcs_blobs_as_cleaned(&mut self, l1_batch_numbers: &[i64]) {
        sqlx::query!(
            "UPDATE witness_inputs \
            SET is_blob_cleaned = TRUE \
            WHERE l1_batch_number = ANY($1)",
            l1_batch_numbers
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    // methods used for measuring Eth tx stage transition latencies
    // and emitting metrics base on these measured data
    pub async fn oldest_uncommitted_batch_timestamp(&mut self) -> Option<u64> {
        sqlx::query!(
            "SELECT timestamp FROM l1_batches \
            WHERE eth_commit_tx_id IS NULL AND number > 0 \
            ORDER BY number LIMIT 1",
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.timestamp as u64)
    }

    pub async fn oldest_unproved_batch_timestamp(&mut self) -> Option<u64> {
        sqlx::query!(
            "SELECT timestamp FROM l1_batches \
            WHERE eth_prove_tx_id IS NULL AND number > 0 \
            ORDER BY number LIMIT 1",
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.timestamp as u64)
    }

    pub async fn oldest_unexecuted_batch_timestamp(&mut self) -> Option<u64> {
        sqlx::query!(
            "SELECT timestamp FROM l1_batches \
            WHERE eth_execute_tx_id IS NULL AND number > 0 \
            ORDER BY number LIMIT 1",
        )
        .fetch_optional(self.storage.conn())
        .await
        .unwrap()
        .map(|row| row.timestamp as u64)
    }
}

/// These functions should only be used for tests.
impl BlocksDal<'_, '_> {
    // The actual l1 batch hash is only set by the metadata calculator.
    pub async fn set_l1_batch_hash(&mut self, batch_num: L1BatchNumber, hash: H256) {
        sqlx::query!(
            "UPDATE l1_batches SET hash = $1 WHERE number = $2",
            hash.as_bytes(),
            batch_num.0 as i64
        )
        .execute(self.storage.conn())
        .await
        .unwrap();
    }

    /// Deletes all miniblocks and L1 batches, including the genesis ones. Should only be used in tests.
    pub async fn delete_genesis(&mut self) {
        self.delete_miniblocks_inner(None).await;
        self.delete_l1_batches_inner(None).await;
    }
}

#[cfg(test)]
mod tests {
    use db_test_macro::db_test;
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::Address;

    use super::*;
    use crate::ConnectionPool;

    #[db_test(dal_crate)]
    async fn getting_predicted_gas(pool: ConnectionPool) {
        let mut conn = pool.access_storage().await;
        conn.blocks_dal().delete_l1_batches(L1BatchNumber(0)).await;

        let mut header = L1BatchHeader::new(
            L1BatchNumber(1),
            100,
            Address::default(),
            BaseSystemContractsHashes::default(),
        );
        let mut predicted_gas = BlockGasCount {
            commit: 2,
            prove: 3,
            execute: 10,
        };
        conn.blocks_dal()
            .insert_l1_batch(&header, predicted_gas)
            .await;

        header.number = L1BatchNumber(2);
        header.timestamp += 100;
        predicted_gas += predicted_gas;
        conn.blocks_dal()
            .insert_l1_batch(&header, predicted_gas)
            .await;

        let action_types_and_predicted_gas = [
            (AggregatedActionType::ExecuteBlocks, 10),
            (AggregatedActionType::CommitBlocks, 2),
            (AggregatedActionType::PublishProofBlocksOnchain, 3),
        ];
        for (action_type, expected_gas) in action_types_and_predicted_gas {
            let gas = conn
                .blocks_dal()
                .get_blocks_predicted_gas(L1BatchNumber(1), L1BatchNumber(1), action_type)
                .await;
            assert_eq!(gas, expected_gas);

            let gas = conn
                .blocks_dal()
                .get_blocks_predicted_gas(L1BatchNumber(2), L1BatchNumber(2), action_type)
                .await;
            assert_eq!(gas, 2 * expected_gas);

            let gas = conn
                .blocks_dal()
                .get_blocks_predicted_gas(L1BatchNumber(1), L1BatchNumber(2), action_type)
                .await;
            assert_eq!(gas, 3 * expected_gas);
        }
    }
}
