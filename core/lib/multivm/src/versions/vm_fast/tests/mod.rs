use std::collections::HashSet;

use zksync_types::{writes::StateDiffRecord, StorageKey, Transaction, H256, U256};
use zksync_utils::h256_to_u256;
use zksync_vm2::interface::{HeapId, StateInterface};
use zksync_vm_interface::{
    storage::ReadStorage, CurrentExecutionState, L2BlockEnv, VmExecutionMode,
    VmExecutionResultAndLogs, VmInterfaceExt,
};

use super::Vm;
use crate::{
    interface::storage::{ImmutableStorageView, InMemoryStorage},
    versions::testonly::TestedVm,
    vm_fast::CircuitsTracer,
};

mod block_tip;
mod bootloader;
mod bytecode_publishing;
mod circuits;
mod code_oracle;
mod default_aa;
mod gas_limit;
mod get_used_contracts;
mod is_write_initial;
mod l1_tx_execution;
mod l2_blocks;
mod nonce_holder;
mod precompiles;
mod refunds;
mod require_eip712;
mod secp256r1;
mod simple_execution;
mod storage;
/*
// mod call_tracer; FIXME: requires tracers
// mod prestate_tracer; FIXME: is pre-state tracer still relevant?
mod rollbacks;
mod tester;
mod tracing_execution_error;
mod transfer;
mod upgrade;
mod utils;
*/

impl TestedVm for Vm<ImmutableStorageView<InMemoryStorage>> {
    fn gas_remaining(&mut self) -> u32 {
        self.gas_remaining()
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.get_current_execution_state()
    }

    fn decommitted_hashes(&self) -> HashSet<U256> {
        self.decommitted_hashes().collect()
    }

    fn execute_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        mode: VmExecutionMode,
    ) -> VmExecutionResultAndLogs {
        self.enforce_state_diffs(diffs);
        self.execute(mode)
    }

    fn insert_bytecodes(&mut self, bytecodes: &[&[u8]]) {
        self.insert_bytecodes(bytecodes.iter().copied())
    }

    fn known_bytecode_hashes(&self) -> HashSet<U256> {
        self.world.bytecode_cache.keys().copied().collect()
    }

    fn manually_decommit(&mut self, code_hash: H256) -> bool {
        let (_, is_fresh) = self.inner.world_diff_mut().decommit_opcode(
            &mut self.world,
            &mut ((), CircuitsTracer::default()),
            h256_to_u256(code_hash),
        );
        is_fresh
    }

    fn verify_required_bootloader_heap(&self, required_values: &[(u32, U256)]) {
        for &(slot, expected_value) in required_values {
            let current_value = self.inner.read_heap_u256(HeapId::FIRST, slot * 32);
            assert_eq!(current_value, expected_value);
        }
    }

    fn write_to_bootloader_heap(&mut self, cells: &[(usize, U256)]) {
        self.write_to_bootloader_heap(cells.iter().copied());
    }

    fn read_storage(&mut self, key: StorageKey) -> U256 {
        let storage_changes = self.inner.world_diff().get_storage_state();
        let main_storage = &mut self.world.storage;
        storage_changes
            .get(&(*key.account().address(), h256_to_u256(*key.key())))
            .copied()
            .unwrap_or_else(|| h256_to_u256(main_storage.read_value(&key)))
    }

    fn last_l2_block_hash(&self) -> H256 {
        self.bootloader_state.last_l2_block().get_hash()
    }

    fn push_l2_block_unchecked(&mut self, block: L2BlockEnv) {
        self.bootloader_state.push_l2_block(block);
    }

    fn push_transaction_with_refund(&mut self, tx: Transaction, refund: u64) {
        self.push_transaction_inner(tx, refund, true);
    }
}
