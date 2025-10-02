use std::{collections::HashSet, fmt, ops::Range, time};

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
    /// Minimum time between current block.timestamp and the end of the asserted range
    pub min_time_till_end: time::Duration,
}

/// Rules that can be violated when validating a transaction.
#[derive(Debug, Clone, PartialEq)]
pub enum ViolatedValidationRule {
    /// The transaction touched disallowed storage slots during validation.
    TouchedDisallowedStorageSlots(Address, U256),
    /// The transaction called a contract without attached bytecode.
    CalledContractWithNoCode(Address),
    /// The transaction touched disallowed context.
    TouchedDisallowedContext,
    /// The transaction used too much gas during validation.
    TookTooManyComputationalGas(u32),
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
            ViolatedValidationRule::TimestampAssertionCloseToRangeEnd => {
                write!(f, "block.timestamp is too close to the range end")
            }
        }
    }
}

/// Errors returned when validating a transaction.
#[derive(Debug, PartialEq)]
pub enum ValidationError {
    /// VM execution was halted during validation.
    FailedTx(Halt),
    /// Transaction violated one of account validation rules.
    ViolatedRule(ViolatedValidationRule),
}

/// Traces the validation of a transaction, providing visibility into the aspects the transaction interacts with.
///
/// For instance, the `timestamp_asserter_range` represent the range within which the transaction might make
/// assertions on `block.timestamp`. This information is crucial for the caller, as expired transactions should
/// be excluded from the mempool.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValidationTraces {
    pub timestamp_asserter_range: Option<Range<u64>>,
}

impl ValidationTraces {
    /// Merges two ranges by selecting the maximum of the start values and the minimum of the end values,
    /// producing the narrowest possible time window. Note that overlapping ranges are essential;
    /// a lack of overlap would have triggered an assertion failure in the `TimestampAsserter` contract,
    /// as `block.timestamp` cannot satisfy two non-overlapping ranges.
    pub fn apply_timestamp_asserter_range(&mut self, new_range: Range<u64>) {
        if let Some(range) = &mut self.timestamp_asserter_range {
            range.start = range.start.max(new_range.start);
            range.end = range.end.min(new_range.end);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_range_when_none() {
        let mut validation_traces = ValidationTraces {
            timestamp_asserter_range: None,
        };
        let new_range = 10..20;
        validation_traces.apply_timestamp_asserter_range(new_range.clone());
        assert_eq!(validation_traces.timestamp_asserter_range, Some(new_range));
    }

    #[test]
    fn test_apply_range_with_overlap_narrower_result() {
        let mut validation_traces = ValidationTraces {
            timestamp_asserter_range: Some(5..25),
        };
        validation_traces.apply_timestamp_asserter_range(10..20);
        assert_eq!(validation_traces.timestamp_asserter_range, Some(10..20));
    }

    #[test]
    fn test_apply_range_with_partial_overlap() {
        let mut validation_traces = ValidationTraces {
            timestamp_asserter_range: Some(10..30),
        };
        validation_traces.apply_timestamp_asserter_range(20..40);
        assert_eq!(validation_traces.timestamp_asserter_range, Some(20..30));
    }
}
