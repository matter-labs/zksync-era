use std::{any::Any, collections::HashSet, fmt, rc::Rc};

use zksync_types::{
    h256_to_u256, l2::L2Tx, writes::StateDiffRecord, StorageKey, Transaction, H160, H256, U256,
};
use zksync_vm2::interface::{Event, HeapId, StateInterface, Tracer};

use super::{FullValidationTracer, ValidationTracer, Vm};
use crate::{
    interface::{
        pubdata::{PubdataBuilder, PubdataInput},
        storage::{ImmutableStorageView, InMemoryStorage, ReadStorage, StorageView},
        tracer::ViolatedValidationRule,
        Call, CurrentExecutionState, InspectExecutionMode, L2BlockEnv, VmExecutionMode,
        VmExecutionResultAndLogs, VmInterface,
    },
    versions::testonly::{
        validation_params, TestedVm, TestedVmForValidation, TestedVmWithCallTracer,
        TestedVmWithStorageLimit,
    },
    vm_fast::{
        tracers::WithBuiltinTracers, CallTracer, FastValidationTracer, StorageInvocationsTracer,
    },
};

mod account_validation_rules;
mod block_tip;
mod bootloader;
mod bytecode_publishing;
mod call_tracer;
mod circuits;
mod code_oracle;
mod default_aa;
mod evm;
mod gas_limit;
mod get_used_contracts;
mod is_write_initial;
mod l1_messenger;
mod l1_tx_execution;
mod l2_blocks;
mod mock_evm;
mod nonce_holder;
mod precompiles;
mod refunds;
mod require_eip712;
mod rollbacks;
mod secp256r1;
mod simple_execution;
mod storage;
mod tracing_execution_error;
mod transfer;
mod upgrade;

trait ObjectSafeEq: fmt::Debug + AsRef<dyn Any> {
    fn eq(&self, other: &dyn ObjectSafeEq) -> bool;
}

#[derive(Debug)]
struct BoxedEq<T>(T);

impl<T: 'static> AsRef<dyn Any> for BoxedEq<T> {
    fn as_ref(&self) -> &dyn Any {
        &self.0
    }
}

impl<T: fmt::Debug + PartialEq + 'static> ObjectSafeEq for BoxedEq<T> {
    fn eq(&self, other: &dyn ObjectSafeEq) -> bool {
        let Some(other) = other.as_ref().downcast_ref::<T>() else {
            return false;
        };
        self.0 == *other
    }
}

// TODO this doesn't include all the state of ModifiedWorld
#[derive(Debug)]
pub(crate) struct VmStateDump {
    state: Box<dyn ObjectSafeEq>,
    storage_writes: Vec<((H160, U256), U256)>,
    events: Box<[Event]>,
}

impl PartialEq for VmStateDump {
    fn eq(&self, other: &Self) -> bool {
        self.state.as_ref().eq(other.state.as_ref())
            && self.storage_writes == other.storage_writes
            && self.events == other.events
    }
}

pub(crate) type TestedFastVm<Tr, Val> = Vm<ImmutableStorageView<InMemoryStorage>, Tr, Val>;

impl<Tr, Val> TestedVm for TestedFastVm<Tr, Val>
where
    Tr: 'static + Tracer + Default + fmt::Debug,
    Val: 'static + ValidationTracer + fmt::Debug,
{
    type StateDump = VmStateDump;

    fn dump_state(&self) -> Self::StateDump {
        VmStateDump {
            state: Box::new(BoxedEq(self.inner.dump_state())),
            storage_writes: self.inner.get_storage_state().collect(),
            events: self.inner.events().collect(),
        }
    }

    fn gas_remaining(&mut self) -> u32 {
        self.gas_remaining()
    }

    fn get_current_execution_state(&self) -> CurrentExecutionState {
        self.get_current_execution_state()
    }

    fn decommitted_hashes(&self) -> HashSet<U256> {
        self.decommitted_hashes().collect()
    }

    fn finish_batch_with_state_diffs(
        &mut self,
        diffs: Vec<StateDiffRecord>,
        pubdata_builder: Rc<dyn PubdataBuilder>,
    ) -> VmExecutionResultAndLogs {
        self.enforce_state_diffs(diffs);
        self.finish_batch(pubdata_builder)
            .block_tip_execution_result
    }

    fn finish_batch_without_pubdata(&mut self) -> VmExecutionResultAndLogs {
        self.inspect_inner(&mut Default::default(), VmExecutionMode::Batch, None)
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
            &mut WithBuiltinTracers::mock(),
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

    fn pubdata_input(&self) -> PubdataInput {
        self.bootloader_state.get_pubdata_information().clone()
    }
}

impl TestedVmForValidation for TestedFastVm<(), FullValidationTracer> {
    fn run_validation(
        &mut self,
        tx: L2Tx,
        timestamp: u64,
    ) -> (VmExecutionResultAndLogs, Option<ViolatedValidationRule>) {
        let validation_params = validation_params(&tx, &self.system_env);
        self.push_transaction(tx.into());
        let mut tracer = ((), FullValidationTracer::new(validation_params, timestamp));
        let result = self.inspect(&mut tracer, InspectExecutionMode::OneTx);
        (result, tracer.1.validation_error())
    }
}

impl TestedVmWithCallTracer for TestedFastVm<CallTracer, FastValidationTracer> {
    fn inspect_with_call_tracer(&mut self) -> (VmExecutionResultAndLogs, Vec<Call>) {
        let mut tracer = (CallTracer::default(), FastValidationTracer::default());
        let result = self.inspect(&mut tracer, InspectExecutionMode::OneTx);
        (result, tracer.0.into_result())
    }
}

type TestStorageLimiter = StorageInvocationsTracer<StorageView<InMemoryStorage>>;

impl TestedVmWithStorageLimit for TestedFastVm<TestStorageLimiter, FastValidationTracer> {
    fn execute_with_storage_limit(&mut self, limit: usize) -> VmExecutionResultAndLogs {
        let storage = self.world.storage.to_rc_ptr();
        let mut tracer = (
            StorageInvocationsTracer::new(storage, limit),
            FastValidationTracer::default(),
        );
        self.inspect(&mut tracer, InspectExecutionMode::OneTx)
    }
}
