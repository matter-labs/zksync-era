use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<vm_m5::oracles::tracer::ValidationError>
    for vm_virtual_blocks::oracles::tracer::ValidationError
{
    fn glue_from(value: vm_m5::oracles::tracer::ValidationError) -> Self {
        match value {
            vm_m5::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            vm_m5::oracles::tracer::ValidationError::VioalatedRule(value) => {
                let rule = match value {
                    vm_m5::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                    vm_m5::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                    vm_m5::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}

impl GlueFrom<vm_m6::oracles::tracer::ValidationError>
    for vm_virtual_blocks::oracles::tracer::ValidationError
{
    fn glue_from(value: vm_m6::oracles::tracer::ValidationError) -> Self {
        match value {
            vm_m6::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            vm_m6::oracles::tracer::ValidationError::VioalatedRule(value) => {
                let rule = match value {
                    vm_m6::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                    vm_m6::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                    vm_m6::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                    vm_m6::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a),
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}

impl GlueFrom<vm_1_3_2::oracles::tracer::ValidationError>
    for vm_virtual_blocks::oracles::tracer::ValidationError
{
    fn glue_from(value: vm_1_3_2::oracles::tracer::ValidationError) -> Self {
        match value {
            vm_1_3_2::oracles::tracer::ValidationError::FailedTx(value) => {
                Self::FailedTx(value.glue_into())
            }
            vm_1_3_2::oracles::tracer::ValidationError::ViolatedRule(value) => {
                let rule = match value {
                    vm_1_3_2::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedStorageSlots(a, b),
                    vm_1_3_2::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::CalledContractWithNoCode(a),
                    vm_1_3_2::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TouchedUnallowedContext,
                    vm_1_3_2::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a) =>
                        vm_virtual_blocks::oracles::tracer::ViolatedValidationRule::TookTooManyComputationalGas(a),
                };
                Self::ViolatedRule(rule)
            }
        }
    }
}
