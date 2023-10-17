use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_m5::oracles::tracer::ValidationError>
    for crate::vm_latest::oracles::tracer::ValidationError
{
    fn glue_from(value: crate::vm_m5::oracles::tracer::ValidationError) -> Self {
        match value {
            crate::vm_m5::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            crate::vm_m5::oracles::tracer::ValidationError::VioalatedRule(value) => {
                let rule = match value {
                     crate::vm_m5::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                     crate::vm_m5::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                     crate::vm_m5::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}

impl GlueFrom<crate::vm_m6::oracles::tracer::ValidationError>
    for crate::vm_latest::oracles::tracer::ValidationError
{
    fn glue_from(value: crate::vm_m6::oracles::tracer::ValidationError) -> Self {
        match value {
            crate::vm_m6::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            crate::vm_m6::oracles::tracer::ValidationError::VioalatedRule(value) => {
                let rule = match value {
                     crate::vm_m6::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                     crate::vm_m6::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                     crate::vm_m6::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                     crate::vm_m6::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a),
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}

impl GlueFrom<crate::vm_1_3_2::oracles::tracer::ValidationError>
    for crate::vm_latest::oracles::tracer::ValidationError
{
    fn glue_from(value: crate::vm_1_3_2::oracles::tracer::ValidationError) -> Self {
        match value {
            crate::vm_1_3_2::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            crate::vm_1_3_2::oracles::tracer::ValidationError::ViolatedRule(value) => {
                let rule = match value {
                    crate::vm_1_3_2::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                    crate::vm_1_3_2::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                    crate::vm_1_3_2::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                    crate::vm_1_3_2::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a) =>
                        crate::vm_latest::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a),
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}
