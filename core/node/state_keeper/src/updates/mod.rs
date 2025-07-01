use zksync_contracts::BaseSystemContractsHashes;
use zksync_multivm::{
    interface::{Call, FinishedL1Batch, VmExecutionMetrics, VmExecutionResultAndLogs},
    utils::{get_batch_base_fee, StorageWritesDeduplicator},
};
use zksync_types::{
    commitment::PubdataParams, fee_model::BatchFeeInput, Address, L1BatchNumber, L2BlockNumber,
    ProtocolVersionId, Transaction, H256,
};

pub(crate) use self::{l1_batch_updates::L1BatchUpdates, l2_block_updates::L2BlockUpdates};
use super::{
    io::{BatchInitParams, IoCursor, L2BlockParams},
    metrics::{BATCH_TIP_METRICS, UPDATES_MANAGER_METRICS},
};
use crate::updates::l2_block_updates::RollingTxHashUpdates;

pub mod l1_batch_updates;
pub mod l2_block_updates;

/// Most of the information needed to seal the l1 batch / L2 block is contained within the VM.
///
/// Things that are not captured there are accumulated externally.
/// `L2BlockUpdates` keeps updates for the pending L2 block.
/// `L1BatchUpdates` keeps updates for the already sealed L2 blocks of the pending L1 batch.
/// `UpdatesManager` manages the state of both of these accumulators to be consistent
/// and provides information about the pending state of the current L1 batch.
#[derive(Debug)]
pub struct UpdatesManager {
    batch_timestamp: u64,
    pub fee_account_address: Address,
    pub batch_fee_input: BatchFeeInput,
    base_fee_per_gas: u64,
    base_system_contract_hashes: BaseSystemContractsHashes,
    protocol_version: ProtocolVersionId,
    pub l1_batch: L1BatchUpdates,
    pub l2_block: L2BlockUpdates,
    pub rolling_tx_hash_updates: RollingTxHashUpdates,
    pub storage_writes_deduplicator: StorageWritesDeduplicator,
    pubdata_params: PubdataParams,
    pubdata_limit: Option<u64>,
    next_l2_block_params: Option<L2BlockParams>,
    previous_batch_protocol_version: ProtocolVersionId,
}

impl UpdatesManager {
    pub(crate) fn new(
        batch_init_params: &BatchInitParams,
        previous_batch_protocol_version: ProtocolVersionId,
    ) -> Self {
        let protocol_version = batch_init_params.system_env.version;
        Self {
            batch_timestamp: batch_init_params.l1_batch_env.timestamp,
            fee_account_address: batch_init_params.l1_batch_env.fee_account,
            batch_fee_input: batch_init_params.l1_batch_env.fee_input,
            base_fee_per_gas: get_batch_base_fee(
                &batch_init_params.l1_batch_env,
                protocol_version.into(),
            ),
            protocol_version,
            base_system_contract_hashes: batch_init_params
                .system_env
                .base_system_smart_contracts
                .hashes(),
            l1_batch: L1BatchUpdates::new(batch_init_params.l1_batch_env.number),
            l2_block: L2BlockUpdates::new(
                batch_init_params.timestamp_ms,
                L2BlockNumber(batch_init_params.l1_batch_env.first_l2_block.number),
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .prev_block_hash,
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .max_virtual_blocks_to_create,
                protocol_version,
                batch_init_params
                    .l1_batch_env
                    .first_l2_block
                    .interop_roots
                    .clone(),
            ),
            rolling_tx_hash_updates: RollingTxHashUpdates {
                rolling_hash: H256::zero(),
            },
            storage_writes_deduplicator: StorageWritesDeduplicator::new(),
            pubdata_params: batch_init_params.pubdata_params,
            pubdata_limit: batch_init_params.pubdata_limit,
            next_l2_block_params: None,
            previous_batch_protocol_version,
        }
    }

    pub(crate) fn batch_timestamp(&self) -> u64 {
        self.batch_timestamp
    }

    pub fn base_system_contract_hashes(&self) -> BaseSystemContractsHashes {
        self.base_system_contract_hashes
    }

    pub(crate) fn next_l2_block_timestamp_ms_mut(&mut self) -> Option<&mut u64> {
        self.next_l2_block_params
            .as_mut()
            .map(|params| params.timestamp_ms_mut())
    }

    pub(crate) fn get_next_l2_block_or_batch_timestamp(&mut self) -> u64 {
        if let Some(next_l2_block_params) = &self.next_l2_block_params {
            return next_l2_block_params.timestamp();
        }
        self.l2_block.timestamp()
    }

    pub(crate) fn has_next_block_params(&self) -> bool {
        self.next_l2_block_params.is_some()
    }

    pub(crate) fn io_cursor(&self) -> IoCursor {
        IoCursor {
            next_l2_block: self.l2_block.number + 1,
            prev_l2_block_hash: self.l2_block.get_l2_block_hash(),
            prev_l2_block_timestamp: self.l2_block.timestamp(),
            l1_batch: self.l1_batch.number,
            prev_l1_batch_timestamp: self.batch_timestamp,
        }
    }

    pub(crate) fn seal_l2_block_command(
        &self,
        l2_legacy_shared_bridge_addr: Option<Address>,
        pre_insert_data: bool,
    ) -> L2BlockSealCommand {
        L2BlockSealCommand {
            l1_batch_number: self.l1_batch.number,
            l2_block: self.l2_block.clone(),
            first_tx_index: self.l1_batch.executed_transaction_hashes.len(),
            fee_account_address: self.fee_account_address,
            fee_input: self.batch_fee_input,
            base_fee_per_gas: self.base_fee_per_gas,
            base_system_contracts_hashes: self.base_system_contract_hashes,
            protocol_version: Some(self.protocol_version),
            l2_legacy_shared_bridge_addr,
            pre_insert_data,
            pubdata_params: self.pubdata_params,
            rolling_txs_hash: self.rolling_tx_hash_updates.rolling_hash,
        }
    }

    pub fn protocol_version(&self) -> ProtocolVersionId {
        self.protocol_version
    }

    pub fn previous_batch_protocol_version(&self) -> ProtocolVersionId {
        self.previous_batch_protocol_version
    }

    pub fn extend_from_executed_transaction(
        &mut self,
        tx: Transaction,
        tx_execution_result: VmExecutionResultAndLogs,
        execution_metrics: VmExecutionMetrics,
        call_traces: Vec<Call>,
    ) {
        let latency = UPDATES_MANAGER_METRICS
            .extend_from_executed_transaction
            .start();
        self.storage_writes_deduplicator
            .apply(&tx_execution_result.logs.storage_logs);

        self.rolling_tx_hash_updates
            .append_rolling_hash(tx.hash(), !tx_execution_result.result.is_failed());
        self.l2_block.extend_from_executed_transaction(
            tx,
            tx_execution_result,
            execution_metrics,
            call_traces,
        );
        latency.observe();
    }

    pub fn finish_batch(&mut self, finished_batch: FinishedL1Batch) {
        let latency = UPDATES_MANAGER_METRICS.finish_batch.start();
        assert!(
            self.l1_batch.finished.is_none(),
            "Cannot finish already finished batch"
        );

        let result = &finished_batch.block_tip_execution_result;
        let batch_tip_execution_metrics = result.get_execution_metrics();

        let before = self.storage_writes_deduplicator.metrics();
        self.storage_writes_deduplicator
            .apply(&result.logs.storage_logs);
        let after = self.storage_writes_deduplicator.metrics();
        BATCH_TIP_METRICS.observe_writes_metrics(&before, &after, self.protocol_version());

        self.l2_block
            .extend_from_fictive_transaction(result.clone(), batch_tip_execution_metrics);
        self.l1_batch.finished = Some(finished_batch);

        latency.observe();
    }

    /// Pushes a new L2 block with the specified timestamp into this manager. The previously
    /// held L2 block is considered sealed and is used to extend the L1 batch data.
    pub fn push_l2_block(&mut self) {
        let next_l2_block_params = self
            .next_l2_block_params
            .take()
            .expect("next l2 block params cannot be empty");
        let new_l2_block_updates = L2BlockUpdates::new(
            next_l2_block_params.timestamp_ms(),
            self.l2_block.number + 1,
            self.l2_block.get_l2_block_hash(),
            next_l2_block_params.virtual_blocks(),
            self.protocol_version,
            next_l2_block_params.interop_roots().to_vec(),
        );
        let old_l2_block_updates = std::mem::replace(&mut self.l2_block, new_l2_block_updates);
        self.l1_batch
            .extend_from_sealed_l2_block(old_l2_block_updates);
    }

    pub fn set_next_l2_block_params(&mut self, mut l2_block_params: L2BlockParams) {
        assert!(
            self.next_l2_block_params.is_none(),
            "next_l2_block_params cannot be set twice"
        );
        // We need to filter already applied interop roots. Because we seal L2 blocks in async manner,
        // it's possible that database returns already applied interop roots
        let mut interop_roots = vec![];
        for interop_root in l2_block_params.interop_roots() {
            if !self.l1_batch.interop_roots.contains(interop_root) {
                interop_roots.push(interop_root.clone());
                self.l1_batch.interop_roots.push(interop_root.clone());
            }
        }
        l2_block_params.set_interop_roots(interop_roots);
        self.next_l2_block_params = Some(l2_block_params);
    }

    pub fn get_next_l2_block_params(&mut self) -> Option<L2BlockParams> {
        self.next_l2_block_params.clone()
    }

    pub(crate) fn pending_executed_transactions_len(&self) -> usize {
        self.l1_batch.executed_transaction_hashes.len() + self.l2_block.executed_transactions.len()
    }

    pub(crate) fn pending_l1_transactions_len(&self) -> usize {
        self.l1_batch.l1_tx_count + self.l2_block.l1_tx_count
    }

    pub(crate) fn pending_execution_metrics(&self) -> VmExecutionMetrics {
        self.l1_batch.block_execution_metrics + self.l2_block.block_execution_metrics
    }

    pub(crate) fn pending_txs_encoding_size(&self) -> usize {
        self.l1_batch.txs_encoding_size + self.l2_block.txs_encoding_size
    }

    pub fn pubdata_limit(&self) -> Option<u64> {
        self.pubdata_limit
    }
}

/// Command to seal an L2 block containing all necessary data for it.
#[derive(Debug)]
pub struct L2BlockSealCommand {
    pub l1_batch_number: L1BatchNumber,
    pub l2_block: L2BlockUpdates,
    pub first_tx_index: usize,
    pub fee_account_address: Address,
    pub fee_input: BatchFeeInput,
    pub base_fee_per_gas: u64,
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub protocol_version: Option<ProtocolVersionId>,
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    /// Whether transactions or interop roots should be pre-inserted to DB.
    /// Should be set to `true` for EN's IO as EN doesn't store transactions and interop roots in DB
    /// before they are included into L2 blocks.
    pub pre_insert_data: bool,
    pub pubdata_params: PubdataParams,
    pub rolling_txs_hash: H256,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{create_execution_result, create_transaction, create_updates_manager};

    #[test]
    fn apply_l2_block() {
        // Init accumulators.
        let mut updates_manager = create_updates_manager();
        assert_eq!(updates_manager.pending_executed_transactions_len(), 0);

        // Apply tx.
        let tx = create_transaction(10, 100);
        updates_manager.extend_from_executed_transaction(
            tx,
            create_execution_result([]),
            VmExecutionMetrics::default(),
            vec![],
        );

        // Check that only pending state is updated.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.l2_block.executed_transactions.len(), 1);
        assert_eq!(
            updates_manager.l1_batch.executed_transaction_hashes.len(),
            0
        );

        // Seal an L2 block.
        updates_manager.set_next_l2_block_params(L2BlockParams::new(2000));
        updates_manager.push_l2_block();

        // Check that L1 batch updates are the same with the pending state
        // and L2 block updates are empty.
        assert_eq!(updates_manager.pending_executed_transactions_len(), 1);
        assert_eq!(updates_manager.l2_block.executed_transactions.len(), 0);
        assert_eq!(
            updates_manager.l1_batch.executed_transaction_hashes.len(),
            1
        );
    }
}
