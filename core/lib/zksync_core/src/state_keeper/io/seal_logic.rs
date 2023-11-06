//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use itertools::Itertools;
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use multivm::interface::{FinishedL1Batch, L1BatchEnv};
use zksync_dal::StorageProcessor;
use zksync_system_constants::ACCOUNT_CODE_STORAGE_ADDRESS;
use zksync_types::{
    block::unpack_block_info,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    CURRENT_VIRTUAL_BLOCK_INFO_POSITION, SYSTEM_CONTEXT_ADDRESS,
};
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    event::{extract_added_tokens, extract_long_l2_to_l1_messages},
    storage_writes_deduplicator::{ModifiedSlot, StorageWritesDeduplicator},
    tx::{
        tx_execution_info::DeduplicatedWritesMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    zkevm_test_harness::witness::sort_storage_access::sort_storage_access_queries,
    AccountTreeId, Address, ExecuteTransactionCommon, L1BatchNumber, LogQuery, MiniblockNumber,
    StorageKey, StorageLog, StorageLogQuery, StorageValue, Transaction, VmEvent, H256,
};
// TODO (SMA-1206): use seconds instead of milliseconds.
use zksync_utils::{h256_to_u256, time::millis_since_epoch, u256_to_h256};

use crate::{
    metrics::{BlockStage, MiniblockStage, APP_METRICS},
    state_keeper::{
        extractors,
        metrics::{L1BatchSealStage, MiniblockSealStage, L1_BATCH_METRICS, MINIBLOCK_METRICS},
        updates::{MiniblockSealCommand, UpdatesManager},
    },
};

impl UpdatesManager {
    /// Persists an L1 batch in the storage.
    /// This action includes a creation of an empty "fictive" miniblock that contains
    /// the events generated during the bootloader "tip phase".
    pub(crate) async fn seal_l1_batch(
        mut self,
        storage: &mut StorageProcessor<'_>,
        current_miniblock_number: MiniblockNumber,
        l1_batch_env: &L1BatchEnv,
        finished_batch: FinishedL1Batch,
        l2_erc20_bridge_addr: Address,
    ) {
        let started_at = Instant::now();
        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::VmFinalization);
        let mut transaction = storage.start_transaction().await.unwrap();
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FictiveMiniblock);
        self.extend_from_fictive_transaction(finished_batch.block_tip_execution_result);
        // Seal fictive miniblock with last events and storage logs.
        let miniblock_command = self.seal_miniblock_command(
            l1_batch_env.number,
            current_miniblock_number,
            l2_erc20_bridge_addr,
        );
        miniblock_command.seal_inner(&mut transaction, true).await;
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::LogDeduplication);
        let (_, deduped_log_queries) = sort_storage_access_queries(
            finished_batch
                .final_execution_state
                .storage_log_queries
                .iter()
                .map(|log| &log.log_query),
        );
        progress.observe(deduped_log_queries.len());

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l1_batch.executed_transactions);
        let (writes_count, reads_count) = storage_log_query_write_read_counts(
            &finished_batch.final_execution_state.storage_log_queries,
        );
        let (dedup_writes_count, dedup_reads_count) =
            log_query_write_read_counts(deduped_log_queries.iter());

        tracing::info!(
            "Sealing L1 batch {current_l1_batch_number} with {total_tx_count} \
             ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {l2_to_l1_log_count} l2_l1_logs, \
             {event_count} events, {reads_count} reads ({dedup_reads_count} deduped), \
             {writes_count} writes ({dedup_writes_count} deduped)",
            total_tx_count = l1_tx_count + l2_tx_count,
            l2_to_l1_log_count = finished_batch
                .final_execution_state
                .user_l2_to_l1_logs
                .len(),
            event_count = finished_batch.final_execution_state.events.len(),
            current_l1_batch_number = l1_batch_env.number
        );

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertL1BatchHeader);
        let (_prev_hash, prev_timestamp) =
            extractors::wait_for_prev_l1_batch_params(&mut transaction, l1_batch_env.number).await;
        assert!(
            prev_timestamp < l1_batch_env.timestamp,
            "Cannot seal L1 batch #{}: Timestamp of previous L1 batch ({}) >= provisional L1 batch timestamp ({}), \
             meaning that L1 batch will be rejected by the bootloader",
            l1_batch_env.number,
            extractors::display_timestamp(prev_timestamp),
            extractors::display_timestamp(l1_batch_env.timestamp)
        );

        let l2_to_l1_messages =
            extract_long_l2_to_l1_messages(&finished_batch.final_execution_state.events);

        let l1_batch = L1BatchHeader {
            number: l1_batch_env.number,
            is_finished: true,
            timestamp: l1_batch_env.timestamp,
            fee_account_address: l1_batch_env.fee_account,
            priority_ops_onchain_data: self.l1_batch.priority_ops_onchain_data.clone(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            l2_to_l1_logs: finished_batch.final_execution_state.user_l2_to_l1_logs,
            l2_to_l1_messages,
            bloom: Default::default(),
            used_contract_hashes: finished_batch.final_execution_state.used_contract_hashes,
            base_fee_per_gas: l1_batch_env.base_fee(),
            l1_gas_price: self.l1_gas_price(),
            l2_fair_gas_price: self.fair_l2_gas_price(),
            base_system_contracts_hashes: self.base_system_contract_hashes(),
            protocol_version: Some(self.protocol_version()),
            system_logs: finished_batch.final_execution_state.system_logs,
        };

        let events_queue = finished_batch
            .final_execution_state
            .deduplicated_events_logs;

        transaction
            .blocks_dal()
            .insert_l1_batch(
                &l1_batch,
                finished_batch.final_bootloader_memory.as_ref().unwrap(),
                self.l1_batch.l1_gas_count,
                &events_queue,
                &finished_batch.final_execution_state.storage_refunds,
            )
            .await
            .unwrap();
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::SetL1BatchNumberForMiniblocks);
        transaction
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(l1_batch_env.number)
            .await
            .unwrap();
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::MarkTxsAsExecutedInL1Batch);
        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l1_batch(
                l1_batch_env.number,
                &self.l1_batch.executed_transactions,
            )
            .await;
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertProtectiveReads);
        let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
            .into_iter()
            .partition(|log_query| log_query.rw_flag);
        transaction
            .storage_logs_dedup_dal()
            .insert_protective_reads(l1_batch_env.number, &protective_reads)
            .await;
        progress.observe(protective_reads.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FilterWrittenSlots);
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
        progress.observe(deduplicated_writes.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertInitialWrites);
        let written_storage_keys: Vec<_> = deduplicated_writes
            .iter()
            .filter_map(|log| {
                let key = StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
                (!non_initial_writes.contains(&key.hashed_key())).then_some(key)
            })
            .collect();

        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(l1_batch_env.number, &written_storage_keys)
            .await;
        progress.observe(deduplicated_writes.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::CommitL1Batch);
        transaction.commit().await.unwrap();
        progress.observe(None);

        let writes_metrics = self.storage_writes_deduplicator.metrics();
        // Sanity check metrics.
        assert_eq!(
            deduplicated_writes.len(),
            writes_metrics.initial_storage_writes + writes_metrics.repeated_storage_writes,
            "Results of in-flight and common deduplications are mismatched"
        );

        self.report_l1_batch_metrics(
            started_at,
            l1_batch_env.number,
            l1_batch_env.timestamp,
            &writes_metrics,
        );
    }

    fn report_l1_batch_metrics(
        &self,
        started_at: Instant,
        current_l1_batch_number: L1BatchNumber,
        block_timestamp: u64,
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

        let l1_batch_latency =
            ((millis_since_epoch() - block_timestamp as u128 * 1_000) as f64) / 1_000.0;
        APP_METRICS.block_latency[&BlockStage::Sealed]
            .observe(Duration::from_secs_f64(l1_batch_latency));

        let elapsed = started_at.elapsed();
        L1_BATCH_METRICS.sealed_time.observe(elapsed);
        tracing::debug!("Sealed L1 batch {current_l1_batch_number} in {elapsed:?}");
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
        let progress =
            MINIBLOCK_METRICS.start(MiniblockSealStage::InsertMiniblockHeader, is_fictive);

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.miniblock.executed_transactions);
        let (writes_count, reads_count) =
            storage_log_query_write_read_counts(&self.miniblock.storage_logs);
        tracing::info!(
            "Sealing miniblock {miniblock_number} (L1 batch {l1_batch_number}) \
             with {total_tx_count} ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {event_count} events, \
             {reads_count} reads, {writes_count} writes",
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.miniblock.events.len()
        );

        let mut transaction = storage.start_transaction().await.unwrap();
        let miniblock_header = MiniblockHeader {
            number: miniblock_number,
            timestamp: self.miniblock.timestamp,
            hash: self.miniblock.get_miniblock_hash(),
            l1_tx_count: l1_tx_count as u16,
            l2_tx_count: l2_tx_count as u16,
            base_fee_per_gas: self.base_fee_per_gas,
            l1_gas_price: self.l1_gas_price,
            l2_fair_gas_price: self.fair_l2_gas_price,
            base_system_contracts_hashes: self.base_system_contracts_hashes,
            protocol_version: self.protocol_version,
            virtual_blocks: self.miniblock.virtual_blocks,
        };

        transaction
            .blocks_dal()
            .insert_miniblock(&miniblock_header)
            .await
            .unwrap();
        progress.observe(None);

        let progress =
            MINIBLOCK_METRICS.start(MiniblockSealStage::MarkTransactionsInMiniblock, is_fictive);
        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_miniblock(
                miniblock_number,
                &self.miniblock.executed_transactions,
                self.base_fee_per_gas.into(),
            )
            .await;
        progress.observe(self.miniblock.executed_transactions.len());

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::InsertStorageLogs, is_fictive);
        let write_logs = self.extract_deduplicated_write_logs(is_fictive);
        let write_log_count: usize = write_logs.iter().map(|(_, logs)| logs.len()).sum();
        transaction
            .storage_logs_dal()
            .insert_storage_logs(miniblock_number, &write_logs)
            .await;
        progress.observe(write_log_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::ApplyStorageLogs, is_fictive);
        let unique_updates = transaction
            .storage_dal()
            .apply_storage_logs(&write_logs)
            .await;
        progress.observe(write_log_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::InsertFactoryDeps, is_fictive);
        let new_factory_deps = &self.miniblock.new_factory_deps;
        let new_factory_deps_count = new_factory_deps.len();
        if !new_factory_deps.is_empty() {
            transaction
                .storage_dal()
                .insert_factory_deps(miniblock_number, new_factory_deps)
                .await;
        }
        progress.observe(new_factory_deps_count);

        let progress =
            MINIBLOCK_METRICS.start(MiniblockSealStage::ExtractContractsDeployed, is_fictive);
        let deployed_contract_count = Self::count_deployed_contracts(&unique_updates);
        progress.observe(deployed_contract_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::ExtractAddedTokens, is_fictive);
        let added_tokens = extract_added_tokens(self.l2_erc20_bridge_addr, &self.miniblock.events);
        progress.observe(added_tokens.len());
        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::InsertTokens, is_fictive);
        let added_tokens_len = added_tokens.len();
        if !added_tokens.is_empty() {
            transaction.tokens_dal().add_tokens(added_tokens).await;
        }
        progress.observe(added_tokens_len);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::ExtractEvents, is_fictive);
        let miniblock_events = self.extract_events(is_fictive);
        let miniblock_event_count: usize = miniblock_events
            .iter()
            .map(|(_, events)| events.len())
            .sum();
        progress.observe(miniblock_event_count);
        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::InsertEvents, is_fictive);
        transaction
            .events_dal()
            .save_events(miniblock_number, &miniblock_events)
            .await;
        progress.observe(miniblock_event_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::ExtractL2ToL1Logs, is_fictive);

        let system_l2_to_l1_logs = self.extract_system_l2_to_l1_logs(is_fictive);
        let user_l2_to_l1_logs = self.extract_user_l2_to_l1_logs(is_fictive);

        let system_l2_to_l1_log_count: usize = system_l2_to_l1_logs
            .iter()
            .map(|(_, l2_to_l1_logs)| l2_to_l1_logs.len())
            .sum();
        let user_l2_to_l1_log_count: usize = user_l2_to_l1_logs
            .iter()
            .map(|(_, l2_to_l1_logs)| l2_to_l1_logs.len())
            .sum();

        progress.observe(system_l2_to_l1_log_count + user_l2_to_l1_log_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::InsertL2ToL1Logs, is_fictive);
        transaction
            .events_dal()
            .save_user_l2_to_l1_logs(miniblock_number, &user_l2_to_l1_logs)
            .await;
        progress.observe(user_l2_to_l1_log_count);

        let progress = MINIBLOCK_METRICS.start(MiniblockSealStage::CommitMiniblock, is_fictive);
        let current_l2_virtual_block_info = transaction
            .storage_dal()
            .get_by_key(&StorageKey::new(
                AccountTreeId::new(SYSTEM_CONTEXT_ADDRESS),
                CURRENT_VIRTUAL_BLOCK_INFO_POSITION,
            ))
            .await
            .unwrap_or_default();
        let (current_l2_virtual_block_number, _) =
            unpack_block_info(h256_to_u256(current_l2_virtual_block_info));

        transaction.commit().await.unwrap();
        progress.observe(None);
        self.report_miniblock_metrics(started_at, current_l2_virtual_block_number);
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

    fn extract_deduplicated_write_logs(&self, is_fictive: bool) -> Vec<(H256, Vec<StorageLog>)> {
        let mut storage_writes_deduplicator = StorageWritesDeduplicator::new();
        storage_writes_deduplicator.apply(
            self.miniblock
                .storage_logs
                .iter()
                .filter(|log| log.log_query.rw_flag),
        );
        let deduplicated_logs = storage_writes_deduplicator.into_modified_key_values();

        deduplicated_logs
            .into_iter()
            .map(
                |(
                    key,
                    ModifiedSlot {
                        value, tx_index, ..
                    },
                )| (tx_index, (key, value)),
            )
            .sorted_by_key(|(tx_index, _)| *tx_index)
            .group_by(|(tx_index, _)| *tx_index)
            .into_iter()
            .map(|(tx_index, logs)| {
                let tx_hash = if is_fictive {
                    assert_eq!(tx_index as usize, self.first_tx_index);
                    H256::zero()
                } else {
                    self.transaction(tx_index as usize).hash()
                };
                (
                    tx_hash,
                    logs.into_iter()
                        .map(|(_, (key, value))| {
                            StorageLog::new_write_log(key, u256_to_h256(value))
                        })
                        .collect(),
                )
            })
            .collect()
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
                // TODO(SMA-1554): Support contracts deletion.
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

    fn extract_system_l2_to_l1_logs(
        &self,
        is_fictive: bool,
    ) -> Vec<(IncludedTxLocation, Vec<&SystemL2ToL1Log>)> {
        self.group_by_tx_location(&self.miniblock.system_l2_to_l1_logs, is_fictive, |log| {
            u32::from(log.0.tx_number_in_block)
        })
    }

    fn extract_user_l2_to_l1_logs(
        &self,
        is_fictive: bool,
    ) -> Vec<(IncludedTxLocation, Vec<&UserL2ToL1Log>)> {
        self.group_by_tx_location(&self.miniblock.user_l2_to_l1_logs, is_fictive, |log| {
            u32::from(log.0.tx_number_in_block)
        })
    }

    fn report_miniblock_metrics(&self, started_at: Instant, latest_virtual_block_number: u64) {
        let miniblock_number = self.miniblock_number;

        MINIBLOCK_METRICS
            .transactions_in_miniblock
            .observe(self.miniblock.executed_transactions.len());
        MINIBLOCK_METRICS.sealed_time.observe(started_at.elapsed());

        let miniblock_latency =
            ((millis_since_epoch() - self.miniblock.timestamp as u128 * 1_000) as f64) / 1_000.0;
        let stage = &MiniblockStage::Sealed;
        APP_METRICS.miniblock_latency[stage].observe(Duration::from_secs_f64(miniblock_latency));
        APP_METRICS.miniblock_number[stage].set(miniblock_number.0.into());
        APP_METRICS.miniblock_virtual_block_number[stage].set(latest_virtual_block_number);

        tracing::debug!(
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
