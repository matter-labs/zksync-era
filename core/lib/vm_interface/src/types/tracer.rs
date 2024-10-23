use std::{cmp, collections::HashSet, fmt};

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
    /// Parameters of the timestamp asserter if configured
    pub timestamp_asserter_params: Option<TimestampAsserterParams>,
}

#[derive(Debug, Clone)]
pub struct TimestampAsserterParams {
    /// Address of the timestamp asserter. This contract is allowed to touch block.timestamp regardless
    /// of the calling context.
    pub address: Address,
    /// Minimum difference in seconds between the range start and range end
    pub min_range_sec: u32,
    /// Minimum time between current block.timestamp and the end of the asserted range
    pub min_time_till_end_sec: u32,
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
    /// The transaction failed block.timestamp assertion because the range is too short
    TimestampAssertionShortRange,
    /// The transaction failed block.timestamp assertion because the block.timestamp is too close to the range end
    TimestampAssertionCloseToRangeEnd,
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
            ViolatedValidationRule::TimestampAssertionShortRange => {
                write!(f, "block.timestamp range is too short")
            }
            ViolatedValidationRule::TimestampAssertionCloseToRangeEnd => {
                write!(f, "block.timestamp is too close to the range end")
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

/// Traces the validation of a transaction, providing visibility into the aspects the transaction interacts with.
/// For instance, the `timestamp_asserter_range` represent the range within which the transaction might make
/// assertions on `block.timestamp`. This information is crucial for the caller, as expired transactions should
/// be excluded from the mempool.
#[derive(Debug, Clone, Default)]
pub struct ValidationTraces {
    /// Represents a range from-to. Each field is a number of seconds since the epoch.
    pub timestamp_asserter_range: Option<(i64, i64)>,
}

impl ValidationTraces {
    /// Merges two ranges together by taking the maximum of the starts and the minimum of the ends
    /// resulting into the narrowest possible time window
    pub fn apply_range(&mut self, new_range: (i64, i64)) {
        if let Some(mut range) = self.timestamp_asserter_range {
            range.0 = cmp::max(range.0, new_range.0);
            range.1 = cmp::min(range.1, new_range.1);
        } else {
            self.timestamp_asserter_range = Some(new_range);
        }
    }
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
