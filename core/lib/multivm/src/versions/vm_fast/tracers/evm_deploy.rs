//! Tracer tracking deployment of EVM bytecodes during VM execution.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_types::{bytecode::BytecodeHash, U256};
use zksync_vm2::interface::{
    CallframeInterface, CallingMode, GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer,
};

use crate::vm_fast::utils::read_raw_fat_pointer;

/// Container for dynamic bytecodes added by [`EvmDeployTracer`].
#[derive(Debug, Clone, Default)]
pub(crate) struct DynamicBytecodes(Rc<RefCell<HashMap<U256, Vec<u8>>>>);

impl DynamicBytecodes {
    pub(crate) fn map<R>(&self, hash: U256, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        self.0.borrow().get(&hash).map(|code| f(code))
    }

    fn insert(&self, hash: U256, bytecode: Vec<u8>) {
        self.0.borrow_mut().insert(hash, bytecode);
    }
}

/// Tracer that tracks EVM bytecode deployments.
///
/// Unlike EraVM bytecodes, EVM bytecodes are *dynamic*; they are not necessarily known before transaction execution.
/// (EraVM bytecodes must be present in the storage or be mentioned in the `factory_deps` field of a transaction.)
/// Hence, it's necessary to track which EVM bytecodes were deployed so that they are persisted after VM execution.
#[derive(Debug)]
pub(super) struct EvmDeployTracer {
    tracked_signature: [u8; 4],
    bytecodes: DynamicBytecodes,
}

impl EvmDeployTracer {
    pub(super) fn new(bytecodes: DynamicBytecodes) -> Self {
        let tracked_signature = ethabi::short_signature(
            "publishEVMBytecode",
            &[ethabi::ParamType::Uint(256), ethabi::ParamType::Bytes],
        );
        Self {
            tracked_signature,
            bytecodes,
        }
    }

    fn handle_far_call(&self, state: &mut impl GlobalStateInterface) {
        let from = state.current_frame().caller();
        let to = state.current_frame().code_address();
        if from != CONTRACT_DEPLOYER_ADDRESS || to != KNOWN_CODES_STORAGE_ADDRESS {
            return;
        }

        let data = read_raw_fat_pointer(state, state.read_register(1).0);
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
                // `unwrap`s should be safe since the function signature is checked above.
                let mut decoded_iter = decoded.into_iter();
                let raw_bytecode_len = decoded_iter.next().unwrap().into_uint().unwrap().try_into();
                match raw_bytecode_len {
                    Ok(raw_bytecode_len) => {
                        let published_bytecode = decoded_iter.next().unwrap().into_bytes().unwrap();
                        let bytecode_hash =
                            BytecodeHash::for_evm_bytecode(raw_bytecode_len, &published_bytecode)
                                .value_u256();
                        self.bytecodes.insert(bytecode_hash, published_bytecode);
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

impl Tracer for EvmDeployTracer {
    #[inline(always)]
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(
        &mut self,
        state: &mut S,
    ) -> ShouldStop {
        if matches!(OP::VALUE, Opcode::FarCall(CallingMode::Normal)) {
            self.handle_far_call(state);
        }
        ShouldStop::Continue
    }
}
