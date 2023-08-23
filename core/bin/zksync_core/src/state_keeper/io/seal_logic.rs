//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use itertools::Itertools;

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use vm::{
    vm_with_bootloader::{
        get_bootloader_memory, BlockContextMode, DerivedBlockContext, TxExecutionMode,
    },
    VmBlockResult,
};
use zksync_config::constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_dal::StorageProcessor;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    event::{extract_added_tokens, extract_long_l2_to_l1_messages},
    l2_to_l1_log::L2ToL1Log,
    tx::{
        tx_execution_info::DeduplicatedWritesMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    zk_evm::aux_structures::LogQuery,
    zkevm_test_harness::witness::sort_storage_access::sort_storage_access_queries,
    AccountTreeId, Address, ExecuteTransactionCommon, L1BatchNumber, MiniblockNumber, StorageKey,
    StorageLog, StorageLogQuery, StorageValue, Transaction, VmEvent, H256, U256,
};
use zksync_utils::{miniblock_hash, time::millis_since_epoch, u256_to_h256};

use crate::state_keeper::{
    extractors,
    io::common::set_missing_initial_writes_indices,
    updates::{L1BatchUpdates, MiniblockSealCommand, UpdatesManager},
};

#[derive(Debug, Clone, Copy)]
struct SealProgressMetricNames {
    target: &'static str,
    stage_latency: &'static str,
    entity_count: &'static str,
    latency_per_unit: &'static str,
}

impl SealProgressMetricNames {
    const L1_BATCH: Self = Self {
        target: "L1 batch",
        stage_latency: "server.state_keeper.l1_batch.sealed_time_stage",
        entity_count: "server.state_keeper.l1_batch.sealed_entity_count",
        latency_per_unit: "server.state_keeper.l1_batch.sealed_entity_per_unit",
    };

    const MINIBLOCK: Self = Self {
        target: "miniblock",
        stage_latency: "server.state_keeper.miniblock.sealed_time_stage",
        entity_count: "server.state_keeper.miniblock.sealed_entity_count",
        latency_per_unit: "server.state_keeper.miniblock.sealed_entity_per_unit",
    };
}

/// Tracking progress of L1 batch sealing.
#[derive(Debug)]
struct SealProgress {
    metric_names: SealProgressMetricNames,
    is_fictive: Option<bool>,
    stage_start: Instant,
}

impl SealProgress {
    fn for_l1_batch() -> Self {
        Self {
            metric_names: SealProgressMetricNames::L1_BATCH,
            is_fictive: None,
            stage_start: Instant::now(),
        }
    }

    fn for_miniblock(is_fictive: bool) -> Self {
        Self {
            metric_names: SealProgressMetricNames::MINIBLOCK,
            is_fictive: Some(is_fictive),
            stage_start: Instant::now(),
        }
    }

    fn end_stage(&mut self, stage: &'static str, count: Option<usize>) {
        const MIN_STAGE_DURATION_TO_REPORT: Duration = Duration::from_millis(10);

        let elapsed = self.stage_start.elapsed();
        if elapsed > MIN_STAGE_DURATION_TO_REPORT {
            let target = self.metric_names.target;
            vlog::debug!("{target} execution stage {stage} took {elapsed:?} with count {count:?}");
        }

        let (l1_batch_labels, miniblock_labels);
        let labels: &[_] = if let Some(is_fictive) = self.is_fictive {
            let is_fictive_label = if is_fictive { "true" } else { "false" };
            miniblock_labels = [("is_fictive", is_fictive_label), ("stage", stage)];
            &miniblock_labels
        } else {
            l1_batch_labels = [("stage", stage)];
            &l1_batch_labels
        };
        metrics::histogram!(self.metric_names.stage_latency, elapsed, labels);

        if let Some(count) = count {
            metrics::histogram!(self.metric_names.entity_count, count as f64, labels);
            if count > 0 {
                metrics::histogram!(
                    self.metric_names.latency_per_unit,
                    elapsed.div_f64(count as f64),
                    labels
                );
            }
        }
        self.stage_start = Instant::now();
    }
}

impl UpdatesManager {
    /// Persists an L1 batch in the storage.
    /// This action includes a creation of an empty "fictive" miniblock that contains
    /// the events generated during the bootloader "tip phase".
    pub(crate) async fn seal_l1_batch(
        mut self,
        storage: &mut StorageProcessor<'_>,
        current_miniblock_number: MiniblockNumber,
        current_l1_batch_number: L1BatchNumber,
        block_result: VmBlockResult,
        block_context: DerivedBlockContext,
        l2_erc20_bridge_addr: Address,
    ) {
        let started_at = Instant::now();
        let mut progress = SealProgress::for_l1_batch();
        let mut transaction = storage.start_transaction().await;

        // The vm execution was paused right after the last transaction was executed.
        // There is some post-processing work that the VM needs to do before the block is fully processed.
        let VmBlockResult {
            full_result,
            block_tip_result,
        } = block_result;
        assert!(
            full_result.revert_reason.is_none(),
            "VM must not revert when finalizing block. Revert reason: {:?}",
            full_result.revert_reason
        );
        progress.end_stage("vm_finalization", None);

        self.extend_from_fictive_transaction(block_tip_result.logs);
        // Seal fictive miniblock with last events and storage logs.
        let miniblock_command = self.seal_miniblock_command(
            current_l1_batch_number,
            current_miniblock_number,
            l2_erc20_bridge_addr,
        );
        miniblock_command.seal_inner(&mut transaction, true).await;
        progress.end_stage("fictive_miniblock", None);

        let (_, deduped_log_queries) = sort_storage_access_queries(
            full_result
                .storage_log_queries
                .iter()
                .map(|log| &log.log_query),
        );
        progress.end_stage("log_deduplication", Some(deduped_log_queries.len()));

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l1_batch.executed_transactions);
        let (writes_count, reads_count) =
            storage_log_query_write_read_counts(&full_result.storage_log_queries);
        let (dedup_writes_count, dedup_reads_count) =
            log_query_write_read_counts(deduped_log_queries.iter());
        vlog::info!(
            "Sealing L1 batch {current_l1_batch_number} with {total_tx_count} \
             ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {l2_to_l1_log_count} l2_l1_logs, \
             {event_count} events, {reads_count} reads ({dedup_reads_count} deduped), \
             {writes_count} writes ({dedup_writes_count} deduped)",
            total_tx_count = l1_tx_count + l2_tx_count,
            l2_to_l1_log_count = full_result.l2_to_l1_logs.len(),
            event_count = full_result.events.len()
        );

        let (prev_hash, prev_timestamp) =
            extractors::wait_for_prev_l1_batch_params(&mut transaction, current_l1_batch_number)
                .await;
        let timestamp = block_context.context.block_timestamp;
        assert!(
            prev_timestamp < timestamp,
            "Cannot seal L1 batch #{}: Timestamp of previous L1 batch ({}) >= provisional L1 batch timestamp ({}), \
             meaning that L1 batch will be rejected by the bootloader",
            current_l1_batch_number,
            extractors::display_timestamp(prev_timestamp),
            extractors::display_timestamp(timestamp)
        );

        let l1_batch = L1BatchHeader {
            number: current_l1_batch_number,
            is_finished: true,
            timestamp,
            fee_account_address: block_context.context.operator_address,
            priority_ops_onchain_data: self.l1_batch.priority_ops_onchain_data.clone(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: full_result.l2_to_l1_logs,
            l2_to_l1_messages: extract_long_l2_to_l1_messages(&full_result.events),
            bloom: Default::default(),
            used_contract_hashes: full_result.used_contract_hashes,
            base_fee_per_gas: block_context.base_fee,
            l1_gas_price: self.l1_gas_price(),
            l2_fair_gas_price: self.fair_l2_gas_price(),
            base_system_contracts_hashes: self.base_system_contract_hashes(),
            protocol_version: Some(self.protocol_version()),
        };

        let block_context_properties = BlockContextMode::NewBlock(block_context, prev_hash);
        let initial_bootloader_contents =
            Self::initial_bootloader_memory(&self.l1_batch, block_context_properties);

        transaction
            .blocks_dal()
            .insert_l1_batch(
                &l1_batch,
                &initial_bootloader_contents,
                self.l1_batch.l1_gas_count,
            )
            .await;
        progress.end_stage("insert_l1_batch_header", None);

        transaction
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(current_l1_batch_number)
            .await;
        progress.end_stage("set_l1_batch_number_for_miniblocks", None);

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                current_l1_batch_number,
                &self.l1_batch.executed_transactions,
            )
            .await;
        progress.end_stage("mark_txs_as_executed_in_l1_batch", None);

        let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
            .into_iter()
            .partition(|log_query| log_query.rw_flag);
        transaction
            .storage_logs_dedup_dal()
            .insert_protective_reads(current_l1_batch_number, &protective_reads)
            .await;
        progress.end_stage("insert_protective_reads", Some(protective_reads.len()));

        let deduplicated_writes_hashed_keys: Vec<_> = deduplicated_writes
            .iter()
            .map(|log| {
                H256(StorageKey::raw_hashed_key(
                    &log.address,
                    &u256_to_h256(log.key),
                ))
            })
            .collect();
        let non_initial_writes = transaction
            .storage_logs_dedup_dal()
            .filter_written_slots(&deduplicated_writes_hashed_keys)
            .await;
        progress.end_stage("filter_written_slots", Some(deduplicated_writes.len()));

        let written_storage_keys: Vec<_> = deduplicated_writes
            .iter()
            .filter_map(|log| {
                let key = StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
                (!non_initial_writes.contains(&key.hashed_key())).then_some(key)
            })
            .collect();

        // One-time migration completion for initial writes' indices.
        set_missing_initial_writes_indices(&mut transaction).await;
        progress.end_stage("set_missing_initial_writes_indices", None);

        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(current_l1_batch_number, &written_storage_keys)
            .await;
        progress.end_stage("insert_initial_writes", Some(deduplicated_writes.len()));

        transaction.commit().await;
        progress.end_stage("commit_l1_batch", None);

        let writes_metrics = self.storage_writes_deduplicator.metrics();
        // Sanity check metrics.
        assert_eq!(
            deduplicated_writes.len(),
            writes_metrics.initial_storage_writes + writes_metrics.repeated_storage_writes,
            "Results of in-flight and common deduplications are mismatched"
        );

        self.report_l1_batch_metrics(
            started_at,
            current_l1_batch_number,
            timestamp,
            &writes_metrics,
        );
    }

    fn initial_bootloader_memory(
        updates_accumulator: &L1BatchUpdates,
        block_context: BlockContextMode,
    ) -> Vec<(usize, U256)> {
        let transactions_data = updates_accumulator
            .executed_transactions
            .iter()
            .map(|res| res.transaction.clone().into())
            .collect();

        let refunds = updates_accumulator
            .executed_transactions
            .iter()
            .map(|res| res.operator_suggested_refund)
            .collect();

        let compressed_bytecodes = updates_accumulator
            .executed_transactions
            .iter()
            .map(|res| res.compressed_bytecodes.clone())
            .collect();

        get_bootloader_memory(
            transactions_data,
            refunds,
            compressed_bytecodes,
            TxExecutionMode::VerifyExecute,
            block_context,
        )
    }

    fn report_l1_batch_metrics(
        &self,
        started_at: Instant,
        current_l1_batch_number: L1BatchNumber,
        block_timestamp: u64,
        writes_metrics: &DeduplicatedWritesMetrics,
    ) {
        metrics::histogram!(
            "server.state_keeper.l1_batch.initial_writes",
            writes_metrics.initial_storage_writes as f64
        );
        metrics::histogram!(
            "server.state_keeper.l1_batch.repeated_writes",
            writes_metrics.repeated_storage_writes as f64
        );

        metrics::histogram!(
            "server.state_keeper.l1_batch.transactions_in_l1_batch",
            self.l1_batch.executed_transactions.len() as f64
        );
        let l1_batch_latency =
            ((millis_since_epoch() - block_timestamp as u128 * 1_000) as f64) / 1_000.0;
        metrics::histogram!(
            "server.l1_batch.latency",
            l1_batch_latency,
            "stage" => "sealed"
        );

        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_time",
            started_at.elapsed(),
        );
        vlog::debug!(
            "Sealed L1 batch {current_l1_batch_number} in {:?}",
            started_at.elapsed()
        );
    }
}

impl MiniblockSealCommand {
    pub async fn seal(&self, storage: &mut StorageProcessor<'_>) {
        self.seal_inner(storage, false).await;
    }

    /// Seals a miniblock with the given number.
    ///
    /// If `is_fictive` flag is set to true, then it is assumed that we should seal a fictive miniblock
    /// with no transactions in it. It is needed because there might be some storage logs / events
    /// that are created after the last processed tx in the L1 batch: after the last transaction is processed,
    /// the bootloader enters the "tip" phase in which it can still generate events (e.g.,
    /// one for sending fees to the operator).
    ///
    /// `l2_erc20_bridge_addr` is required to extract the information on newly added tokens.
    async fn seal_inner(&self, storage: &mut StorageProcessor<'_>, is_fictive: bool) {
        self.assert_valid_miniblock(is_fictive);

        let l1_batch_number = self.l1_batch_number;
        let miniblock_number = self.miniblock_number;
        let started_at = Instant::now();
        let mut progress = SealProgress::for_miniblock(is_fictive);

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.miniblock.executed_transactions);
        let (writes_count, reads_count) =
            storage_log_query_write_read_counts(&self.miniblock.storage_logs);
        vlog::info!(
            "Sealing miniblock {miniblock_number} (L1 batch {l1_batch_number}) \
             with {total_tx_count} ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {event_count} events, \
             {reads_count} reads, {writes_count} writes",
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.miniblock.events.len()
        );

        let mut transaction = storage.start_transaction().await;
        let miniblock_header = MiniblockHeader {
            number: miniblock_number,
            timestamp: self.miniblock.timestamp,
            hash: miniblock_hash(miniblock_number),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            base_fee_per_gas: self.base_fee_per_gas,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.fair_l2_gas_price,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            protocol_version: Some(self.protocol_version),
        };

        transaction
            .blocks_dal()
            .insert_miniblock(&miniblock_header)
            .await;
        progress.end_stage("insert_miniblock_header", None);

        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_miniblock(
                miniblock_number,
                &self.miniblock.executed_transactions,
                self.base_fee_per_gas.into(),
            )
            .await;
        progress.end_stage(
            "mark_transactions_in_miniblock",
            Some(self.miniblock.executed_transactions.len()),
        );

        let write_logs = self.extract_write_logs(is_fictive);
        let write_log_count = write_logs.iter().map(|(_, logs)| logs.len()).sum();

        transaction
            .storage_logs_dal()
            .insert_storage_logs(miniblock_number, &write_logs)
            .await;
        progress.end_stage("insert_storage_logs", Some(write_log_count));

        let unique_updates = transaction
            .storage_dal()
            .apply_storage_logs(&write_logs)
            .await;
        progress.end_stage("apply_storage_logs", Some(write_log_count));

        let new_factory_deps = &self.miniblock.new_factory_deps;
        let new_factory_deps_count = new_factory_deps.len();
        if !new_factory_deps.is_empty() {
            transaction
                .storage_dal()
                .insert_factory_deps(miniblock_number, new_factory_deps)
                .await;
        }
        progress.end_stage("insert_factory_deps", Some(new_factory_deps_count));

        // Factory deps should be inserted before using `count_deployed_contracts`.
        let deployed_contract_count = Self::count_deployed_contracts(&unique_updates);
        progress.end_stage("extract_contracts_deployed", Some(deployed_contract_count));

        let added_tokens = extract_added_tokens(self.l2_erc20_bridge_addr, &self.miniblock.events);
        progress.end_stage("extract_added_tokens", Some(added_tokens.len()));
        let added_tokens_len = added_tokens.len();
        if !added_tokens.is_empty() {
            transaction.tokens_dal().add_tokens(added_tokens).await;
        }
        progress.end_stage("insert_tokens", Some(added_tokens_len));

        let miniblock_events = self.extract_events(is_fictive);
        let miniblock_event_count = miniblock_events
            .iter()
            .map(|(_, events)| events.len())
            .sum();
        progress.end_stage("extract_events", Some(miniblock_event_count));
        transaction
            .events_dal()
            .save_events(miniblock_number, &miniblock_events)
            .await;
        progress.end_stage("insert_events", Some(miniblock_event_count));

        let l2_to_l1_logs = self.extract_l2_to_l1_logs(is_fictive);
        let l2_to_l1_log_count = l2_to_l1_logs
            .iter()
            .map(|(_, l2_to_l1_logs)| l2_to_l1_logs.len())
            .sum();
        progress.end_stage("extract_l2_to_l1_logs", Some(l2_to_l1_log_count));
        transaction
            .events_dal()
            .save_l2_to_l1_logs(miniblock_number, &l2_to_l1_logs)
            .await;
        progress.end_stage("insert_l2_to_l1_logs", Some(l2_to_l1_log_count));

        transaction.commit().await;
        progress.end_stage("commit_miniblock", None);
        self.report_miniblock_metrics(started_at);
    }

    /// Performs several sanity checks to make sure that the miniblock is valid.
    fn assert_valid_miniblock(&self, is_fictive: bool) {
        assert_eq!(self.miniblock.executed_transactions.is_empty(), is_fictive);

        let first_tx_index = self.first_tx_index;
        let next_tx_index = first_tx_index + self.miniblock.executed_transactions.len();
        let tx_index_range = if is_fictive {
            next_tx_index..(next_tx_index + 1)
        } else {
            first_tx_index..next_tx_index
        };

        for event in &self.miniblock.events {
            let tx_index = event.location.1 as usize;
            assert!(tx_index_range.contains(&tx_index));
        }
        for storage_log in &self.miniblock.storage_logs {
            let tx_index = storage_log.log_query.tx_number_in_block as usize;
            assert!(tx_index_range.contains(&tx_index));
        }
    }

    fn extract_write_logs(&self, is_fictive: bool) -> Vec<(H256, Vec<StorageLog>)> {
        let logs = self.miniblock.storage_logs.iter();
        let grouped_logs = logs.group_by(|log| log.log_query.tx_number_in_block);

        let grouped_logs = grouped_logs.into_iter().map(|(tx_index, logs)| {
            let tx_hash = if is_fictive {
                assert_eq!(tx_index as usize, self.first_tx_index);
                H256::zero()
            } else {
                self.transaction(tx_index as usize).hash()
            };
            let logs = logs.filter_map(|log| {
                log.log_query
                    .rw_flag
                    .then(|| StorageLog::from_log_query(log))
            });
            (tx_hash, logs.collect())
        });
        grouped_logs.collect()
    }

    fn transaction(&self, index: usize) -> &Transaction {
        let tx_result = &self.miniblock.executed_transactions[index - self.first_tx_index];
        &tx_result.transaction
    }

    fn count_deployed_contracts(
        unique_updates: &HashMap<StorageKey, (H256, StorageValue)>,
    ) -> usize {
        let mut count = 0;
        for (key, (_, value)) in unique_updates {
            if *key.account().address() == ACCOUNT_CODE_STORAGE_ADDRESS {
                let bytecode_hash = *value;
                //  For now, we expected that if the `bytecode_hash` is zero, the contract was not deployed
                //  in the first place, so we don't do anything
                if bytecode_hash != H256::zero() {
                    count += 1;
                }
            }
        }
        count
    }

    fn extract_events(&self, is_fictive: bool) -> Vec<(IncludedTxLocation, Vec<&VmEvent>)> {
        self.group_by_tx_location(&self.miniblock.events, is_fictive, |event| event.location.1)
    }

    fn group_by_tx_location<'a, T>(
        &'a self,
        entries: &'a [T],
        is_fictive: bool,
        tx_location: impl Fn(&T) -> u32,
    ) -> Vec<(IncludedTxLocation, Vec<&'a T>)> {
        let grouped_entries = entries.iter().group_by(|&entry| tx_location(entry));
        let grouped_entries = grouped_entries.into_iter().map(|(tx_index, entries)| {
            let (tx_hash, tx_initiator_address) = if is_fictive {
                assert_eq!(tx_index as usize, self.first_tx_index);
                (H256::zero(), Address::zero())
            } else {
                let tx = self.transaction(tx_index as usize);
                (tx.hash(), tx.initiator_account())
            };

            let location = IncludedTxLocation {
                tx_hash,
                tx_index_in_miniblock: tx_index - self.first_tx_index as u32,
                tx_initiator_address,
            };
            (location, entries.collect())
        });
        grouped_entries.collect()
    }

    fn extract_l2_to_l1_logs(
        &self,
        is_fictive: bool,
    ) -> Vec<(IncludedTxLocation, Vec<&L2ToL1Log>)> {
        self.group_by_tx_location(&self.miniblock.l2_to_l1_logs, is_fictive, |log| {
            u32::from(log.tx_number_in_block)
        })
    }

    fn report_miniblock_metrics(&self, started_at: Instant) {
        let miniblock_number = self.miniblock_number;

        metrics::histogram!(
            "server.state_keeper.miniblock.transactions_in_miniblock",
            self.miniblock.executed_transactions.len() as f64
        );
        let miniblock_latency =
            ((millis_since_epoch() - self.miniblock.timestamp as u128 * 1_000) as f64) / 1_000.0;
        metrics::histogram!(
            "server.miniblock.latency",
            miniblock_latency,
            "stage" => "sealed"
        );
        metrics::histogram!(
            "server.state_keeper.miniblock.sealed_time",
            started_at.elapsed(),
        );
        metrics::gauge!(
            "server.miniblock.number",
            miniblock_number.0 as f64,
            "stage" => "sealed"
        );

        vlog::debug!(
            "Sealed miniblock {miniblock_number} in {:?}",
            started_at.elapsed()
        );
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

fn log_query_write_read_counts<'a>(logs: impl Iterator<Item = &'a LogQuery>) -> (usize, usize) {
    let mut reads_count = 0;
    let mut writes_count = 0;

    for log in logs {
        if log.rw_flag {
            writes_count += 1;
        } else {
            reads_count += 1;
        }
    }
    (writes_count, reads_count)
}

fn storage_log_query_write_read_counts(logs: &[StorageLogQuery]) -> (usize, usize) {
    log_query_write_read_counts(logs.iter().map(|log| &log.log_query))
}
