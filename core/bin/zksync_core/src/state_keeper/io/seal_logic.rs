//! This module is a source-of-truth on what is expected to be done when sealing a block.
//! It contains the logic of the block sealing, which is used by both the mempool-based and external node IO.

use std::time::{Duration, Instant};

use vm::vm_with_bootloader::BlockContextMode;
use vm::vm_with_bootloader::DerivedBlockContext;
use vm::VmBlockResult;
use zksync_dal::StorageProcessor;
use zksync_types::{
    block::L1BatchHeader,
    block::MiniblockHeader,
    event::{extract_added_tokens, extract_long_l2_to_l1_messages},
    zkevm_test_harness::witness::sort_storage_access::sort_storage_access_queries,
    L1BatchNumber, MiniblockNumber,
};
use zksync_utils::{miniblock_hash, time::millis_since_epoch};

use crate::state_keeper::{extractors, io::common::StateKeeperStats, updates::UpdatesManager};

/// Persists an L1 batch in the storage.
/// This action includes a creation of an empty "fictive" miniblock that contains the events
/// generated during the bootloader "tip phase".
pub(crate) fn seal_l1_batch_impl(
    current_miniblock_number: MiniblockNumber,
    current_l1_batch_number: L1BatchNumber,
    statistics: &mut StateKeeperStats,
    storage: &mut StorageProcessor<'_>,
    block_result: VmBlockResult,
    mut updates_manager: UpdatesManager,
    block_context: DerivedBlockContext,
) {
    let started_at = Instant::now();
    let mut stage_started_at: Instant = Instant::now();

    let mut transaction = storage.start_transaction_blocking();

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
    track_l1_batch_execution_stage("vm_finalization", &mut stage_started_at, None);

    updates_manager.extend_from_fictive_transaction(block_tip_result.logs);
    // Seal fictive miniblock with last events and storage logs.
    seal_miniblock_impl(
        current_miniblock_number,
        current_l1_batch_number,
        statistics,
        &mut transaction,
        &updates_manager,
        true,
    );
    track_l1_batch_execution_stage("fictive_miniblock", &mut stage_started_at, None);

    let (_, deduped_log_queries) = sort_storage_access_queries(
        full_result
            .storage_log_queries
            .iter()
            .map(|log| &log.log_query),
    );
    track_l1_batch_execution_stage(
        "log_deduplication",
        &mut stage_started_at,
        Some(deduped_log_queries.len()),
    );

    let (l1_tx_count, l2_tx_count) =
        extractors::l1_l2_tx_count(&updates_manager.l1_batch.executed_transactions);
    vlog::info!(
        "sealing l1 batch {:?} with {:?} ({:?} l2 + {:?} l1) txs, {:?} l2_l1_logs, {:?} events, (writes, reads): {:?} , (writes_dedup, reads_dedup): {:?} ",
        current_l1_batch_number,
        l1_tx_count + l2_tx_count,
        l2_tx_count,
        l1_tx_count,
        full_result.l2_to_l1_logs.len(),
        full_result.events.len(),
        extractors::storage_log_query_write_read_counts(&full_result.storage_log_queries),
        extractors::log_query_write_read_counts(&deduped_log_queries),
    );

    let hash = extractors::wait_for_prev_l1_batch_state_root_unchecked(
        &mut transaction,
        current_l1_batch_number,
    );
    let block_context_properties = BlockContextMode::NewBlock(block_context, hash);

    let l1_batch = L1BatchHeader {
        number: current_l1_batch_number,
        is_finished: true,
        timestamp: block_context.context.block_timestamp,
        fee_account_address: block_context.context.operator_address,
        priority_ops_onchain_data: updates_manager.l1_batch.priority_ops_onchain_data.clone(),
        l1_tx_count: l1_tx_count as u16,
        l2_tx_count: l2_tx_count as u16,
        l2_to_l1_logs: full_result.l2_to_l1_logs,
        l2_to_l1_messages: extract_long_l2_to_l1_messages(&full_result.events),
        bloom: Default::default(),
        initial_bootloader_contents: extractors::get_initial_bootloader_memory(
            &updates_manager.l1_batch,
            block_context_properties,
        ),
        used_contract_hashes: full_result.used_contract_hashes,
        base_fee_per_gas: block_context.base_fee,
        l1_gas_price: updates_manager.l1_gas_price(),
        l2_fair_gas_price: updates_manager.fair_l2_gas_price(),
        base_system_contracts_hashes: updates_manager.base_system_contract_hashes(),
    };

    transaction
        .blocks_dal()
        .insert_l1_batch(l1_batch, updates_manager.l1_batch.l1_gas_count);
    track_l1_batch_execution_stage("insert_l1_batch_header", &mut stage_started_at, None);

    transaction
        .blocks_dal()
        .mark_miniblocks_as_executed_in_l1_batch(current_l1_batch_number);
    track_l1_batch_execution_stage(
        "set_l1_batch_number_for_miniblocks",
        &mut stage_started_at,
        None,
    );

    transaction
        .transactions_dal()
        .mark_txs_as_executed_in_l1_batch(
            current_l1_batch_number,
            &updates_manager.l1_batch.executed_transactions,
        );
    track_l1_batch_execution_stage(
        "mark_txs_as_executed_in_l1_batch",
        &mut stage_started_at,
        None,
    );

    let (deduplicated_writes, protective_reads): (Vec<_>, Vec<_>) = deduped_log_queries
        .into_iter()
        .partition(|log_query| log_query.rw_flag);
    transaction
        .storage_logs_dedup_dal()
        .insert_protective_reads(current_l1_batch_number, &protective_reads);
    track_l1_batch_execution_stage(
        "insert_protective_reads",
        &mut stage_started_at,
        Some(protective_reads.len()),
    );

    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(current_l1_batch_number, &deduplicated_writes);
    track_l1_batch_execution_stage(
        "insert_initial_writes",
        &mut stage_started_at,
        Some(deduplicated_writes.len()),
    );

    transaction.commit_blocking();
    track_l1_batch_execution_stage("commit_l1_batch", &mut stage_started_at, None);

    let writes_metrics = updates_manager.storage_writes_deduplicator.metrics();
    // Sanity check.
    assert_eq!(
        deduplicated_writes.len(),
        writes_metrics.initial_storage_writes + writes_metrics.repeated_storage_writes,
        "Results of in-flight and common deduplications are mismatched"
    );
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
        updates_manager.l1_batch.executed_transactions.len() as f64
    );
    metrics::histogram!(
        "server.l1_batch.latency",
        ((millis_since_epoch() - block_context.context.block_timestamp as u128 * 1000) as f64) / 1000f64,
         "stage" => "sealed"
    );

    metrics::histogram!(
        "server.state_keeper.l1_batch.sealed_time",
        started_at.elapsed(),
    );
    vlog::debug!(
        "sealed l1 batch {} in {:?}",
        current_l1_batch_number,
        started_at.elapsed()
    );
}

// Seal miniblock with the given number.
//
// If `is_fictive` flag is set to true, then it is assumed that we should seal a fictive miniblock with no transactions
// in it. It is needed because there might be some storage logs/events that are created after the last processed tx in
// l1 batch: after the last transaction is processed, bootloader enters the "tip" phase in which it can still generate
// events (e.g. one for sending fees to the operator).
pub(crate) fn seal_miniblock_impl(
    current_miniblock_number: MiniblockNumber,
    current_l1_batch_number: L1BatchNumber,
    statistics: &mut StateKeeperStats,
    storage: &mut StorageProcessor<'_>,
    updates_manager: &UpdatesManager,
    is_fictive: bool,
) {
    miniblock_assertions(updates_manager, is_fictive);

    let started_at = Instant::now();
    let mut stage_started_at: Instant = Instant::now();

    let (l1_tx_count, l2_tx_count) =
        extractors::l1_l2_tx_count(&updates_manager.miniblock.executed_transactions);
    vlog::info!(
            "sealing miniblock {} (l1 batch {}) with {} ({} l2 + {} l1) txs, {} events, (writes, reads): {:?}",
            current_miniblock_number,
            current_l1_batch_number,
            l1_tx_count + l2_tx_count,
            l2_tx_count,
            l1_tx_count,
            updates_manager.miniblock.events.len(),
            extractors::storage_log_query_write_read_counts(&updates_manager.miniblock.storage_logs),
        );

    let mut transaction = storage.start_transaction_blocking();
    let miniblock_header = MiniblockHeader {
        number: current_miniblock_number,
        timestamp: updates_manager.miniblock.timestamp,
        hash: miniblock_hash(current_miniblock_number),
        l1_tx_count: l1_tx_count as u16,
        l2_tx_count: l2_tx_count as u16,
        base_fee_per_gas: updates_manager.base_fee_per_gas(),
        l1_gas_price: updates_manager.l1_gas_price(),
        l2_fair_gas_price: updates_manager.fair_l2_gas_price(),
        base_system_contracts_hashes: updates_manager.base_system_contract_hashes(),
    };

    transaction.blocks_dal().insert_miniblock(miniblock_header);
    track_miniblock_execution_stage(
        "insert_miniblock_header",
        &mut stage_started_at,
        None,
        is_fictive,
    );

    transaction
        .transactions_dal()
        .mark_txs_as_executed_in_miniblock(
            current_miniblock_number,
            &updates_manager.miniblock.executed_transactions,
            updates_manager.base_fee_per_gas().into(),
        );
    track_miniblock_execution_stage(
        "mark_transactions_in_miniblock",
        &mut stage_started_at,
        Some(updates_manager.miniblock.executed_transactions.len()),
        is_fictive,
    );

    let storage_logs = extractors::log_queries_to_storage_logs(
        &updates_manager.miniblock.storage_logs,
        updates_manager,
        is_fictive,
    );
    let write_logs = extractors::write_logs_from_storage_logs(storage_logs);
    let write_logs_len = write_logs.iter().flat_map(|(_, logs)| logs).count();

    transaction
        .storage_logs_dal()
        .insert_storage_logs(current_miniblock_number, &write_logs);
    track_miniblock_execution_stage(
        "insert_storage_logs",
        &mut stage_started_at,
        Some(write_logs_len),
        is_fictive,
    );

    let unique_updates = transaction.storage_dal().apply_storage_logs(&write_logs);
    track_miniblock_execution_stage(
        "apply_storage_logs",
        &mut stage_started_at,
        Some(write_logs_len),
        is_fictive,
    );

    let new_factory_deps = updates_manager.miniblock.new_factory_deps.clone();
    let new_factory_deps_len = new_factory_deps.iter().flat_map(|(_, deps)| deps).count();
    if !new_factory_deps.is_empty() {
        transaction
            .storage_dal()
            .insert_factory_deps(current_miniblock_number, new_factory_deps);
    }
    track_miniblock_execution_stage(
        "insert_factory_deps",
        &mut stage_started_at,
        Some(new_factory_deps_len),
        is_fictive,
    );

    // Factory deps should be inserted before using `contracts_deployed_this_miniblock`.
    let deployed_contracts =
        extractors::contracts_deployed_this_miniblock(unique_updates, &mut transaction);
    if !deployed_contracts.is_empty() {
        statistics.num_contracts += deployed_contracts.len() as u64;
    }
    let deployed_contracts_len = deployed_contracts
        .iter()
        .flat_map(|(_, contracts)| contracts)
        .count();
    track_miniblock_execution_stage(
        "extract_contracts_deployed",
        &mut stage_started_at,
        Some(deployed_contracts_len),
        is_fictive,
    );

    let added_tokens = extract_added_tokens(&updates_manager.miniblock.events);
    track_miniblock_execution_stage(
        "extract_added_tokens",
        &mut stage_started_at,
        Some(added_tokens.len()),
        is_fictive,
    );
    let added_tokens_len = added_tokens.len();
    if !added_tokens.is_empty() {
        transaction.tokens_dal().add_tokens(added_tokens);
    }
    track_miniblock_execution_stage(
        "insert_tokens",
        &mut stage_started_at,
        Some(added_tokens_len),
        is_fictive,
    );

    let events_this_miniblock = extractors::extract_events_this_block(
        &updates_manager.miniblock.events,
        updates_manager,
        is_fictive,
    );

    let events_this_miniblock_len = events_this_miniblock
        .iter()
        .flat_map(|(_, events)| events.iter())
        .count();

    track_miniblock_execution_stage(
        "extract_events",
        &mut stage_started_at,
        Some(events_this_miniblock_len),
        is_fictive,
    );
    transaction
        .events_dal()
        .save_events(current_miniblock_number, events_this_miniblock);
    track_miniblock_execution_stage(
        "insert_events",
        &mut stage_started_at,
        Some(events_this_miniblock_len),
        is_fictive,
    );

    let l2_to_l1_logs_this_miniblock = extractors::extract_l2_to_l1_logs_this_block(
        &updates_manager.miniblock.l2_to_l1_logs,
        updates_manager,
        is_fictive,
    );

    let l2_to_l1_logs_this_miniblock_len = l2_to_l1_logs_this_miniblock
        .iter()
        .flat_map(|(_, l2_to_l1_logs)| l2_to_l1_logs.iter())
        .count();

    track_miniblock_execution_stage(
        "extract_l2_to_l1_logs",
        &mut stage_started_at,
        Some(l2_to_l1_logs_this_miniblock_len),
        is_fictive,
    );
    transaction
        .events_dal()
        .save_l2_to_l1_logs(current_miniblock_number, l2_to_l1_logs_this_miniblock);
    track_miniblock_execution_stage(
        "insert_l2_to_l1_logs",
        &mut stage_started_at,
        Some(l2_to_l1_logs_this_miniblock_len),
        is_fictive,
    );

    transaction.commit_blocking();
    track_miniblock_execution_stage("commit_miniblock", &mut stage_started_at, None, is_fictive);

    metrics::histogram!(
        "server.state_keeper.miniblock.transactions_in_miniblock",
        updates_manager.miniblock.executed_transactions.len() as f64
    );
    metrics::histogram!(
        "server.miniblock.latency",
        ((millis_since_epoch() - updates_manager.miniblock.timestamp as u128 * 1000) as f64) / 1000f64,
         "stage" => "sealed"
    );
    metrics::histogram!(
        "server.state_keeper.miniblock.sealed_time",
        started_at.elapsed(),
    );
    metrics::gauge!(
        "server.miniblock.number",
        current_miniblock_number.0 as f64,
        "stage" => "sealed"
    );

    metrics::gauge!(
        "server.state_keeper.storage_contracts_size",
        statistics.num_contracts as f64
    );
    vlog::debug!(
        "sealed miniblock {} in {:?}",
        current_miniblock_number,
        started_at.elapsed()
    );

    track_miniblock_execution_stage(
        "apply_miniblock_updates_to_l1_batch_updates_accumulator",
        &mut stage_started_at,
        None,
        is_fictive,
    );
}

/// Performs several sanity checks to make sure that the miniblock is valid.
fn miniblock_assertions(updates_manager: &UpdatesManager, is_fictive: bool) {
    if is_fictive {
        assert!(updates_manager.miniblock.executed_transactions.is_empty());
    } else {
        assert!(!updates_manager.miniblock.executed_transactions.is_empty());
    }

    let first_tx_index_in_miniblock = updates_manager.l1_batch.executed_transactions.len();
    let next_tx_index = updates_manager.pending_executed_transactions_len();
    let miniblock_tx_index_range = if is_fictive {
        next_tx_index..(next_tx_index + 1)
    } else {
        first_tx_index_in_miniblock..next_tx_index
    };

    for event in updates_manager.miniblock.events.iter() {
        assert!(miniblock_tx_index_range.contains(&(event.location.1 as usize)))
    }
    for storage_log in updates_manager.miniblock.storage_logs.iter() {
        assert!(
            miniblock_tx_index_range.contains(&(storage_log.log_query.tx_number_in_block as usize))
        )
    }
}

fn track_l1_batch_execution_stage(
    stage: &'static str,
    stage_started_at: &mut Instant,
    count: Option<usize>,
) {
    metrics::histogram!(
        "server.state_keeper.l1_batch.sealed_time_stage",
        stage_started_at.elapsed(),
        "stage" => stage
    );
    if let Some(count) = count {
        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_entity_count",
            count as f64,
            "stage" => stage
        );
        metrics::histogram!(
            "server.state_keeper.l1_batch.sealed_entity_per_unit",
            stage_started_at.elapsed().div_f64(count as f64),
            "stage" => stage
        );
    }
    *stage_started_at = Instant::now();
}

fn track_miniblock_execution_stage(
    stage: &'static str,
    stage_started_at: &mut Instant,
    count: Option<usize>,
    is_fictive: bool,
) {
    if stage_started_at.elapsed() > Duration::from_millis(10) {
        vlog::debug!(
            "miniblock execution stage {} took {:?} with count {:?}",
            stage,
            stage_started_at.elapsed(),
            count
        );
    }
    metrics::histogram!(
        "server.state_keeper.miniblock.sealed_time_stage",
        stage_started_at.elapsed(),
        "stage" => stage,
        "is_fictive" => is_fictive.to_string(),
    );
    if let Some(count) = count {
        metrics::histogram!(
            "server.state_keeper.miniblock.sealed_entity_count",
            count as f64,
            "stage" => stage,
            "is_fictive" => is_fictive.to_string(),
        );
        if count > 0 {
            metrics::histogram!(
                "server.state_keeper.miniblock.sealed_entity_per_unit",
                stage_started_at.elapsed().div_f64(count as f64),
                "stage" => stage,
                "is_fictive" => is_fictive.to_string(),
            );
        }
    }
    *stage_started_at = Instant::now();
}
