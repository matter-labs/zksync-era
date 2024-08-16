use std::marker::PhantomData;

use zk_evm_1_5_0::{aux_structures::Timestamp, zkevm_opcode_defs::{FatPointer, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER}};
use zksync_contracts::known_code_storage_contract;
use zksync_state::WriteStorage;
use zksync_types::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_utils::{bytes_to_be_words, h256_to_u256};

use crate::{interface::{dyn_tracers::vm_1_5_0::DynTracer, tracer::TracerExecutionStatus}, vm_latest::{utils::hash_evm_bytecode, BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState}};

use super::{traits::VmTracer, utils::read_pointer};

/// Tracer responsible for collecting information about EVM deploys and providing those
/// to the code decommitter.
#[derive(Debug, Clone)]
pub(crate) struct EvmDeployTracer<S> {
    _phantom: PhantomData<S>,
}

impl<S> EvmDeployTracer<S> {
    pub(crate) fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for EvmDeployTracer<S> {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for EvmDeployTracer<S> {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        // We check if ContractDeployer was called with provided evm bytecode.
        // It is assumed that by that time the user has already paid for its size.
        // So even if we do not revert the addition of the this bytecode it is not a ddos vector, since
        // the payment is the same as if the bytecode publication was reverted.

        let current_callstack = &state.local_state.callstack.current;

        // Here we assume that the only case when PC is 0 at the start of the execution of the contract.
        let known_code_storage_call = current_callstack.this_address == KNOWN_CODES_STORAGE_ADDRESS
            && current_callstack.pc == 0
            && current_callstack.msg_sender == CONTRACT_DEPLOYER_ADDRESS;

        if !known_code_storage_call {
            // Just continue executing
            return TracerExecutionStatus::Continue;
        }

        // Now, we need to check whether it is indeed a call to publish EVM code.
        let calldata_ptr =
            state.local_state.registers[CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER as usize];

        let data = read_pointer(&state.memory, FatPointer::from_u256(calldata_ptr.value));

        let contract = known_code_storage_contract();

        if data.len() < 4 {
            // Not interested
            return TracerExecutionStatus::Continue;
        }

        let (signature, data) = data.split_at(4);

        if signature
            != contract
                .function("publishEVMBytecode")
                .unwrap()
                .short_signature()
        {
            // Not interested
            return TracerExecutionStatus::Continue;
        }

        let Ok(call_params) = contract
            .function("publishEVMBytecode")
            .unwrap()
            .decode_input(data)
        else {
            // Not interested
            return TracerExecutionStatus::Continue;
        };

        let published_bytecode = call_params[0].clone().into_bytes().unwrap();

        let hash = hash_evm_bytecode(&published_bytecode);
        let as_words = bytes_to_be_words(published_bytecode);

        state.decommittment_processor.populate(
            vec![(h256_to_u256(hash), as_words)],
            Timestamp(state.local_state.timestamp),
        );

        TracerExecutionStatus::Continue
    }
}
