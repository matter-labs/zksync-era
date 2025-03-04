//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use std::{
    ops,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use itertools::Itertools;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
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
    updates::{L2BlockSealCommand, UpdatesManager},
};

pub mod l2_block_seal_subtasks;

#[derive(Debug)]
pub enum SealStrategy<'pool> {
    Sequential(Connection<'pool, Core>),
    Parallel(&'pool ConnectionPool<Core>),
}

// As opposed to `Cow` from `std`; a union of an owned type and a mutable ref to it
enum Goat<'a, T> {
    Owned(T),
    Borrowed(&'a mut T),
}

impl<T> ops::Deref for Goat<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(value) => value,
            Self::Borrowed(value) => value,
        }
    }
}

impl<T> ops::DerefMut for Goat<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Owned(value) => value,
            Self::Borrowed(value) => value,
        }
    }
}

impl<'pool> SealStrategy<'pool> {
    async fn connection(&mut self) -> anyhow::Result<Goat<'_, Connection<'pool, Core>>> {
        Ok(match self {
            Self::Parallel(pool) => Goat::Owned(pool.connection_tagged("state_keeper").await?),
            Self::Sequential(conn) => Goat::Borrowed(conn),
        })
    }
}

impl L2BlockSealCommand {
    pub async fn seal(&self, pool: ConnectionPool<Core>) -> anyhow::Result<()> {
        let l2_block_number = self.l2_block.number;
        self.seal_inner(&mut SealStrategy::Parallel(&pool), false)
            .await
            .with_context(|| format!("failed sealing L2 block #{l2_block_number}"))
    }

    /// Seals an L2 block with the given number.
    ///
    /// If `is_fictive` flag is set to true, then it is assumed that we should seal a fictive L2 block
    /// with no transactions in it. It is needed because there might be some storage logs / events
    /// that are created after the last processed tx in the L1 batch: after the last transaction is processed,
    /// the bootloader enters the "tip" phase in which it can still generate events (e.g.,
    /// one for sending fees to the operator).
    async fn seal_inner(
        &self,
        strategy: &mut SealStrategy<'_>,
        is_fictive: bool,
    ) -> anyhow::Result<()> {
        let started_at = Instant::now();
        self.ensure_valid_l2_block(is_fictive)
            .context("L2 block is invalid")?;

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l2_block.executed_transactions);
        tracing::info!(
            "Sealing L2 block {l2_block_number} with timestamp {ts} (L1 batch {l1_batch_number}) \
             with {total_tx_count} ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {event_count} events",
            l2_block_number = self.l2_block.number,
            l1_batch_number = self.l1_batch_number,
            ts = display_timestamp(self.timestamp),
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.l2_block.events.len()
        );

        // Run sub-tasks in parallel.
        L2BlockSealProcess::run_subtasks(self, strategy).await?;

        Ok(())
    }

    /// Performs several sanity checks to make sure that the L2 block is valid.
    fn ensure_valid_l2_block(&self, is_fictive: bool) -> anyhow::Result<()> {
        if is_fictive {
            anyhow::ensure!(
                self.l2_block.executed_transactions.is_empty(),
                "fictive L2 block must not have transactions"
            );
        } else {
            anyhow::ensure!(
                !self.l2_block.executed_transactions.is_empty(),
                "non-fictive L2 block must have at least one transaction"
            );
        }

        let first_tx_index = self.first_tx_index;
        let next_tx_index = first_tx_index + self.l2_block.executed_transactions.len();
        let tx_index_range = if is_fictive {
            next_tx_index..(next_tx_index + 1)
        } else {
            first_tx_index..next_tx_index
        };

        for event in &self.l2_block.events {
            let tx_index = event.location.1 as usize;
            anyhow::ensure!(
                tx_index_range.contains(&tx_index),
                "event transaction index {tx_index} is outside of the expected range {tx_index_range:?}"
            );
        }
        Ok(())
    }

    fn extract_deduplicated_write_logs(&self) -> Vec<StorageLog> {
        self.l2_block.storage_logs.clone()
    }

    fn transaction_hash(&self, index: usize) -> H256 {
        let tx_result = &self.l2_block.executed_transactions[index - self.first_tx_index];
        tx_result.hash
    }

    fn extract_events(&self, is_fictive: bool) -> Vec<(IncludedTxLocation, Vec<&VmEvent>)> {
        self.group_by_tx_location(&self.l2_block.events, is_fictive, |event| event.location.1)
    }

    fn group_by_tx_location<'a, T>(
        &'a self,
        entries: &'a [T],
        is_fictive: bool,
        tx_location: impl Fn(&T) -> u32,
    ) -> Vec<(IncludedTxLocation, Vec<&'a T>)> {
        let grouped_entries = entries.iter().group_by(|&entry| tx_location(entry));
        let grouped_entries = grouped_entries.into_iter().map(|(tx_index, entries)| {
            let tx_hash = if is_fictive {
                assert_eq!(tx_index as usize, self.first_tx_index);
                H256::zero()
            } else {
                self.transaction_hash(tx_index as usize)
            };

            let location = IncludedTxLocation {
                tx_hash,
                tx_index_in_l2_block: tx_index - self.first_tx_index as u32,
            };
            (location, entries.collect())
        });
        grouped_entries.collect()
    }

    fn extract_user_l2_to_l1_logs(
        &self,
        is_fictive: bool,
    ) -> Vec<(IncludedTxLocation, Vec<&UserL2ToL1Log>)> {
        self.group_by_tx_location(&self.l2_block.user_l2_to_l1_logs, is_fictive, |log| {
            u32::from(log.0.tx_number_in_block)
        })
    }

    fn is_l2_block_fictive(&self) -> bool {
        self.l2_block.executed_transactions.is_empty()
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

impl UpdatesManager {
    /// Persists an L1 batch in the storage.
    pub async fn seal_l1_batch(&self, pool: ConnectionPool<Core>) -> anyhow::Result<()> {
        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l2_block.executed_transactions);

        let mut connection = pool.connection_tagged("state_keeper").await?;
        let mut transaction = connection.start_transaction().await?;

        let events_iter = self.l2_block.events.iter().flat_map(|event| {
            event
                .indexed_topics
                .iter()
                .map(|topic| BloomInput::Raw(topic.as_bytes()))
                .chain([BloomInput::Raw(event.address.as_bytes())])
        });
        let logs_bloom = build_bloom(events_iter);

        // Seal l2 block header and l1 block header within the same DB transaction.
        let l2_block_header = L2BlockHeader {
            number: self.l2_block.number,
            timestamp: self.timestamp(),
            hash: H256::zero(), // ?
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            fee_account_address: self.fee_account_address,
            base_fee_per_gas: self.base_fee_per_gas,
            batch_fee_input: self.batch_fee_input,
            base_system_contracts_hashes: Default::default(), // ??
            protocol_version: Some(self.protocol_version()),
            gas_per_pubdata_limit: u64::MAX, // TODO: remove
            virtual_blocks: 1,
            gas_limit: self.gas_limit,
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
            ts = display_timestamp(self.timestamp()),
            total_tx_count = l1_tx_count + l2_tx_count,
            l2_to_l1_log_count = self.l2_block.user_l2_to_l1_logs.len(),
            event_count = self.l2_block.events.len(),
            current_l1_batch_number = self.l1_batch_number
        );

        let l1_batch = L1BatchHeader {
            number: self.l1_batch_number,
            timestamp: self.timestamp(),
            priority_ops_onchain_data: vec![],
            l1_tx_count: 0, // TODO: l1_tx_count as u16
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: self.l2_block.user_l2_to_l1_logs.clone(),
            l2_to_l1_messages: vec![],
            bloom: Default::default(),
            used_contract_hashes: vec![],
            base_system_contracts_hashes: Default::default(),
            protocol_version: Some(self.protocol_version()),
            system_logs: vec![],
            pubdata_input: None,
            fee_address: self.fee_account_address,
            batch_fee_input: self.batch_fee_input,
        };

        let final_bootloader_memory = vec![];

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
            .mark_l2_blocks_as_executed_in_l1_batch(self.l1_batch_number)
            .await?;

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                self.l1_batch_number,
                &self.l2_block.executed_transactions,
            )
            .await?;

        let initial_writes = self.get_initial_writes(&mut transaction).await?;
        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(self.l1_batch_number, &initial_writes)
            .await?;

        transaction.commit().await?;

        Ok(())
    }
}
