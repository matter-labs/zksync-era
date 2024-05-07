//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use std::time::{Duration, Instant};

use anyhow::Context as _;
use itertools::Itertools;
use multivm::utils::{get_max_batch_gas_limit, get_max_gas_per_pubdata_byte};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_shared_metrics::{BlockStage, L2BlockStage, APP_METRICS};
use zksync_types::{
    block::{L1BatchHeader, L2BlockHeader},
    event::{extract_added_tokens, extract_long_l2_to_l1_messages},
    helpers::unix_timestamp_ms,
    l1::L1Tx,
    l2::L2Tx,
    l2_to_l1_log::{SystemL2ToL1Log, UserL2ToL1Log},
    protocol_upgrade::ProtocolUpgradeTx,
    storage_writes_deduplicator::{ModifiedSlot, StorageWritesDeduplicator},
    tx::{
        tx_execution_info::DeduplicatedWritesMetrics, IncludedTxLocation,
        TransactionExecutionResult,
    },
    utils::display_timestamp,
    zk_evm_types::LogQuery,
    AccountTreeId, Address, ExecuteTransactionCommon, L1BlockNumber, ProtocolVersionId, StorageKey,
    StorageLog, Transaction, VmEvent, H256,
};
use zksync_utils::u256_to_h256;

use crate::state_keeper::{
    metrics::{
        L1BatchSealStage, L2BlockSealStage, TxExecutionType, KEEPER_METRICS, L1_BATCH_METRICS,
        L2_BLOCK_METRICS,
    },
    updates::{L2BlockSealCommand, UpdatesManager},
};

impl UpdatesManager {
    /// Persists an L1 batch in the storage.
    /// This action includes a creation of an empty "fictive" L2 block that contains
    /// the events generated during the bootloader "tip phase". Returns updates for this fictive L2 block.
    pub(super) async fn seal_l1_batch(
        &self,
        storage: &mut Connection<'_, Core>,
        l2_shared_bridge_addr: Address,
        insert_protective_reads: bool,
    ) -> anyhow::Result<()> {
        let started_at = Instant::now();
        let finished_batch = self
            .l1_batch
            .finished
            .as_ref()
            .context("L1 batch is not actually finished")?;
        let mut transaction = storage.start_transaction().await?;

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FictiveL2Block);
        // Seal fictive L2 block with last events and storage logs.
        let l2_block_command = self.seal_l2_block_command(
            l2_shared_bridge_addr,
            false, // fictive L2 blocks don't have txs, so it's fine to pass `false` here.
        );
        l2_block_command
            .seal_inner(&mut transaction, true)
            .await
            .context("failed persisting fictive L2 block")?;
        progress.observe(None);

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::LogDeduplication);

        progress.observe(
            finished_batch
                .final_execution_state
                .deduplicated_storage_log_queries
                .len(),
        );

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l1_batch.executed_transactions);
        let (dedup_writes_count, dedup_reads_count) = log_query_write_read_counts(
            finished_batch
                .final_execution_state
                .deduplicated_storage_log_queries
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
            extract_long_l2_to_l1_messages(&finished_batch.final_execution_state.events);
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
        };

        let final_bootloader_memory = finished_batch
            .final_bootloader_memory
            .clone()
            .unwrap_or_default();
        transaction
            .blocks_dal()
            .insert_l1_batch(
                &l1_batch,
                &final_bootloader_memory,
                self.pending_l1_gas_count(),
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

        let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = finished_batch
            .final_execution_state
            .deduplicated_storage_log_queries
            .iter()
            .partition(|log_query| log_query.rw_flag);
        if insert_protective_reads {
            let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertProtectiveReads);
            transaction
                .storage_logs_dedup_dal()
                .insert_protective_reads(self.l1_batch.number, &protective_reads)
                .await?;
            progress.observe(protective_reads.len());
        }

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::FilterWrittenSlots);
        let written_storage_keys: Vec<_> =
            if let Some(initially_written_slots) = &finished_batch.initially_written_slots {
                deduplicated_writes
                    .iter()
                    .filter_map(|log| {
                        let key =
                            StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
                        initially_written_slots
                            .contains(&key.hashed_key())
                            .then_some(key)
                    })
                    .collect()
            } else {
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
                    .await?;

                deduplicated_writes
                    .iter()
                    .filter_map(|log| {
                        let key =
                            StorageKey::new(AccountTreeId::new(log.address), u256_to_h256(log.key));
                        (!non_initial_writes.contains(&key.hashed_key())).then_some(key)
                    })
                    .collect()
            };
        progress.observe(deduplicated_writes.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::InsertInitialWrites);
        transaction
            .storage_logs_dedup_dal()
            .insert_initial_writes(self.l1_batch.number, &written_storage_keys)
            .await?;
        progress.observe(written_storage_keys.len());

        let progress = L1_BATCH_METRICS.start(L1BatchSealStage::CommitL1Batch);
        transaction.commit().await?;
        progress.observe(None);

        let writes_metrics = self.storage_writes_deduplicator.metrics();
        // Sanity check metrics.
        anyhow::ensure!(
            deduplicated_writes.len()
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

impl L2BlockSealCommand {
    pub(super) async fn seal(&self, storage: &mut Connection<'_, Core>) -> anyhow::Result<()> {
        self.seal_inner(storage, false)
            .await
            .with_context(|| format!("failed sealing L2 block #{}", self.l2_block.number))
    }

    async fn insert_transactions(
        &self,
        transaction: &mut Connection<'_, Core>,
    ) -> anyhow::Result<()> {
        for tx_result in &self.l2_block.executed_transactions {
            let tx = tx_result.transaction.clone();
            let tx_hash = tx.hash();

            match &tx.common_data {
                ExecuteTransactionCommon::L1(_) => {
                    // `unwrap` is safe due to the check above
                    let l1_tx = L1Tx::try_from(tx).unwrap();
                    let l1_block_number = L1BlockNumber(l1_tx.common_data.eth_block as u32);
                    transaction
                        .transactions_dal()
                        .insert_transaction_l1(&l1_tx, l1_block_number)
                        .await
                        .with_context(|| format!("failed persisting L1 transaction {tx_hash:?}"))?;
                }
                ExecuteTransactionCommon::L2(_) => {
                    // `unwrap` is safe due to the check above
                    let l2_tx = L2Tx::try_from(tx).unwrap();
                    // Using `Default` for execution metrics should be OK here, since this data is not used on the EN.
                    transaction
                        .transactions_dal()
                        .insert_transaction_l2(&l2_tx, Default::default())
                        .await
                        .with_context(|| format!("failed persisting L2 transaction {tx_hash:?}"))?;
                }
                ExecuteTransactionCommon::ProtocolUpgrade(_) => {
                    // `unwrap` is safe due to the check above
                    let protocol_system_upgrade_tx = ProtocolUpgradeTx::try_from(tx).unwrap();
                    transaction
                        .transactions_dal()
                        .insert_system_transaction(&protocol_system_upgrade_tx)
                        .await
                        .with_context(|| {
                            format!("failed persisting protocol upgrade transaction {tx_hash:?}")
                        })?;
                }
            }
        }
        Ok(())
    }

    /// Seals an L2 block with the given number.
    ///
    /// If `is_fictive` flag is set to true, then it is assumed that we should seal a fictive L2 block
    /// with no transactions in it. It is needed because there might be some storage logs / events
    /// that are created after the last processed tx in the L1 batch: after the last transaction is processed,
    /// the bootloader enters the "tip" phase in which it can still generate events (e.g.,
    /// one for sending fees to the operator).
    ///
    /// `l2_shared_bridge_addr` is required to extract the information on newly added tokens.
    async fn seal_inner(
        &self,
        storage: &mut Connection<'_, Core>,
        is_fictive: bool,
    ) -> anyhow::Result<()> {
        self.ensure_valid_l2_block(is_fictive)
            .context("L2 block is invalid")?;

        let mut transaction = storage.start_transaction().await?;
        if self.pre_insert_txs {
            let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::PreInsertTxs, is_fictive);
            self.insert_transactions(&mut transaction)
                .await
                .context("failed persisting transactions in L2 block")?;
            progress.observe(Some(self.l2_block.executed_transactions.len()));
        }

        let l1_batch_number = self.l1_batch_number;
        let l2_block_number = self.l2_block.number;
        let started_at = Instant::now();
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertL2BlockHeader, is_fictive);

        let (l1_tx_count, l2_tx_count) = l1_l2_tx_count(&self.l2_block.executed_transactions);
        tracing::info!(
            "Sealing L2 block {l2_block_number} with timestamp {ts} (L1 batch {l1_batch_number}) \
             with {total_tx_count} ({l2_tx_count} L2 + {l1_tx_count} L1) txs, {event_count} events",
            ts = display_timestamp(self.l2_block.timestamp),
            total_tx_count = l1_tx_count + l2_tx_count,
            event_count = self.l2_block.events.len()
        );

        let definite_vm_version = self
            .protocol_version
            .unwrap_or_else(ProtocolVersionId::last_potentially_undefined)
            .into();

        let l2_block_header = L2BlockHeader {
            number: l2_block_number,
            timestamp: self.l2_block.timestamp,
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
        };

        transaction
            .blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await?;
        progress.observe(None);

        let progress =
            L2_BLOCK_METRICS.start(L2BlockSealStage::MarkTransactionsInL2Block, is_fictive);
        transaction
            .transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                l2_block_number,
                &self.l2_block.executed_transactions,
                self.base_fee_per_gas.into(),
            )
            .await?;
        progress.observe(self.l2_block.executed_transactions.len());

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertStorageLogs, is_fictive);
        let write_logs = self.extract_deduplicated_write_logs(is_fictive);
        let write_log_count: usize = write_logs.iter().map(|(_, logs)| logs.len()).sum();
        transaction
            .storage_logs_dal()
            .insert_storage_logs(l2_block_number, &write_logs)
            .await?;
        progress.observe(write_log_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertFactoryDeps, is_fictive);
        let new_factory_deps = &self.l2_block.new_factory_deps;
        let new_factory_deps_count = new_factory_deps.len();
        if !new_factory_deps.is_empty() {
            transaction
                .factory_deps_dal()
                .insert_factory_deps(l2_block_number, new_factory_deps)
                .await?;
        }
        progress.observe(new_factory_deps_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractAddedTokens, is_fictive);
        let added_tokens = extract_added_tokens(self.l2_shared_bridge_addr, &self.l2_block.events);
        progress.observe(added_tokens.len());
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertTokens, is_fictive);
        let added_tokens_len = added_tokens.len();
        if !added_tokens.is_empty() {
            transaction.tokens_dal().add_tokens(&added_tokens).await?;
        }
        progress.observe(added_tokens_len);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractEvents, is_fictive);
        let l2_block_events = self.extract_events(is_fictive);
        let l2_block_event_count: usize =
            l2_block_events.iter().map(|(_, events)| events.len()).sum();
        progress.observe(l2_block_event_count);
        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertEvents, is_fictive);
        transaction
            .events_dal()
            .save_events(l2_block_number, &l2_block_events)
            .await?;
        progress.observe(l2_block_event_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::ExtractL2ToL1Logs, is_fictive);

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

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::InsertL2ToL1Logs, is_fictive);
        transaction
            .events_dal()
            .save_user_l2_to_l1_logs(l2_block_number, &user_l2_to_l1_logs)
            .await?;
        progress.observe(user_l2_to_l1_log_count);

        let progress = L2_BLOCK_METRICS.start(L2BlockSealStage::CommitL2Block, is_fictive);

        transaction.commit().await?;
        progress.observe(None);

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
        for storage_log in &self.l2_block.storage_logs {
            let tx_index = storage_log.log_query.tx_number_in_block as usize;
            anyhow::ensure!(
                tx_index_range.contains(&tx_index),
                "log transaction index {tx_index} is outside of the expected range {tx_index_range:?}"
            );
        }
        Ok(())
    }

    fn extract_deduplicated_write_logs(&self, is_fictive: bool) -> Vec<(H256, Vec<StorageLog>)> {
        let mut storage_writes_deduplicator = StorageWritesDeduplicator::new();
        storage_writes_deduplicator.apply(
            self.l2_block
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
                tx_index_in_l2_block: tx_index - self.first_tx_index as u32,
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
        self.group_by_tx_location(&self.l2_block.system_l2_to_l1_logs, is_fictive, |log| {
            u32::from(log.0.tx_number_in_block)
        })
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
            unix_timestamp_ms().saturating_sub(self.l2_block.timestamp * 1_000) as f64 / 1_000.0;
        let stage = &L2BlockStage::Sealed;
        APP_METRICS.miniblock_latency[stage].observe(Duration::from_secs_f64(l2_block_latency));
        APP_METRICS.miniblock_number[stage].set(l2_block_number.0.into());

        tracing::debug!(
            "Sealed L2 block #{l2_block_number} in {:?}",
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
