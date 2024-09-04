use std::{collections::HashSet, fmt};

use zksync_types::{Address, U256};

use crate::Halt;

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStopReason {
    Finish,
    Abort(Halt),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TracerExecutionStatus {
    Continue,
    Stop(TracerExecutionStopReason),
}

impl TracerExecutionStatus {
    /// Chose the stricter ExecutionStatus
    /// If both statuses are Continue, then the result is Continue
    /// If one of the statuses is Abort, then the result is Abort
    /// If one of the statuses is Finish, then the result is Finish
    pub fn stricter(&self, other: &Self) -> Self {
        match (self, other) {
            (Self::Continue, Self::Continue) => Self::Continue,
            (Self::Stop(TracerExecutionStopReason::Abort(reason)), _)
            | (_, Self::Stop(TracerExecutionStopReason::Abort(reason))) => {
                Self::Stop(TracerExecutionStopReason::Abort(reason.clone()))
            }
            (Self::Stop(TracerExecutionStopReason::Finish), _)
            | (_, Self::Stop(TracerExecutionStopReason::Finish)) => {
                Self::Stop(TracerExecutionStopReason::Finish)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VmExecutionStopReason {
    VmFinished,
    TracerRequestedStop(TracerExecutionStopReason),
}

/// Transaction validation parameters.
#[derive(Debug, Clone)]
pub struct ValidationParams {
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

/// Rules that can be violated when validating a transaction.
#[derive(Debug, Clone)]
pub enum ViolatedValidationRule {
    /// The transaction touched disallowed storage slots during validation.
    TouchedDisallowedStorageSlots(Address, U256),
    /// The transaction called a contract without attached bytecode.
    CalledContractWithNoCode(Address),
    /// The transaction touched disallowed context.
    TouchedDisallowedContext,
    /// The transaction used too much gas during validation.
    TookTooManyComputationalGas(u32),
}

impl fmt::Display for ViolatedValidationRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ViolatedValidationRule::TouchedDisallowedStorageSlots(contract, key) => write!(
                f,
                "Touched disallowed storage slots: address {contract:x}, key: {key:x}",
            ),
            ViolatedValidationRule::CalledContractWithNoCode(contract) => {
                write!(f, "Called contract with no code: {contract:x}")
            }
            ViolatedValidationRule::TouchedDisallowedContext => {
                write!(f, "Touched disallowed context")
            }
            ViolatedValidationRule::TookTooManyComputationalGas(gas_limit) => {
                write!(
                    f,
                    "Took too many computational gas, allowed limit: {gas_limit}"
                )
            }
        }
    }
}

/// Errors returned when validating a transaction.
#[derive(Debug)]
pub enum ValidationError {
    /// VM execution was halted during validation.
    FailedTx(Halt),
    /// Transaction violated one of account validation rules.
    ViolatedRule(ViolatedValidationRule),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
