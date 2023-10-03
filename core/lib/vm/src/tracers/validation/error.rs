use crate::Halt;
use std::fmt::Display;
use zksync_types::{Address, U256};
use zksync_utils::u256_to_h256;

#[derive(Debug, Clone)]
pub enum ViolatedValidationRule {
    TouchedUnallowedStorageSlots(Address, U256),
    CalledContractWithNoCode(Address),
    TouchedUnallowedContext,
    TookTooManyComputationalGas(u32),
}

#[derive(Debug, Clone)]
pub enum ValidationError {
    FailedTx(Halt),
    ViolatedRule(ViolatedValidationRule),
}

impl Display for ViolatedValidationRule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViolatedValidationRule::TouchedUnallowedStorageSlots(contract, key) => write!(
                f,
                "Touched unallowed storage slots: address {}, key: {}",
                hex::encode(contract),
                hex::encode(u256_to_h256(*key))
            ),
            ViolatedValidationRule::CalledContractWithNoCode(contract) => {
                write!(f, "Called contract with no code: {}", hex::encode(contract))
            }
            ViolatedValidationRule::TouchedUnallowedContext => {
                write!(f, "Touched unallowed context")
            }
            ViolatedValidationRule::TookTooManyComputationalGas(gas_limit) => {
                write!(
                    f,
                    "Took too many computational gas, allowed limit: {}",
                    gas_limit
                )
            }
        }
    }
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
