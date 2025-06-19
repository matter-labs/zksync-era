use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};

use serde::{Deserialize, Serialize};
use zksync_types::{block::L2BlockExecutionData, L1BatchNumber, L2BlockNumber, Transaction, H256};

use crate::{
    pubdata::PubdataBuilder,
    storage::{ReadStorage, StoragePtr, StorageSnapshot, StorageView},
    BytecodeCompressionResult, FinishedL1Batch, InspectExecutionMode, L1BatchEnv, L2BlockEnv,
    PushTransactionResult, SystemEnv, VmExecutionResultAndLogs, VmFactory, VmInterface,
    VmInterfaceExt, VmInterfaceHistoryEnabled, VmTrackingContracts,
};

fn create_storage_snapshot<S: ReadStorage>(
    storage: &StoragePtr<StorageView<S>>,
    used_contract_hashes: Vec<H256>,
) -> StorageSnapshot {
    let mut storage = storage.borrow_mut();
    let storage_cache = storage.cache();
    let mut storage_slots: HashMap<_, _> = storage_cache
        .read_storage_keys()
        .into_iter()
        .map(|(key, value)| {
            let enum_index = storage.get_enumeration_index(&key);
            let value_and_index = compress_value_and_index(value, enum_index);
            (key.hashed_key(), value_and_index)
        })
        .collect();

    // Normally, all writes are internally read in order to calculate their gas costs, so the code below
    // is defensive programming.
    for (key, _) in storage_cache.initial_writes() {
        let hashed_key = key.hashed_key();
        if storage_slots.contains_key(&hashed_key) {
            continue;
        }

        let value = storage.read_value(&key);
        let enum_index = storage.get_enumeration_index(&key);
        let value_and_index = compress_value_and_index(value, enum_index);
        storage_slots.insert(hashed_key, value_and_index);
    }

    let factory_deps = used_contract_hashes
        .into_iter()
        .filter_map(|hash| Some((hash, storage.load_factory_dep(hash)?)))
        .collect();

    StorageSnapshot::new(storage_slots, factory_deps)
}

/// Compresses a value + enum index into an `Option<_>` so that it's more efficiently serializable.
fn compress_value_and_index(value: H256, enum_index: Option<u64>) -> Option<(H256, u64)> {
    match (value, enum_index) {
        (value, Some(idx)) => Some((value, idx)),
        (value, None) if value.is_zero() => None,
        // There may be non-zero values w/o an assigned enum index if the VM execution
        // starts in the middle of an L1 batch. We mark such values with an enum index 0, which is not a legal value.
        (value, None) => Some((value, 0)),
    }
}

/// VM dump allowing to re-run the VM on the same inputs. Can be (de)serialized.
/// Note, dump is not capable of finishing batch in terms of VM execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VmDump {
    pub l1_batch_env: L1BatchEnv,
    pub system_env: SystemEnv,
    pub l2_blocks: Vec<L2BlockExecutionData>,
    pub storage: StorageSnapshot,
}

impl VmDump {
    pub fn l1_batch_number(&self) -> L1BatchNumber {
        self.l1_batch_env.number
    }

    /// Plays back this dump on the specified VM.
    pub fn play_back<Vm>(self) -> Vm
    where
        Vm: VmFactory<StorageView<StorageSnapshot>>,
    {
        self.play_back_custom(Vm::new)
    }

    /// Plays back this dump on a VM created using the provided closure.
    #[doc(hidden)] // too low-level
    pub fn play_back_custom<Vm: VmInterface>(
        self,
        create_vm: impl FnOnce(L1BatchEnv, SystemEnv, StoragePtr<StorageView<StorageSnapshot>>) -> Vm,
    ) -> Vm {
        let storage = StorageView::new(self.storage).to_rc_ptr();
        let mut vm = create_vm(self.l1_batch_env, self.system_env, storage);

        for (i, l2_block) in self.l2_blocks.into_iter().enumerate() {
            if i > 0 {
                // First block is already set.
                vm.start_new_l2_block(L2BlockEnv {
                    number: l2_block.number.0,
                    timestamp: l2_block.timestamp,
                    prev_block_hash: l2_block.prev_block_hash,
                    max_virtual_blocks_to_create: l2_block.virtual_blocks,
                });
            }

            for tx in l2_block.txs {
                let tx_hash = tx.hash();
                let (compression_result, _) =
                    vm.execute_transaction_with_bytecode_compression(tx, true);
                if let Err(err) = compression_result {
                    panic!("Failed compressing bytecodes for transaction {tx_hash:?}: {err}");
                }
            }
        }
        vm
    }
}

#[derive(Debug, Clone, Copy)]
struct L2BlocksSnapshot {
    block_count: usize,
    tx_count_in_last_block: usize,
}

/// VM wrapper that can create [`VmDump`]s during execution.
#[derive(Debug)]
pub(super) struct DumpingVm<S, Vm> {
    storage: StoragePtr<StorageView<S>>,
    inner: Vm,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
    l2_blocks: Vec<L2BlockExecutionData>,
    l2_blocks_snapshot: VecDeque<L2BlocksSnapshot>,
}

impl<S: ReadStorage, Vm: VmTrackingContracts> DumpingVm<S, Vm> {
    fn last_block_mut(&mut self) -> &mut L2BlockExecutionData {
        self.l2_blocks.last_mut().unwrap()
    }

    fn record_transaction(&mut self, tx: Transaction) {
        self.last_block_mut().txs.push(tx);
    }

    pub fn dump_state(&self) -> VmDump {
        VmDump {
            l1_batch_env: self.l1_batch_env.clone(),
            system_env: self.system_env.clone(),
            l2_blocks: self.l2_blocks.clone(),
            storage: create_storage_snapshot(&self.storage, self.inner.used_contract_hashes()),
        }
    }
}

impl<S, Vm> AsRef<Vm> for DumpingVm<S, Vm> {
    fn as_ref(&self) -> &Vm {
        &self.inner
    }
}

impl<S, Vm> AsMut<Vm> for DumpingVm<S, Vm> {
    fn as_mut(&mut self) -> &mut Vm {
        &mut self.inner
    }
}

impl<S: ReadStorage, Vm: VmTrackingContracts> VmInterface for DumpingVm<S, Vm> {
    type TracerDispatcher = Vm::TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) -> PushTransactionResult {
        self.record_transaction(tx.clone());
        self.inner.push_transaction(tx)
    }

    fn inspect(
        &mut self,
        dispatcher: &mut Self::TracerDispatcher,
        execution_mode: InspectExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inner.inspect(dispatcher, execution_mode)
    }

    fn start_new_l2_block(&mut self, l2_block_env: L2BlockEnv) {
        self.l2_blocks.push(L2BlockExecutionData {
            number: L2BlockNumber(l2_block_env.number),
            timestamp: l2_block_env.timestamp,
            prev_block_hash: l2_block_env.prev_block_hash,
            virtual_blocks: l2_block_env.max_virtual_blocks_to_create,
            txs: vec![],
        });
        self.inner.start_new_l2_block(l2_block_env);
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: &mut Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (BytecodeCompressionResult, VmExecutionResultAndLogs) {
        self.record_transaction(tx.clone());
        self.inner
            .inspect_transaction_with_bytecode_compression(tracer, tx, with_compression)
    }

    fn finish_batch(&mut self, pubdata_builder: Rc<dyn PubdataBuilder>) -> FinishedL1Batch {
        self.inner.finish_batch(pubdata_builder)
    }
}

impl<S, Vm> VmInterfaceHistoryEnabled for DumpingVm<S, Vm>
where
    S: ReadStorage,
    Vm: VmInterfaceHistoryEnabled + VmTrackingContracts,
{
    fn make_snapshot(&mut self) {
        let snapshot = L2BlocksSnapshot {
            block_count: self.l2_blocks.len(),
            tx_count_in_last_block: self.last_block_mut().txs.len(),
        };
        self.l2_blocks_snapshot.push_back(snapshot);
        self.inner.make_snapshot();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.inner.rollback_to_the_latest_snapshot();
        let snapshot = self
            .l2_blocks_snapshot
            .pop_back()
            .expect("rollback w/o snapshot");
        self.l2_blocks.truncate(snapshot.block_count);
        assert_eq!(
            self.l2_blocks.len(),
            snapshot.block_count,
            "L2 blocks were removed after creating a snapshot"
        );
        self.last_block_mut()
            .txs
            .truncate(snapshot.tx_count_in_last_block);
    }

    fn pop_snapshot_no_rollback(&mut self) {
        self.inner.pop_snapshot_no_rollback();
        self.l2_blocks_snapshot.pop_back();
    }

    fn pop_front_snapshot_no_rollback(&mut self) {
        self.inner.pop_front_snapshot_no_rollback();
        self.l2_blocks_snapshot.pop_front();
    }
}

impl<S, Vm> VmFactory<StorageView<S>> for DumpingVm<S, Vm>
where
    S: ReadStorage,
    Vm: VmFactory<StorageView<S>> + VmTrackingContracts,
{
    fn new(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage: StoragePtr<StorageView<S>>,
    ) -> Self {
        let inner = Vm::new(l1_batch_env.clone(), system_env.clone(), storage.clone());
        let first_block = L2BlockExecutionData {
            number: L2BlockNumber(l1_batch_env.first_l2_block.number),
            timestamp: l1_batch_env.first_l2_block.timestamp,
            prev_block_hash: l1_batch_env.first_l2_block.prev_block_hash,
            virtual_blocks: l1_batch_env.first_l2_block.max_virtual_blocks_to_create,
            txs: vec![],
        };
        Self {
            l1_batch_env,
            system_env,
            l2_blocks: vec![first_block],
            l2_blocks_snapshot: VecDeque::new(),
            storage,
            inner,
        }
    }
}
