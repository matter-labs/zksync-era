use std::fmt::Display;

use zksync_types::{Address, H256};

use crate::interface::{tracer::ViolatedValidationRule, Halt};

#[derive(Debug, Clone, Eq, PartialEq, Copy)]
#[allow(clippy::enum_variant_names)]
pub(super) enum ValidationTracerMode {
    /// Should be activated when the transaction is being validated by user.
    UserTxValidation,
    /// Should be activated when the transaction is being validated by the paymaster.
    PaymasterTxValidation,
    /// Is a state when there are no restrictions on the execution.
    NoValidation,
}

#[derive(Debug, Clone, Default)]
pub(super) struct NewTrustedValidationItems {
    pub(super) new_allowed_slots: Vec<H256>,
    pub(super) new_trusted_addresses: Vec<Address>,
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    FailedTx(Halt),
    ViolatedRule(ViolatedValidationRule),
}

impl Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FailedTx(revert_reason) => {
                write!(f, "Validation revert: {}", revert_reason)
            }
            Self::ViolatedRule(rule) => {
                write!(f, "Violated validation rules: {}", rule)
            }
        }
    }
}
