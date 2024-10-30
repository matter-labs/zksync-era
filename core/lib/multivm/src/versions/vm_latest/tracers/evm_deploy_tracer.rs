use std::{marker::PhantomData, mem};

use zk_evm_1_5_0::{
    aux_structures::Timestamp,
    tracing::{AfterExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{
        FarCallOpcode, FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
    },
};
use zksync_types::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_utils::{bytes_to_be_words, h256_to_u256};
use zksync_vm_interface::storage::StoragePtr;

use super::{traits::VmTracer, utils::read_pointer};
use crate::{
    interface::{storage::WriteStorage, tracer::TracerExecutionStatus},
    tracers::dynamic::vm_1_5_0::DynTracer,
    vm_latest::{
        utils::hash_evm_bytecode, BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState,
    },
};

/// Tracer responsible for collecting information about EVM deploys and providing those
/// to the code decommitter.
#[derive(Debug)]
pub(crate) struct EvmDeployTracer<S> {
    tracked_signature: [u8; 4],
    pending_bytecodes: Vec<Vec<u8>>,
    _phantom: PhantomData<S>,
}

impl<S> EvmDeployTracer<S> {
    pub(crate) fn new() -> Self {
        let tracked_signature =
            ethabi::short_signature("publishEVMBytecode", &[ethabi::ParamType::Bytes]);

        Self {
            tracked_signature,
            pending_bytecodes: vec![],
            _phantom: PhantomData,
        }
    }
}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for EvmDeployTracer<S> {
    fn after_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: AfterExecutionData,
        memory: &SimpleMemory<H>,
        _storage: StoragePtr<S>,
    ) {
        if !matches!(
            data.opcode.variant.opcode,
            Opcode::FarCall(FarCallOpcode::Normal)
        ) {
            return;
        };

        let current = state.vm_local_state.callstack.current;
        let from = current.msg_sender;
        let to = current.this_address;
        if from != CONTRACT_DEPLOYER_ADDRESS || to != KNOWN_CODES_STORAGE_ADDRESS {
            return;
        }

        let calldata_ptr =
            state.vm_local_state.registers[usize::from(CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER)];
        let data = read_pointer(memory, FatPointer::from_u256(calldata_ptr.value));
        if data.len() < 4 {
            return;
        }
        let (signature, data) = data.split_at(4);
        if signature != self.tracked_signature {
            return;
        }

        match ethabi::decode(&[ethabi::ParamType::Bytes], data) {
            Ok(decoded) => {
                let published_bytecode = decoded.into_iter().next().unwrap().into_bytes().unwrap();
                self.pending_bytecodes.push(published_bytecode);
            }
            Err(err) => tracing::error!("Unable to decode `publishEVMBytecode` call: {err}"),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for EvmDeployTracer<S> {
    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        for published_bytecode in mem::take(&mut self.pending_bytecodes) {
            let hash = hash_evm_bytecode(&published_bytecode);
            let as_words = bytes_to_be_words(published_bytecode);

            state.decommittment_processor.populate(
                vec![(h256_to_u256(hash), as_words)],
                Timestamp(state.local_state.timestamp),
            );
        }
        TracerExecutionStatus::Continue
    }
}
