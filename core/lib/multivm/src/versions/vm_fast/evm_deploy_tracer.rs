//! Tracer tracking deployment of EVM bytecodes during VM execution.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_types::U256;
use zksync_utils::{bytecode::hash_evm_bytecode, h256_to_u256};
use zksync_vm2::interface::{
    CallframeInterface, CallingMode, GlobalStateInterface, Opcode, OpcodeType, ShouldStop, Tracer,
};

use super::utils::read_fat_pointer;

/// Container for dynamic bytecodes added by [`EvmDeployTracer`].
#[derive(Debug, Clone, Default)]
pub(super) struct DynamicBytecodes(Rc<RefCell<HashMap<U256, Vec<u8>>>>);

impl DynamicBytecodes {
    pub(super) fn take(&self, hash: U256) -> Option<Vec<u8>> {
        self.0.borrow_mut().remove(&hash)
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
        let tracked_signature =
            ethabi::short_signature("publishEVMBytecode", &[ethabi::ParamType::Bytes]);
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

        let data = read_fat_pointer(state, state.read_register(1).0);
        if data.len() < 4 {
            return;
        }
        let (signature, data) = data.split_at(4);
        if signature != self.tracked_signature {
            return;
        }

        match ethabi::decode(&[ethabi::ParamType::Bytes], data) {
            Ok(decoded) => {
                // `unwrap`s should be safe since the function signature is checked above.
                let published_bytecode = decoded.into_iter().next().unwrap().into_bytes().unwrap();
                let bytecode_hash = h256_to_u256(hash_evm_bytecode(&published_bytecode));
                self.bytecodes.insert(bytecode_hash, published_bytecode);
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
