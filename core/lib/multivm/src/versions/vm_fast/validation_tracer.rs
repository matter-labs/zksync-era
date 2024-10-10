use zksync_vm2::interface::{
    self, CallframeInterface, Opcode::*, OpcodeType, ReturnType::*, Tracer,
};

use super::vm::TracerExt;

pub struct ValidationTracer {
    probably_out_of_gas: bool,
    in_validation: bool,
}

impl Default for ValidationTracer {
    fn default() -> Self {
        Self {
            probably_out_of_gas: false,
            in_validation: false,
        }
    }
}

impl Tracer for ValidationTracer {
    fn before_instruction<OP: OpcodeType, S: interface::StateInterface>(&mut self, state: &mut S) {
        if !self.in_validation {
            return;
        }
        match OP::VALUE {
            Ret(Panic) if state.current_frame().gas() == 0 => self.probably_out_of_gas = true,
            _ => {}
        }
    }
}

impl TracerExt for ValidationTracer {
    fn enter_validation(&mut self) {
        self.in_validation = true;
    }

    fn exit_validation(&mut self) {
        self.in_validation = false;
    }
}

impl ValidationTracer {
    pub fn probably_out_of_gas(&self) -> bool {
        self.probably_out_of_gas
    }
}
