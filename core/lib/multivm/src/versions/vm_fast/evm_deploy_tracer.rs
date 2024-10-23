//! Tracer tracking deployment of EVM bytecodes during VM execution.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use zk_evm_1_5_0::zkevm_opcode_defs::CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER;
use zksync_system_constants::{CONTRACT_DEPLOYER_ADDRESS, KNOWN_CODES_STORAGE_ADDRESS};
use zksync_types::U256;
use zksync_utils::{bytecode::hash_evm_bytecode, h256_to_u256};
use zksync_vm2::{
    interface::{
        CallframeInterface, CallingMode, GlobalStateInterface, Opcode, OpcodeType, Tracer,
    },
    FatPointer,
};

#[derive(Debug, Clone, Default)]
pub(super) struct DynamicBytecodes(Rc<RefCell<HashMap<U256, Vec<u8>>>>);

impl DynamicBytecodes {
    pub fn take(&self, hash: U256) -> Option<Vec<u8>> {
        self.0.borrow_mut().remove(&hash)
    }

    fn insert(&self, hash: U256, bytecode: Vec<u8>) {
        self.0.borrow_mut().insert(hash, bytecode);
    }
}

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
}

impl Tracer for EvmDeployTracer {
    #[inline(always)]
    fn after_instruction<OP: OpcodeType, S: GlobalStateInterface>(&mut self, state: &mut S) {
        if !matches!(OP::VALUE, Opcode::FarCall(CallingMode::Normal)) {
            return;
        }

        let from = state.current_frame().caller();
        let to = state.current_frame().code_address();
        if from != CONTRACT_DEPLOYER_ADDRESS || to != KNOWN_CODES_STORAGE_ADDRESS {
            return;
        }

        let (calldata_ptr, is_pointer) =
            state.read_register(CALL_IMPLICIT_CALLDATA_FAT_PTR_REGISTER + 1);
        assert!(
            is_pointer,
            "far call convention violated: register 1 is not a pointer to calldata"
        );
        let calldata_ptr = FatPointer::from(calldata_ptr);
        assert_eq!(
            calldata_ptr.offset, 0,
            "far call convention violated: calldata fat pointer is not shrunk"
        );

        let data: Vec<_> = (calldata_ptr.start..calldata_ptr.start + calldata_ptr.length)
            .map(|addr| state.read_heap_byte(calldata_ptr.memory_page, addr))
            .collect();
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
