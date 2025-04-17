use std::{marker::PhantomData, mem};

use zk_evm_1_5_2::{
    aux_structures::Timestamp,
    tracing::{AfterExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{
        FarCallOpcode, FatPointer, Opcode, CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
    },
};
use zksync_types::{
    bytecode::BytecodeHash, CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS,
};

use super::{traits::VmTracer, utils::read_pointer};
use crate::{
    interface::{
        storage::{StoragePtr, WriteStorage},
        tracer::TracerExecutionStatus,
    },
    tracers::dynamic::vm_1_5_2::DynTracer,
    utils::bytecode::bytes_to_be_words,
    vm_latest::{bootloader::BootloaderState, HistoryMode, SimpleMemory, ZkSyncVmState},
};

/// Tracer responsible for collecting information about EVM deploys and providing those
/// to the code decommitter.
#[derive(Debug)]
pub(crate) struct EvmDeployTracer<S> {
    tracked_signature: [u8; 4],
    pending_bytecodes: Vec<(usize, Vec<u8>)>,
    _phantom: PhantomData<S>,
}

impl<S> EvmDeployTracer<S> {
    pub(crate) fn new() -> Self {
        let tracked_signature = ethabi::short_signature(
            "publishEVMBytecode",
            &[ethabi::ParamType::Uint(256), ethabi::ParamType::Bytes],
        );

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

        match ethabi::decode(
            &[ethabi::ParamType::Uint(256), ethabi::ParamType::Bytes],
            data,
        ) {
            Ok(decoded) => {
                let mut decoded_iter = decoded.into_iter();
                let raw_bytecode_len = decoded_iter.next().unwrap().into_uint().unwrap().try_into();
                match raw_bytecode_len {
                    Ok(raw_bytecode_len) => {
                        let published_bytecode = decoded_iter.next().unwrap().into_bytes().unwrap();
                        self.pending_bytecodes
                            .push((raw_bytecode_len, published_bytecode));
                    }
                    Err(err) => {
                        tracing::error!("Invalid bytecode len in `publishEVMBytecode` call: {err}")
                    }
                }
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
        let timestamp = Timestamp(state.local_state.timestamp);
        for (raw_bytecode_len, published_bytecode) in mem::take(&mut self.pending_bytecodes) {
            let hash =
                BytecodeHash::for_evm_bytecode(raw_bytecode_len, &published_bytecode).value_u256();
            let as_words = bytes_to_be_words(&published_bytecode);
            state
                .decommittment_processor
                .insert_dynamic_bytecode(hash, as_words, timestamp);
        }
        TracerExecutionStatus::Continue
    }
}
