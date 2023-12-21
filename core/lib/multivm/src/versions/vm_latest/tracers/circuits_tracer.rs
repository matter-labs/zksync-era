use std::marker::PhantomData;

use bigdecimal::{BigDecimal, Zero};
use zk_evm_1_4_0::{
    tracing::{BeforeExecutionData, VmLocalStateData},
    zkevm_opcode_defs::{LogOpcode, Opcode, UMAOpcode},
};
use zksync_state::{StoragePtr, WriteStorage};
use zksync_system_constants::{
    ECRECOVER_PRECOMPILE_ADDRESS, KECCAK256_PRECOMPILE_ADDRESS, SHA256_PRECOMPILE_ADDRESS,
};
use zksync_types::{AccountTreeId, StorageKey};
use zksync_utils::u256_to_h256;

use super::circuits_capacity::*;
use crate::{
    interface::{dyn_tracers::vm_1_4_0::DynTracer, tracer::TracerExecutionStatus},
    vm_latest::{
        bootloader_state::BootloaderState,
        old_vm::{history_recorder::HistoryMode, memory::SimpleMemory},
        tracers::traits::VmTracer,
        types::internals::ZkSyncVmState,
    },
};

/// Tracer responsible for collecting information about refunds.
#[derive(Debug)]
pub(crate) struct CircuitsTracer<S> {
    pub(crate) estimated_circuits_used: BigDecimal,
    last_decommitment_history_entry_checked: Option<usize>,
    _phantom_data: PhantomData<S>,
}

impl<S: WriteStorage> CircuitsTracer<S> {
    pub(crate) fn new() -> Self {
        Self {
            estimated_circuits_used: BigDecimal::zero(),
            last_decommitment_history_entry_checked: None,
            _phantom_data: Default::default(),
        }
    }
}

impl<S: WriteStorage, H: HistoryMode> DynTracer<S, SimpleMemory<H>> for CircuitsTracer<S> {
    fn before_execution(
        &mut self,
        state: VmLocalStateData<'_>,
        data: BeforeExecutionData,
        _memory: &SimpleMemory<H>,
        storage: StoragePtr<S>,
    ) {
        let used = match data.opcode.variant.opcode {
            Opcode::Nop(_)
            | Opcode::Add(_)
            | Opcode::Sub(_)
            | Opcode::Mul(_)
            | Opcode::Div(_)
            | Opcode::Jump(_)
            | Opcode::Binop(_)
            | Opcode::Shift(_)
            | Opcode::Ptr(_) => RICH_ADDRESSING_OPCODE_FRACTION.clone(),
            Opcode::Context(_) | Opcode::Ret(_) | Opcode::NearCall(_) => {
                AVERAGE_OPCODE_FRACTION.clone()
            }
            Opcode::Log(LogOpcode::StorageRead) => STORAGE_READ_FRACTION.clone(),
            Opcode::Log(LogOpcode::StorageWrite) => {
                let storage_key = StorageKey::new(
                    AccountTreeId::new(state.vm_local_state.callstack.current.this_address),
                    u256_to_h256(data.src0_value.value),
                );

                let first_write = !storage
                    .borrow()
                    .modified_storage_keys()
                    .contains_key(&storage_key);
                if first_write {
                    COLD_STORAGE_WRITE_FRACTION.clone()
                } else {
                    HOT_STORAGE_WRITE_FRACTION.clone()
                }
            }
            Opcode::Log(LogOpcode::ToL1Message) | Opcode::Log(LogOpcode::Event) => {
                EVENT_OR_L1_MESSAGE_FRACTION.clone()
            }
            Opcode::Log(LogOpcode::PrecompileCall) => {
                match state.vm_local_state.callstack.current.this_address {
                    a if a == KECCAK256_PRECOMPILE_ADDRESS => {
                        let precompile_interpreted = data.src0_value.value.0[3];
                        PRECOMPILE_CALL_COMMON_FRACTION.clone()
                            + BigDecimal::from(precompile_interpreted)
                                * KECCAK256_CYCLE_FRACTION.clone()
                    }
                    a if a == SHA256_PRECOMPILE_ADDRESS => {
                        let precompile_interpreted = data.src0_value.value.0[3];
                        PRECOMPILE_CALL_COMMON_FRACTION.clone()
                            + BigDecimal::from(precompile_interpreted)
                                * SHA256_CYCLE_FRACTION.clone()
                    }
                    a if a == ECRECOVER_PRECOMPILE_ADDRESS => {
                        PRECOMPILE_CALL_COMMON_FRACTION.clone() + ECRECOVER_CYCLE_FRACTION.clone()
                    }
                    _ => PRECOMPILE_CALL_COMMON_FRACTION.clone(),
                }
            }
            Opcode::FarCall(_) => FAR_CALL_FRACTION.clone(),
            Opcode::UMA(UMAOpcode::AuxHeapWrite | UMAOpcode::HeapWrite) => {
                UMA_WRITE_FRACTION.clone()
            }
            Opcode::UMA(
                UMAOpcode::AuxHeapRead | UMAOpcode::HeapRead | UMAOpcode::FatPointerRead,
            ) => UMA_READ_FRACTION.clone(),
            Opcode::Invalid(_) => unreachable!(), // invalid opcodes are never executed
        };

        self.estimated_circuits_used += used;
    }
}

impl<S: WriteStorage, H: HistoryMode> VmTracer<S, H> for CircuitsTracer<S> {
    fn initialize_tracer(&mut self, state: &mut ZkSyncVmState<S, H>) {
        self.last_decommitment_history_entry_checked = Some(
            state
                .decommittment_processor
                .decommitted_code_hashes
                .history()
                .len(),
        );
    }

    fn finish_cycle(
        &mut self,
        state: &mut ZkSyncVmState<S, H>,
        _bootloader_state: &mut BootloaderState,
    ) -> TracerExecutionStatus {
        let last_decommitment_history_entry_checked = self
            .last_decommitment_history_entry_checked
            .expect("Value must be set during init");
        let history = state
            .decommittment_processor
            .decommitted_code_hashes
            .history();
        for (_, history_event) in &history[last_decommitment_history_entry_checked..] {
            // We assume that only insertions may happen during a single VM inspection.
            assert!(history_event.value.is_some());
            let bytecode_len = state
                .decommittment_processor
                .known_bytecodes
                .inner()
                .get(&history_event.key)
                .expect("Bytecode must be known at this point")
                .len();

            // Each cycle of `CodeDecommitter` processes 64 bits.
            let decommitter_cycles_used = bytecode_len / 4;
            self.estimated_circuits_used += BigDecimal::from(decommitter_cycles_used as u64)
                * CODE_DECOMMITTER_CYCLE_FRACTION.clone();
        }
        self.last_decommitment_history_entry_checked = Some(history.len());

        TracerExecutionStatus::Continue
    }
}
