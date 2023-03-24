//! Pure functions that convert blocks/transactions data as required by the state keeper.

use itertools::Itertools;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use vm::vm_with_bootloader::{get_bootloader_memory, BlockContextMode, TxExecutionMode};
use vm::zk_evm::aux_structures::LogQuery;
use zksync_dal::StorageProcessor;
use zksync_types::block::DeployedContract;
use zksync_types::tx::{IncludedTxLocation, TransactionExecutionResult};
use zksync_types::{
    l2_to_l1_log::L2ToL1Log, log::StorageLogKind, AccountTreeId, Address, ExecuteTransactionCommon,
    L1BatchNumber, StorageKey, StorageLog, StorageLogQuery, StorageValue, VmEvent,
    ACCOUNT_CODE_STORAGE_ADDRESS, H256, U256,
};
use zksync_utils::{h256_to_account_address, h256_to_u256};

use super::updates::{L1BatchUpdates, UpdatesManager};

/// Storage logs grouped by transaction hash
type StorageLogs = Vec<(H256, Vec<StorageLog>)>;

pub(crate) fn log_queries_to_storage_logs(
    log_queries: &[StorageLogQuery],
    updates_manager: &UpdatesManager,
    is_fictive_miniblock: bool,
) -> StorageLogs {
    log_queries
        .iter()
        .group_by(|log| log.log_query.tx_number_in_block)
        .into_iter()
        .map(|(tx_index, logs)| {
            let tx_hash = if is_fictive_miniblock {
                assert_eq!(
                    tx_index as usize,
                    updates_manager.pending_executed_transactions_len()
                );
                H256::zero()
            } else {
                updates_manager.get_tx_by_index(tx_index as usize).hash()
            };

            (
                tx_hash,
                logs.map(StorageLog::from_log_query)
                    .collect::<Vec<StorageLog>>(),
            )
        })
        .collect()
}

pub(crate) fn write_logs_from_storage_logs(storage_logs: StorageLogs) -> StorageLogs {
    storage_logs
        .into_iter()
        .map(|(hash, mut logs)| {
            logs.retain(|log| log.kind == StorageLogKind::Write);
            (hash, logs)
        })
        .collect()
}

pub(crate) fn extract_events_this_block(
    vm_events: &[VmEvent],
    updates_manager: &UpdatesManager,
    is_fictive_miniblock: bool,
) -> Vec<(IncludedTxLocation, Vec<VmEvent>)> {
    vm_events
        .iter()
        .group_by(|event| event.location.1)
        .into_iter()
        .map(|(tx_index, events)| {
            let (tx_hash, tx_initiator_address) = if is_fictive_miniblock {
                assert_eq!(
                    tx_index as usize,
                    updates_manager.pending_executed_transactions_len()
                );
                (H256::zero(), Address::zero())
            } else {
                let tx = updates_manager.get_tx_by_index(tx_index as usize);
                (tx.hash(), tx.initiator_account())
            };

            (
                IncludedTxLocation {
                    tx_hash,
                    tx_index_in_miniblock: tx_index
                        - updates_manager.l1_batch.executed_transactions.len() as u32,
                    tx_initiator_address,
                },
                events.cloned().collect::<Vec<VmEvent>>(),
            )
        })
        .collect()
}

pub(crate) fn extract_l2_to_l1_logs_this_block(
    l2_to_l1_logs: &[L2ToL1Log],
    updates_manager: &UpdatesManager,
    is_fictive_miniblock: bool,
) -> Vec<(IncludedTxLocation, Vec<L2ToL1Log>)> {
    l2_to_l1_logs
        .iter()
        .group_by(|log| log.tx_number_in_block)
        .into_iter()
        .map(|(tx_index, l2_to_l1_logs)| {
            let (tx_hash, tx_initiator_address) = if is_fictive_miniblock {
                assert_eq!(
                    tx_index as usize,
                    updates_manager.pending_executed_transactions_len()
                );
                (H256::zero(), Address::zero())
            } else {
                let tx = updates_manager.get_tx_by_index(tx_index as usize);
                (tx.hash(), tx.initiator_account())
            };

            (
                IncludedTxLocation {
                    tx_hash,
                    tx_index_in_miniblock: tx_index as u32
                        - updates_manager.l1_batch.executed_transactions.len() as u32,
                    tx_initiator_address,
                },
                l2_to_l1_logs.cloned().collect::<Vec<L2ToL1Log>>(),
            )
        })
        .collect()
}

pub(crate) fn l1_l2_tx_count(
    executed_transactions: &[TransactionExecutionResult],
) -> (usize, usize) {
    let (l1_txs, l2_txs): (
        Vec<&TransactionExecutionResult>,
        Vec<&TransactionExecutionResult>,
    ) = executed_transactions
        .iter()
        .partition(|t| matches!(t.transaction.common_data, ExecuteTransactionCommon::L1(_)));
    (l1_txs.len(), l2_txs.len())
}

pub(crate) fn get_initial_bootloader_memory(
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

pub(crate) fn storage_log_query_write_read_counts(logs: &[StorageLogQuery]) -> (usize, usize) {
    let (reads, writes): (Vec<&StorageLogQuery>, Vec<&StorageLogQuery>) =
        logs.iter().partition(|l| l.log_query.rw_flag);
    (reads.len(), writes.len())
}

pub(crate) fn log_query_write_read_counts(logs: &[LogQuery]) -> (usize, usize) {
    let (reads, writes): (Vec<&LogQuery>, Vec<&LogQuery>) = logs.iter().partition(|l| l.rw_flag);
    (reads.len(), writes.len())
}

pub(crate) fn contracts_deployed_this_miniblock(
    unique_storage_updates: Vec<(StorageKey, (H256, StorageValue))>,
    storage: &mut StorageProcessor<'_>,
) -> Vec<(H256, Vec<DeployedContract>)> {
    let mut result: HashMap<H256, Vec<DeployedContract>> = Default::default();

    // Each storage update in the AccountCodeStorage denotes the fact
    // some contract bytecode has been deployed
    unique_storage_updates
        .into_iter()
        .filter(|(key, _)| *key.account().address() == ACCOUNT_CODE_STORAGE_ADDRESS)
        .for_each(|(code_key, (tx_hash, bytecode_hash))| {
            if bytecode_hash == H256::zero() {
                return;
            }

            let contract_bytecode = storage
                .storage_dal()
                .get_factory_dep(bytecode_hash)
                .expect("Missing factory dep for deployed contract");

            let contracts_in_tx = result.entry(tx_hash).or_insert_with(Default::default);
            contracts_in_tx.push(DeployedContract {
                account_id: AccountTreeId::new(h256_to_account_address(code_key.key())),
                bytecode: contract_bytecode,
            });
        });

    result.into_iter().collect()
}

pub(crate) fn wait_for_prev_l1_batch_state_root_unchecked(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> U256 {
    if number == L1BatchNumber(0) {
        return U256::default();
    }
    wait_for_l1_batch_state_root_unchecked(storage, number - 1)
}

// warning: if invoked for a `L1BatchNumber` of a non-existent l1 batch, will block current thread indefinitely
pub(crate) fn wait_for_l1_batch_state_root_unchecked(
    storage: &mut StorageProcessor<'_>,
    number: L1BatchNumber,
) -> U256 {
    // If the state root is not known yet, this duration will be used to back off in the while loops
    const SAFE_STATE_ROOT_INTERVAL: Duration = Duration::from_millis(100);

    let stage_started_at: Instant = Instant::now();
    loop {
        let root_hash = storage.blocks_dal().get_block_state_root(number);
        if let Some(root) = root_hash {
            vlog::trace!(
                "Waited for hash of block #{:?} took {:?}",
                number.0,
                stage_started_at.elapsed()
            );
            metrics::histogram!(
                "server.state_keeper.wait_for_prev_hash_time",
                stage_started_at.elapsed()
            );
            return h256_to_u256(root);
        }

        std::thread::sleep(SAFE_STATE_ROOT_INTERVAL);
    }
}
