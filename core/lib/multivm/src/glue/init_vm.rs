use super::GlueInto;
use crate::VmVersion;

pub fn init_vm<'a>(
    version: VmVersion,
    oracle_tools: &'a mut crate::glue::oracle_tools::OracleTools<'a>,
    block_context: vm_vm1_3_2::vm_with_bootloader::BlockContextMode,
    block_properties: &'a crate::glue::block_properties::BlockProperties,
    execution_mode: vm_vm1_3_2::vm_with_bootloader::TxExecutionMode,
    base_system_contracts: &zksync_contracts::BaseSystemContracts,
) -> crate::VmInstance<'a> {
    let gas_limit = match version {
        VmVersion::M5WithoutRefunds => vm_m5::utils::BLOCK_GAS_LIMIT,
        VmVersion::M5WithRefunds => vm_m5::utils::BLOCK_GAS_LIMIT,
        VmVersion::M6Initial => vm_m6::utils::BLOCK_GAS_LIMIT,
        VmVersion::M6BugWithCompressionFixed => vm_m6::utils::BLOCK_GAS_LIMIT,
        VmVersion::Vm1_3_2 => vm_vm1_3_2::utils::BLOCK_GAS_LIMIT,
    };

    init_vm_with_gas_limit(
        version,
        oracle_tools,
        block_context,
        block_properties,
        execution_mode,
        base_system_contracts,
        gas_limit,
    )
}

pub fn init_vm_with_gas_limit<'a>(
    version: VmVersion,
    oracle_tools: &'a mut crate::glue::oracle_tools::OracleTools<'a>,
    block_context: vm_vm1_3_2::vm_with_bootloader::BlockContextMode,
    block_properties: &'a crate::glue::block_properties::BlockProperties,
    execution_mode: vm_vm1_3_2::vm_with_bootloader::TxExecutionMode,
    base_system_contracts: &zksync_contracts::BaseSystemContracts,
    gas_limit: u32,
) -> crate::VmInstance<'a> {
    match version {
        VmVersion::M5WithoutRefunds => {
            let inner_vm = vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
                vm_m5::vm::MultiVMSubversion::V1,
                oracle_tools.m5(),
                block_context.glue_into(),
                block_properties.m5(),
                execution_mode.glue_into(),
                &base_system_contracts.clone().glue_into(),
                gas_limit,
            );
            crate::VmInstance::VmM5(inner_vm)
        }
        VmVersion::M5WithRefunds => {
            let inner_vm = vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
                vm_m5::vm::MultiVMSubversion::V2,
                oracle_tools.m5(),
                block_context.glue_into(),
                block_properties.m5(),
                execution_mode.glue_into(),
                &base_system_contracts.clone().glue_into(),
                gas_limit,
            );
            crate::VmInstance::VmM5(inner_vm)
        }
        VmVersion::M6Initial => {
            let inner_vm = vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
                vm_m6::vm::MultiVMSubversion::V1,
                oracle_tools.m6(),
                block_context.glue_into(),
                block_properties.m6(),
                execution_mode.glue_into(),
                &base_system_contracts.clone().glue_into(),
                gas_limit,
            );
            crate::VmInstance::VmM6(inner_vm)
        }
        VmVersion::M6BugWithCompressionFixed => {
            let inner_vm = vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
                vm_m6::vm::MultiVMSubversion::V2,
                oracle_tools.m6(),
                block_context.glue_into(),
                block_properties.m6(),
                execution_mode.glue_into(),
                &base_system_contracts.clone().glue_into(),
                gas_limit,
            );
            crate::VmInstance::VmM6(inner_vm)
        }
        VmVersion::Vm1_3_2 => {
            let inner_vm = vm_vm1_3_2::vm_with_bootloader::init_vm_with_gas_limit(
                oracle_tools.vm1_3_2(),
                block_context.glue_into(),
                block_properties.vm1_3_2(),
                execution_mode.glue_into(),
                &base_system_contracts.clone().glue_into(),
                gas_limit,
            );
            crate::VmInstance::Vm1_3_2(inner_vm)
        }
    }
}
