use crate::interface::Halt;
use std::collections::HashSet;
use std::fmt::Display;
use zksync_types::vm_trace::ViolatedValidationRule;
use zksync_types::{Address, H256, U256};

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
pub struct ValidationTracerParams {
    pub user_address: Address,
    pub paymaster_address: Address,
    /// Slots that are trusted (i.e. the user can access them).
    pub trusted_slots: HashSet<(Address, U256)>,
    /// Trusted addresses (the user can access any slots on these addresses).
    pub trusted_addresses: HashSet<Address>,
    /// Slots, that are trusted and the value of them is the new trusted address.
    /// They are needed to work correctly with beacon proxy, where the address of the implementation is
    /// stored in the beacon.
    pub trusted_address_slots: HashSet<(Address, U256)>,
    /// Number of computational gas that validation step is allowed to use.
    pub computational_gas_limit: u32,
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
