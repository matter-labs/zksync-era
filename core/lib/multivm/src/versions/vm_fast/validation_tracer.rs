use zksync_vm2::interface::{
    self, CallframeInterface, Opcode::*, OpcodeType, ReturnType::*, Tracer,
};
use zksync_vm_interface::tracer::ViolatedValidationRule;

use super::vm::TracerExt;

/// Account abstraction exposes a chain to denial of service attacks because someone who fails to
/// authenticate does not pay for the failed transaction. Otherwise people could empty other's
/// wallets for free!
///
/// If some address repeatedly posts transactions that validate during preliminary checks but fail
/// to validate during the actual execution, that address is considered a spammer. However, when
/// the spam comes from multiple addresses, that doesn't work.
///
/// We want to ensure that a spammer has to pay for every account that fails validation. This is
/// achieved by limiting what the code of a custom account is allowed to do. If we allowed access
/// to things like time, a validation that fails in the sequencer could be crafted for free, so we
/// don't.
///
/// However, we want to give access to storage. A spammer has to pay for changing storage but
/// could change just one storage slot to invalidate transactions from many accounts. To prevent
/// that, we make sure that the storage slots accessed by different accounts are disjoint by only
/// allowing access to storage in the account itself and slots derived from the account's address.
///
/// Our rules are an extension of the rules are outlined in EIP-7562.
///
/// This tracer enforces the rules by checking what the code does at runtime, even though the
/// properties checked are supposed to always hold for a well-written custom account. Proving
/// that a contract adheres to the rules ahead of time would be challenging or even impossible,
/// considering that contracts that the code depends on may get upgraded.
#[derive(Debug, Default)]
pub struct ValidationTracer {
    in_validation: bool,
    validation_error: Option<ViolatedValidationRule>,
    previous_instruction_was_gasleft: bool,
}

impl Tracer for ValidationTracer {
    fn before_instruction<OP: OpcodeType, S: interface::StateInterface>(&mut self, state: &mut S) {
        if !self.in_validation {
            return;
        }
        if self.previous_instruction_was_gasleft {
            match OP::VALUE {
                // The far call wipes the register, so there is no way the gas number can be used for anything else
                FarCall(_) => {}
                _ => self.set_error(ViolatedValidationRule::TouchedDisallowedContext),
            }
            self.previous_instruction_was_gasleft = false;
        }
        match OP::VALUE {
            // Out of gas once means out of gas for the whole validation, as the EIP forbids handling out of gas errors
            Ret(Panic) if state.current_frame().gas() == 0 => {
                self.set_error(ViolatedValidationRule::TookTooManyComputationalGas(0))
            }
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
    fn set_error(&mut self, error: ViolatedValidationRule) {
        if self.validation_error.is_none() {
            self.validation_error = Some(error);
        }
    }

    pub fn validation_error(&self) -> Option<ViolatedValidationRule> {
        self.validation_error.clone()
    }
}
