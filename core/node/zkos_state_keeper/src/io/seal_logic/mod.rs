//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use std::{
    ops,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use itertools::Itertools;
use zk_ee::common_structs::L2ToL1Log;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_mini_merkle_tree::MiniMerkleTree;
use zksync_types::{
    block::{build_bloom, L1BatchHeader, L2BlockHeader},
    helpers::unix_timestamp_ms,
    l2_to_l1_log::UserL2ToL1Log,
    tx::IncludedTxLocation,
    u256_to_h256,
    utils::display_timestamp,
    Address, BloomInput, ExecuteTransactionCommon, ProtocolVersionId, StorageKey, StorageLog,
    Transaction, H256,
};
use zksync_vm_interface::{TransactionExecutionResult, VmEvent};

use crate::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
    updates::{BlockSealCommand, UpdatesManager},
};

pub mod l2_block_seal_subtasks;

impl BlockSealCommand {
    /// Seals an L2 block with the given number.
    pub async fn seal(&self, pool: ConnectionPool<Core>) -> anyhow::Result<()> {
        let started_at = Instant::now();
        self.ensure_valid_l2_block()
            .context("L2 block is invalid")?;

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.inner.executed_transactions);
        tracing::info!(
            "Sealing L2 block {l2_block_number} with timestamp {ts} (L1 batch {l1_batch_number}) \
             with {total_tx_count} ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {event_count} events",
            l2_block_number = self.inner.l2_block_number,
            l1_batch_number = self.inner.l1_batch_number,
            ts = display_timestamp(self.inner.timestamp),
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.inner.events.len()
        );

        // Run sub-tasks in parallel.
        L2BlockSealProcess::run_subtasks(self, &pool).await?;

        // Synchronous part.
        let mut connection = pool.connection_tagged("state_keeper").await?;
        let mut transaction = connection.start_transaction().await?;

        let events_iter = self.inner.events.iter().flat_map(|event| {
            event
                .indexed_topics
                .iter()
                .map(|topic| BloomInput::Raw(topic.as_bytes()))
                .chain([BloomInput::Raw(event.address.as_bytes())])
        });
        let logs_bloom = build_bloom(events_iter);

        // Seal l2 block header and l1 block header within the same DB transaction.
        let l2_block_header = L2BlockHeader {
            number: self.inner.l2_block_number,
            timestamp: self.inner.timestamp,
            hash: self
                .inner
                .block_header
                .as_ref()
                .context(format!(
                    "Missing header for block ${}",
                    self.inner.l2_block_number
                ))?
                .hash()
                .into(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            fee_account_address: self.inner.fee_account_address,
            base_fee_per_gas: self.inner.base_fee_per_gas,
            batch_fee_input: self.inner.batch_fee_input,
            base_system_contracts_hashes: Default::default(), // ??
            protocol_version: Some(self.inner.protocol_version),
            gas_per_pubdata_limit: u64::MAX, // TODO: remove
            virtual_blocks: 1,
            gas_limit: self.inner.gas_limit,
            logs_bloom,
            pubdata_params: Default::default(), // ???
        };

        transaction
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await?;

        tracing::info!(
            "Sealing L1 batch {current_l1_batch_number} with timestamp {ts}, {total_tx_count} \
             ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {l2_to_l1_log_count} l2_l1_logs, \
             {event_count} events",
            ts = display_timestamp(self.inner.timestamp),
            total_tx_count = l1_tx_count + l2_tx_count,
            l2_to_l1_log_count = self.inner.user_l2_to_l1_logs.len(),
            event_count = self.inner.events.len(),
            current_l1_batch_number = self.inner.l1_batch_number
        );

        let l1_batch = L1BatchHeader {
            number: self.inner.l1_batch_number,
            timestamp: self.inner.timestamp,
            priority_ops_onchain_data: vec![],
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: vec![], //todo: l2_to_l1_logs are not saved yet
            l2_to_l1_messages: vec![],
            bloom: Default::default(),
            used_contract_hashes: vec![],
            base_system_contracts_hashes: Default::default(),
            protocol_version: Some(self.inner.protocol_version),
            system_logs: vec![],
            pubdata_input: Some(self.inner.block_pubdata.clone().expect("Block pubdata must be set for sealed zkos blocks")),
            fee_address: self.inner.fee_account_address,
            batch_fee_input: self.inner.batch_fee_input,
        };

        let final_bootloader_memory = vec![];

        // todo: this is temporarily saved in state keeper - consider moving to a separate component
        let encoded_l2_l1_logs = self
            .inner
            .user_l2_to_l1_logs
            .iter()
            .map(|log| {
                log.encode()
            });

        // todo - extract constant
        let l2_l1_merkle_root = MiniMerkleTree::new(encoded_l2_l1_logs, Some(2 << 14))
            .merkle_root();

        transaction
            .blocks_dal()
            .insert_l2_l1_message_root(
                self.inner.l1_batch_number,
                l2_l1_merkle_root,
            )
            .await?;

        // todo: tree root hash is temporarily saved in state keeper - remove it and make tree save it instead

        transaction
            .blocks_dal()
            .save_l1_batch_tree_data(
                self.inner.l1_batch_number,
                &self.inner.tree_data.expect("zkos state keeper should fill block hash"),
            )
            .await?;

        transaction
            .blocks_dal()
            .mark_l1_batch_as_sealed(
                &l1_batch,
                &final_bootloader_memory,
                &[],
                &[],
                Default::default(),
            )
            .await?;

        transaction
            .blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(self.inner.l1_batch_number)
            .await?;

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                self.inner.l1_batch_number,
                &self.inner.executed_transactions,
            )
            .await?;

        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(self.inner.l1_batch_number, &self.initial_writes)
            .await?;

        transaction.commit().await?;

        Ok(())
    }

    /// Performs several sanity checks to make sure that the L2 block is valid.
    fn ensure_valid_l2_block(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.inner.executed_transactions.is_empty(),
            "non-fictive L2 block must have at least one transaction"
        );

        let tx_index_range = 0..self.inner.executed_transactions.len();

        for event in &self.inner.events {
            let tx_index = event.location.1 as usize;
            anyhow::ensure!(
                tx_index_range.contains(&tx_index),
                "event transaction index {tx_index} is outside of the expected range {tx_index_range:?}"
            );
        }
        Ok(())
    }

    fn extract_deduplicated_write_logs(&self) -> Vec<StorageLog> {
        self.inner.storage_logs.clone()
    }

    fn transaction_hash(&self, index: usize) -> H256 {
        let tx_result = &self.inner.executed_transactions[index];
        tx_result.hash
    }

    fn extract_events(&self) -> Vec<(IncludedTxLocation, Vec<&VmEvent>)> {
        self.group_by_tx_location(&self.inner.events, |event| event.location.1)
    }

    // todo: this shouldn't be needed - VM returns events and l2_to_l1_logs already grouped
    fn group_by_tx_location<'a, T>(
        &'a self,
        entries: &'a [T],
        tx_location: impl Fn(&T) -> u32,
    ) -> Vec<(IncludedTxLocation, Vec<&'a T>)> {
        let grouped_entries = entries.iter().group_by(|&entry| tx_location(entry));
        let grouped_entries = grouped_entries.into_iter().map(|(tx_index, entries)| {
            let tx_hash = self.transaction_hash(tx_index as usize);

            let location = IncludedTxLocation {
                tx_hash,
                tx_index_in_l2_block: tx_index,
            };
            (location, entries.collect())
        });
        grouped_entries.collect()
    }

    fn extract_user_l2_to_l1_logs(&self) -> Vec<(IncludedTxLocation, Vec<&L2ToL1Log>)> {
        Default::default()
        // todo: l2_to_l1_logs are not saved for now
        // self.group_by_tx_location(&self.inner.user_l2_to_l1_logs, |log| {
        //     u32::from(log.tx_number_in_block)
        // })
    }
}

fn l1_l2_tx_count(executed_transactions: &[TransactionExecutionResult]) -> (usize, usize) {
    let mut l1_tx_count = 0;
    let mut l2_tx_count = 0;

    for tx in executed_transactions {
        if matches!(tx.transaction.common_data, ExecuteTransactionCommon::L1(_)) {
            l1_tx_count += 1;
        } else {
            l2_tx_count += 1;
        }
    }
    (l1_tx_count, l2_tx_count)
}

fn log_query_write_read_counts<'a>(logs: impl Iterator<Item = &'a StorageLog>) -> (usize, usize) {
    let mut reads_count = 0;
    let mut writes_count = 0;

    for log in logs {
        if log.is_write() {
            writes_count += 1;
        } else {
            reads_count += 1;
        }
    }
    (writes_count, reads_count)
}
