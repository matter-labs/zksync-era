//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use std::{
    ops,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use itertools::Itertools;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_multivm::{
    interface::{DeduplicatedWritesMetrics, TransactionExecutionResult, VmEvent},
    utils::{get_max_batch_gas_limit, get_max_gas_per_pubdata_byte, StorageWritesDeduplicator},
};
use zksync_shared_metrics::{BlockStage, L2BlockStage, APP_METRICS};
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

use crate::{
    io::seal_logic::l2_block_seal_subtasks::L2BlockSealProcess,
    metrics::{
        L1BatchSealStage, L2BlockSealStage, TxExecutionType, KEEPER_METRICS, L1_BATCH_METRICS,
        L2_BLOCK_METRICS,
    },
    updates::{L2BlockSealCommand, UpdatesManager},
};

pub mod l2_block_seal_subtasks;

impl UpdatesManager {
    /// Persists an L1 batch in the storage.
    /// This action includes a creation of an empty "fictive" L2 block that contains
    /// the events generated during the bootloader "tip phase". Returns updates for this fictive L2 block.
    pub(super) async fn seal_l1_batch(
        &self,
        pool: ConnectionPool<Core>,
        l2_legacy_shared_bridge_addr: Option<Address>,
        insert_protective_reads: bool,
    ) -> anyhow::Result<()> {
        let started_at = Instant::now();
        let finished_batch = self
            .l1_batch
            .finished
            .as_ref()
            .context("L1 batch is not actually finished")?;

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FictiveL2Block);
        // Seal fictive L2 block with last events and storage logs.
        let l2_block_command = self.seal_l2_block_command(
            l2_legacy_shared_bridge_addr,
            false, // fictive L2 blocks don't have txs, so it's fine to pass `false` here.
        );

        let mut connection = pool.connection_tagged("state_keeper").await?;
        let transaction = connection.start_transaction().await?;

        // We rely on the fact that fictive L2 block and L1 batch data is saved in the same transaction.
        let mut strategy = SealStrategy::Sequential(transaction);
        l2_block_command
            .seal_inner(&mut strategy, true)
            .await
            .context("failed persisting fictive L2 block")?;
        let SealStrategy::Sequential(mut transaction) = strategy else {
            panic!("Sealing L2 block should not mutate type of strategy");
        };
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::LogDeduplication);

        progress.observe(
            finished_batch
                .final_execution_state
                .deduplicated_storage_logs
                .len(),
        );

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l1_batch.executed_transactions);
        let (dedup_writes_count, dedup_reads_count) = log_query_write_read_counts(
            finished_batch
                .final_execution_state
                .deduplicated_storage_logs
                .iter(),
        );

        tracing::info!(
            "Sealing L1 batch {current_l1_batch_number} with timestamp {ts}, {total_tx_count} \
             ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {l2_to_l1_log_count} l2_l1_logs, \
             {event_count} events, {dedup_reads_count} deduped reads, \
             {dedup_writes_count} deduped writes",
            ts = display_timestamp(self.batch_timestamp()),
            total_tx_count = l1_tx_count + l2_tx_count,
            l2_to_l1_log_count = finished_batch
                .final_execution_state
                .user_l2_to_l1_logs
                .len(),
            event_count = finished_batch.final_execution_state.events.len(),
            current_l1_batch_number = self.l1_batch.number
        );

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertL1BatchHeader);
        let l2_to_l1_messages =
            VmEvent::extract_long_l2_to_l1_messages(&finished_batch.final_execution_state.events);
        let l1_batch = L1BatchHeader {
            number: self.l1_batch.number,
            timestamp: self.batch_timestamp(),
            priority_ops_onchain_data: self.l1_batch.priority_ops_onchain_data.clone(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: finished_batch
                .final_execution_state
                .user_l2_to_l1_logs
                .clone(),
            l2_to_l1_messages,
            bloom: Default::default(),
            used_contract_hashes: finished_batch
                .final_execution_state
                .used_contract_hashes
                .clone(),
            base_system_contracts_hashes: self.base_system_contract_hashes(),
            protocol_version: Some(self.protocol_version()),
            system_logs: finished_batch.final_execution_state.system_logs.clone(),
            pubdata_input: finished_batch.pubdata_input.clone(),
            fee_address: self.fee_account_address,
            batch_fee_input: self.batch_fee_input,
        };

        let final_bootloader_memory = finished_batch
            .final_bootloader_memory
            .clone()
            .unwrap_or_default();

        transaction
            .blocks_dal()
            .mark_l1_batch_as_sealed(
                &l1_batch,
                &final_bootloader_memory,
                &finished_batch.final_execution_state.storage_refunds,
                &finished_batch.final_execution_state.pubdata_costs,
                self.pending_execution_metrics().circuit_statistic,
            )
            .await?;
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::SetL1BatchNumberForL2Blocks);
        transaction
            .blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(self.l1_batch.number)
            .await?;
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::MarkTxsAsExecutedInL1Batch);
        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                self.l1_batch.number,
                &self.l1_batch.executed_transactions,
            )
            .await?;
        progress.observe(None);

        if insert_protective_reads {
            let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertProtectiveReads);
            let protective_reads: Vec<_> = finished_batch
                .final_execution_state
                .deduplicated_storage_logs
                .iter()
                .filter(|log_query| !log_query.is_write())
                .copied()
                .collect();
            transaction
                .storage_logs_dedup_dal()
                .insert_protective_reads(self.l1_batch.number, &protective_reads)
                .await?;
            progress.observe(protective_reads.len());
        }

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FilterWrittenSlots);
        let (initial_writes, all_writes_len): (Vec<_>, usize) =
            if let Some(state_diffs) = &finished_batch.state_diffs {
                let all_writes_len = state_diffs.len();

                (
                    state_diffs
                        .iter()
                        .filter(|diff| diff.is_write_initial())
                        .map(|diff| {
                            H256(StorageKey::raw_hashed_key(
                                &diff.address,
                                &u256_to_h256(diff.key),
                            ))
                        })
                        .collect(),
                    all_writes_len,
                )
            } else {
                let deduplicated_writes_hashed_keys_iter = finished_batch
                    .final_execution_state
                    .deduplicated_storage_logs
                    .iter()
                    .filter(|log| log.is_write())
                    .map(|log| log.key.hashed_key());

                let deduplicated_writes_hashed_keys: Vec<_> =
                    deduplicated_writes_hashed_keys_iter.clone().collect();
                let all_writes_len = deduplicated_writes_hashed_keys.len();
                let non_initial_writes = transaction
                    .storage_logs_dedup_dal()
                    .filter_written_slots(&deduplicated_writes_hashed_keys)
                    .await?;

                (
                    deduplicated_writes_hashed_keys_iter
                        .filter(|hashed_key| !non_initial_writes.contains(hashed_key))
                        .collect(),
                    all_writes_len,
                )
            };
        progress.observe(all_writes_len);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertInitialWrites);
        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(self.l1_batch.number, &initial_writes)
            .await?;
        progress.observe(initial_writes.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::CommitL1Batch);
        transaction.commit().await?;
        progress.observe(None);

        let writes_metrics = self.storage_writes_deduplicator.metrics();
        // Sanity check metrics.
        anyhow::ensure!(
            all_writes_len
                == writes_metrics.initial_storage_writes + writes_metrics.repeated_storage_writes,
            "Results of in-flight and common deduplications are mismatched"
        );

        self.report_l1_batch_metrics(started_at, &writes_metrics);
        Ok(())
    }

    fn report_l1_batch_metrics(
        &self,
        started_at: Instant,
        writes_metrics: &DeduplicatedWritesMetrics,
    ) {
        L1_BATCH_METRICS
            .initial_writes
            .observe(writes_metrics.initial_storage_writes);
        L1_BATCH_METRICS
            .repeated_writes
            .observe(writes_metrics.repeated_storage_writes);
        L1_BATCH_METRICS
            .transactions_in_l1_batch
            .observe(self.l1_batch.executed_transactions.len());

        let batch_timestamp = self.batch_timestamp();
        let l1_batch_latency =
            unix_timestamp_ms().saturating_sub(batch_timestamp * 1_000) as f64 / 1_000.0;
        APP_METRICS.block_latency[&BlockStage::Sealed]
            .observe(Duration::from_secs_f64(l1_batch_latency));

        let elapsed = started_at.elapsed();
        L1_BATCH_METRICS.sealed_time.observe(elapsed);
        tracing::debug!("Sealed L1 batch {} in {elapsed:?}", self.l1_batch.number);
    }
}

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
    pub(super) async fn seal(&self, pool: ConnectionPool<Core>) -> anyhow::Result<()> {
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
            ts = display_timestamp(self.l2_block.timestamp()),
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.l2_block.events.len()
        );

        // Run sub-tasks in parallel.
        L2BlockSealProcess::run_subtasks(self, strategy).await?;

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::CalculateLogsBloom, is_fictive);
        let iter = self.l2_block.events.iter().flat_map(|event| {
            event
                .indexed_topics
                .iter()
                .map(|topic| BloomInput::Raw(topic.as_bytes()))
                .chain([BloomInput::Raw(event.address.as_bytes())])
        });
        let logs_bloom = build_bloom(iter);
        progress.observe(Some(self.l2_block.events.len()));

        // Seal block header at the last step.
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertL2BlockHeader, is_fictive);
        let definite_vm_version = self
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined)
            .into();

        let l2_block_header = L2BlockHeader {
            number: self.l2_block.number,
            timestamp: self.l2_block.timestamp(),
            hash: self.l2_block.get_l2_block_hash(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            fee_account_address: self.fee_account_address,
            base_fee_per_gas: self.base_fee_per_gas,
            batch_fee_input: self.fee_input,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            protocol_version: self.protocol_version,
            gas_per_pubdata_limit: get_max_gas_per_pubdata_byte(definite_vm_version),
            virtual_blocks: self.l2_block.virtual_blocks,
            gas_limit: get_max_batch_gas_limit(definite_vm_version),
            logs_bloom,
            pubdata_params: self.pubdata_params,
        };

        let mut connection = strategy.connection().await?;
        connection
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await?;
        progress.observe(None);

        // Report metrics.
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ReportTxMetrics, is_fictive);
        self.report_transaction_metrics();
        progress.observe(Some(self.l2_block.executed_transactions.len()));

        self.report_l2_block_metrics(started_at);
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
        StorageWritesDeduplicator::deduplicate_logs(
            self.l2_block
                .storage_logs
                .iter()
                .filter(|log| log.log.is_write()),
        )
    }

    fn transaction(&self, index: usize) -> &Transaction {
        let tx_result = &self.l2_block.executed_transactions[index - self.first_tx_index];
        &tx_result.transaction
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
        let grouped_entries = entries.iter().chunk_by(|&entry| tx_location(entry));
        let grouped_entries = grouped_entries.into_iter().map(|(tx_index, entries)| {
            let tx_hash = if is_fictive {
                assert_eq!(tx_index as usize, self.first_tx_index);
                H256::zero()
            } else {
                self.transaction(tx_index as usize).hash()
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

    fn report_transaction_metrics(&self) {
        const SLOW_INCLUSION_DELAY: Duration = Duration::from_secs(600);

        if self.pre_insert_txs {
            // This I/O logic is running on the EN. The reported metrics / logs would be meaningless:
            //
            // - If `received_timestamp_ms` are copied from the main node, they can be far in the past (especially during the initial EN sync).
            //   We would falsely classify a lot of transactions as slow.
            // - If `received_timestamp_ms` are overridden with the current timestamp as when persisting transactions,
            //   the observed transaction latencies would always be extremely close to zero.
            return;
        }

        for tx in &self.l2_block.executed_transactions {
            let inclusion_delay =
                unix_timestamp_ms().saturating_sub(tx.transaction.received_timestamp_ms);
            let inclusion_delay = Duration::from_millis(inclusion_delay);
            if inclusion_delay > SLOW_INCLUSION_DELAY {
                tracing::info!(
                    tx_hash = hex::encode(tx.hash),
                    inclusion_delay_ms = inclusion_delay.as_millis(),
                    received_timestamp_ms = tx.transaction.received_timestamp_ms,
                    "Transaction spent >{SLOW_INCLUSION_DELAY:?} in mempool before being included in an L2 block"
                );
            }
            KEEPER_METRICS.transaction_inclusion_delay
                [&TxExecutionType::from_is_l1(tx.transaction.is_l1())]
                .observe(inclusion_delay)
        }
    }

    fn report_l2_block_metrics(&self, started_at: Instant) {
        let l2_block_number = self.l2_block.number;

        L2_BLOCK_METRICS
            .transactions_in_miniblock
            .observe(self.l2_block.executed_transactions.len());
        L2_BLOCK_METRICS.sealed_time.observe(started_at.elapsed());

        let l2_block_latency =
            unix_timestamp_ms().saturating_sub(self.l2_block.timestamp_ms) as f64 / 1_000.0;
        let stage = &L2BlockStage::Sealed;
        APP_METRICS.miniblock_latency[stage].observe(Duration::from_secs_f64(l2_block_latency));
        APP_METRICS.miniblock_number[stage].set(l2_block_number.0.into());

        tracing::debug!(
            "Sealed L2 block #{l2_block_number} in {:?}",
            started_at.elapsed()
        );
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
