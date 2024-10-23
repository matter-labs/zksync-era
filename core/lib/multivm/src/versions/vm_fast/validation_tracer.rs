use zksync_vm2::interface::{
    self, CallframeInterface, Opcode::*, OpcodeType, ReturnType::*, Tracer,
};
use zksync_vm_interface::tracer::ViolatedValidationRule;

use super::vm::TracerExt;

#[derive(Debug, Default)]
pub struct ValidationTracer {
    out_of_gas: bool,
    in_validation: bool,
}

impl Tracer for ValidationTracer {
    fn before_instruction<OP: OpcodeType, S: interface::StateInterface>(&mut self, state: &mut S) {
        if !self.in_validation {
            return;
        }
        match OP::VALUE {
            // Out of gas once means out of gas for the whole validation, as the EIP forbids handling out of gas errors
            Ret(Panic) if state.current_frame().gas() == 0 => self.out_of_gas = true,
            _ => {}
        }
    }
}

impl TracerExt for ValidationTracer {
    fn on_bootloader_hook(&mut self, hook: super::hook::Hook) {
        match hook {
            super::hook::Hook::AccountValidationEntered => self.in_validation = true,
            super::hook::Hook::ValidationExited => self.in_validation = false,
            _ => {}
        }
    }
}

impl ValidationTracer {
    pub fn probably_out_of_gas(&self) -> bool {
        self.out_of_gas
    }

    pub fn validation_error(&self) -> Option<ViolatedValidationRule> {
        if self.out_of_gas {
            Some(ViolatedValidationRule::TookTooManyComputationalGas(0))
        } else {
            None
        }
    }
}
