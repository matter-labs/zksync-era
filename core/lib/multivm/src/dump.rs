use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};
use zksync_types::{
    block::L2BlockExecutionData, web3, L1BatchNumber, L2BlockNumber, Transaction, H256, U256,
};
use zksync_utils::u256_to_h256;
use zksync_vm_interface::storage::WriteStorage;

use crate::{
    interface::{
        storage::{InMemoryStorage, ReadStorage, StoragePtr, StorageView},
        BootloaderMemory, BytecodeCompressionError, CompressedBytecodeInfo, CurrentExecutionState,
        FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmFactory, VmInterface, VmInterfaceHistoryEnabled,
        VmMemoryMetrics,
    },
    vm_fast, vm_latest, HistoryMode,
};

/// VM that tracks decommitment of bytecodes during execution. This is required to create a [`VmDump`].
pub(crate) trait VmTrackingContracts: VmInterface {
    /// Returns hashes of all decommitted bytecodes.
    fn used_contract_hashes(&self) -> Vec<U256>;
}

impl<S: WriteStorage, H: HistoryMode> VmTrackingContracts for vm_latest::Vm<S, H> {
    fn used_contract_hashes(&self) -> Vec<U256> {
        self.get_used_contracts()
    }
}

impl<S: ReadStorage> VmTrackingContracts for vm_fast::Vm<S> {
    fn used_contract_hashes(&self) -> Vec<U256> {
        self.decommitted_hashes().collect()
    }
}

/// Handler for [`VmDump`].
pub type VmDumpHandler = Arc<dyn Fn(VmDump) + Send + Sync>;

// FIXME: use storage snapshots (https://github.com/matter-labs/zksync-era/pull/2724)
/// Part of the VM dump representing the storage oracle for a particular VM run.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct VmStorageDump {
    /// Only existing keys (ones with an assigned enum index) are recorded.
    pub(crate) read_storage_keys: HashMap<H256, (H256, u64)>,
    /// Repeatedly written keys together with their enum indices. Only includes keys that were not read
    /// (i.e., absent from `read_storage_keys`). Since `InMemoryStorage` returns `is_write_initial == true`
    /// for all unknown keys, we don't need to record initial writes.
    ///
    /// Al least when the Fast VM is involved, this map always looks to be empty because the VM reads values
    /// for all written keys (i.e., they all will be added to `read_storage_keys`).
    pub(crate) repeated_writes: HashMap<H256, u64>,
    pub(crate) factory_deps: HashMap<H256, web3::Bytes>,
}

impl VmStorageDump {
    /// Storage must be the one used by the VM.
    pub(crate) fn new<S: ReadStorage>(
        storage: &StoragePtr<StorageView<S>>,
        used_contract_hashes: Vec<U256>,
    ) -> Self {
        let mut storage = storage.borrow_mut();
        let storage_cache = storage.cache();
        let read_storage_keys: HashMap<_, _> = storage_cache
            .read_storage_keys()
            .into_iter()
            .filter_map(|(key, value)| {
                let enum_index = storage.get_enumeration_index(&key)?;
                Some((key.hashed_key(), (value, enum_index)))
            })
            .collect();
        let repeated_writes = storage_cache
            .initial_writes()
            .into_iter()
            .filter_map(|(key, is_initial)| {
                let hashed_key = key.hashed_key();
                if !is_initial && !read_storage_keys.contains_key(&hashed_key) {
                    let enum_index = storage.get_enumeration_index(&key)?;
                    Some((hashed_key, enum_index))
                } else {
                    None
                }
            })
            .collect();

        let factory_deps = used_contract_hashes
            .into_iter()
            .filter_map(|hash| {
                let hash = u256_to_h256(hash);
                Some((hash, web3::Bytes(storage.load_factory_dep(hash)?)))
            })
            .collect();
        Self {
            read_storage_keys,
            repeated_writes,
            factory_deps,
        }
    }

    pub(crate) fn into_storage(self) -> InMemoryStorage {
        let mut storage = InMemoryStorage::default();
        for (key, (value, enum_index)) in self.read_storage_keys {
            storage.set_value_hashed_enum(key, enum_index, value);
        }
        for (key, enum_index) in self.repeated_writes {
            // The value shouldn't be read by the VM, so it doesn't matter.
            storage.set_value_hashed_enum(key, enum_index, H256::zero());
        }
        for (hash, bytecode) in self.factory_deps {
            storage.store_factory_dep(hash, bytecode.0);
        }
        storage
    }
}

/// VM dump allowing to re-run the VM on the same inputs. Opaque, but can be (de)serialized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(PartialEq))]
pub struct VmDump {
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    pub(crate) l2_blocks: Vec<L2BlockExecutionData>,
    #[serde(flatten)]
    pub(crate) storage: VmStorageDump,
}

impl VmDump {
    pub fn l1_batch_number(&self) -> L1BatchNumber {
        self.l1_batch_env.number
    }

    /// Plays back this dump on the specified VM. This prints some debug data to stdout, so should only be used in tests.
    pub fn play_back<Vm: VmFactory<StorageView<InMemoryStorage>>>(self) -> Vm {
        let storage = self.storage.into_storage();
        let storage = StorageView::new(storage).to_rc_ptr();
        let mut vm = Vm::new(self.l1_batch_env, self.system_env, storage);
        Self::play_back_blocks(self.l2_blocks, &mut vm);
        vm
    }

    pub(crate) fn play_back_blocks(
        l2_blocks: Vec<L2BlockExecutionData>,
        vm: &mut impl VmInterface,
    ) {
        for (i, l2_block) in l2_blocks.into_iter().enumerate() {
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
        vm.finish_batch();
    }
}

#[derive(Debug, Clone, Copy)]
struct L2BlocksSnapshot {
    block_count: usize,
    tx_count_in_last_block: usize,
}

#[derive(Debug)]
pub(crate) struct DumpingVm<S, Vm> {
    storage: StoragePtr<StorageView<S>>,
    inner: Vm,
    l1_batch_env: L1BatchEnv,
    system_env: SystemEnv,
    l2_blocks: Vec<L2BlockExecutionData>,
    l2_blocks_snapshot: Option<L2BlocksSnapshot>,
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
            storage: VmStorageDump::new(&self.storage, self.inner.used_contract_hashes()),
        }
    }
}

impl<S: ReadStorage, Vm: VmTrackingContracts> VmInterface for DumpingVm<S, Vm> {
    type TracerDispatcher = Vm::TracerDispatcher;

    fn push_transaction(&mut self, tx: Transaction) {
        self.record_transaction(tx.clone());
        self.inner.push_transaction(tx);
    }

    fn execute(&mut self, execution_mode: VmExecutionMode) -> VmExecutionResultAndLogs {
        self.inner.execute(execution_mode)
    }

    fn inspect(
        &mut self,
        dispatcher: Self::TracerDispatcher,
        execution_mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.inner.inspect(dispatcher, execution_mode)
    }

    fn get_bootloader_memory(&self) -> BootloaderMemory {
        self.inner.get_bootloader_memory()
    }

    fn get_last_tx_compressed_bytecodes(&self) -> Vec<CompressedBytecodeInfo> {
        self.inner.get_last_tx_compressed_bytecodes()
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

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.inner.get_current_execution_state()
    }

    fn execute_transaction_with_bytecode_compression(
        &mut self,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        self.record_transaction(tx.clone());
        self.inner
            .execute_transaction_with_bytecode_compression(tx, with_compression)
    }

    fn inspect_transaction_with_bytecode_compression(
        &mut self,
        tracer: Self::TracerDispatcher,
        tx: Transaction,
        with_compression: bool,
    ) -> (
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    ) {
        self.record_transaction(tx.clone());
        self.inner
            .inspect_transaction_with_bytecode_compression(tracer, tx, with_compression)
    }

    fn record_vm_memory_metrics(&self) -> VmMemoryMetrics {
        self.inner.record_vm_memory_metrics()
    }

    fn gas_remaining(&self) -> u32 {
        self.inner.gas_remaining()
    }

    fn finish_batch(&mut self) -> FinishedL1Batch {
        self.inner.finish_batch()
    }
}

impl<S, Vm> VmInterfaceHistoryEnabled for DumpingVm<S, Vm>
where
    S: ReadStorage,
    Vm: VmInterfaceHistoryEnabled + VmTrackingContracts,
{
    fn make_snapshot(&mut self) {
        self.l2_blocks_snapshot = Some(L2BlocksSnapshot {
            block_count: self.l2_blocks.len(),
            tx_count_in_last_block: self.last_block_mut().txs.len(),
        });
        self.inner.make_snapshot();
    }

    fn rollback_to_the_latest_snapshot(&mut self) {
        self.inner.rollback_to_the_latest_snapshot();
        let snapshot = self
            .l2_blocks_snapshot
            .take()
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
        self.l2_blocks_snapshot = None;
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
            l2_blocks_snapshot: None,
            storage,
            inner,
        }
    }
}
