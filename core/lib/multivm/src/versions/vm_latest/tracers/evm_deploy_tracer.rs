use std::marker::PhantomData;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zk_evm_1_5_0::{
    aux_structures::Timestamp,
    tracing::{BeforeExecutionData, VmLocalStateData},
    vm_state::VmLocalState,
    zkevm_opcode_defs::{
        system_params::L1_MESSAGE_PUBDATA_BYTES, FatPointer,
        CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER,
    },
};
use zksync_contracts::known_code_storage_contract;
use zksync_state::{StoragePtr, WriteStorage};
use zksync_system_constants::{PUBLISH_BYTECODE_OVERHEAD, SYSTEM_CONTEXT_ADDRESS};
use zksync_types::{
    event::{extract_long_l2_to_l1_messages, extract_published_bytecodes},
    get_code_key,
    l2_to_l1_log::L2ToL1Log,
    L1BatchNumber, CONTRACT_DEPLOYER_ADDRESS, H256, KNOWN_CODES_STORAGE_ADDRESS, U256,
};
use zksync_utils::{bytecode::bytecode_len_in_bytes, ceil_div_u256, u256_to_h256};

use super::utils::read_pointer;
use crate::{
    interface::{
        traits::tracers::dyn_tracers::vm_1_5_0::DynTracer, types::tracer::TracerExecutionStatus,
        L1BatchEnv, Refunds,
    },
    vm_latest::{
        bootloader_state::BootloaderState,
        constants::{BOOTLOADER_HEAP_PAGE, OPERATOR_REFUNDS_OFFSET, TX_GAS_LIMIT_OFFSET},
        old_vm::{events::merge_events, history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::{
            traits::VmTracer,
            utils::{get_vm_hook_params, VmHook},
        },
        types::internals::ZkSyncVmState,
        utils::fee::get_batch_base_fee,
    },
};

/// Tracer responsible for collecting information about EVM deploys and providing those
/// to the code decommitter.
#[derive(Debug, Clone)]
pub(crate) struct EvmDeployTracer {}

impl EvmDeployTracer {}

impl<S, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for EvmDeployTracer {}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for EvmDeployTracer {
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

        let Ok(call_params) = contract
            .function("publishEVMBytecode")
            .unwrap()
            .decode_input(&data)
        else {
            // Not interested
            return TracerExecutionStatus::Continue;
        };

        let published_bytecode = call_params[0].clone().into_bytes().unwrap();

        // let bytecode_hash =

        state
            .decommittment_processor
            .populate(vec![], Timestamp(state.local_state.timestamp));

        TracerExecutionStatus::Continue
    }
}

/// Returns the given transactions' gas limit - by reading it directly from the VM memory.
pub(crate) fn pubdata_published<S: WriteStorage, H: HistoryMode>(
    state: &ZkSyncVmState<S, H>,
    storage_writes_pubdata_published: u32,
    from_timestamp: Timestamp,
    batch_number: L1BatchNumber,
) -> u32 {
    let (raw_events, l1_messages) = state
        .event_sink
        .get_events_and_l2_l1_logs_after_timestamp(from_timestamp);
    let events: Vec<_> = merge_events(raw_events)
        .into_iter()
        .map(|e| e.into_vm_event(batch_number))
        .collect();
    // For the first transaction in L1 batch there may be (it depends on the execution mode) an L2->L1 log
    // that is sent by `SystemContext` in `setNewBlock`. It's a part of the L1 batch pubdata overhead and not the transaction itself.
    let l2_l1_logs_bytes = (l1_messages
        .into_iter()
        .map(|log| L2ToL1Log {
            shard_id: log.shard_id,
            is_service: log.is_first,
            tx_number_in_block: log.tx_number_in_block,
            sender: log.address,
            key: u256_to_h256(log.key),
            value: u256_to_h256(log.value),
        })
        .filter(|log| log.sender != SYSTEM_CONTEXT_ADDRESS)
        .count() as u32)
        * L1_MESSAGE_PUBDATA_BYTES;
    let l2_l1_long_messages_bytes: u32 = extract_long_l2_to_l1_messages(&events)
        .iter()
        .map(|event| event.len() as u32)
        .sum();

    let published_bytecode_bytes: u32 = extract_published_bytecodes(&events)
        .iter()
        .map(|bytecodehash| bytecode_len_in_bytes(*bytecodehash) as u32 + PUBLISH_BYTECODE_OVERHEAD)
        .sum();

    storage_writes_pubdata_published
        + l2_l1_logs_bytes
        + l2_l1_long_messages_bytes
        + published_bytecode_bytes
}
