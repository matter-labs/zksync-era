use super::GlueInto;
use crate::glue::history_mode::HistoryMode;
use crate::vm_instance::{VmInstanceData, VmInstanceVersion};
use crate::VmInstance;
use vm_latest::{L1BatchEnv, SystemEnv};
use zksync_state::ReadStorage;
use zksync_utils::h256_to_u256;

impl<'a, S: ReadStorage, H: HistoryMode> VmInstance<'a, S, H> {
    pub fn new(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        initial_version: &'a mut VmInstanceData<S, H>,
    ) -> Self {
        match initial_version {
            VmInstanceData::M5(data) => {
                let inner_vm = vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
                    data.sub_version,
                    data.oracle_tools.m5(),
                    l1_batch_env.glue_into(),
                    data.block_properties.m5(),
                    system_env.execution_mode.glue_into(),
                    &system_env.base_system_smart_contracts.clone().glue_into(),
                    system_env.gas_limit,
                );
                VmInstance {
                    vm: VmInstanceVersion::VmM5(inner_vm),
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
            VmInstanceData::M6(data) => {
                let inner_vm = vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
                    data.sub_version,
                    data.oracle_tools.m6(),
                    l1_batch_env.glue_into(),
                    data.block_properties.m6(),
                    system_env.execution_mode.glue_into(),
                    &system_env.base_system_smart_contracts.clone().glue_into(),
                    system_env.gas_limit,
                );
                VmInstance {
                    vm: VmInstanceVersion::VmM6(inner_vm),
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }

            VmInstanceData::Vm1_3_2(data) => {
                let oracle_tools = vm_1_3_2::OracleTools::new(data.storage_view.clone());
                let block_properties = vm_1_3_2::BlockProperties {
                    default_aa_code_hash: h256_to_u256(
                        system_env.base_system_smart_contracts.default_aa.hash,
                    ),
                    zkporter_is_available: false,
                };
                let inner_vm = vm_1_3_2::vm_with_bootloader::init_vm_with_gas_limit(
                    oracle_tools,
                    l1_batch_env.glue_into(),
                    block_properties,
                    system_env.execution_mode.glue_into(),
                    &system_env.base_system_smart_contracts.clone().glue_into(),
                    system_env.gas_limit,
                );
                VmInstance {
                    vm: VmInstanceVersion::Vm1_3_2(inner_vm),
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
            VmInstanceData::VmVirtualBlocks(data) => {
                let vm = vm_virtual_blocks::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone(),
                    data.storage_view.clone(),
                    H::VmVirtualBlocksMode::default(),
                );
                let vm = VmInstanceVersion::VmVirtualBlocks(Box::new(vm));
                Self {
                    vm,
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
            VmInstanceData::VmTimelessHistory(storage_view) => {
                let vm = vm_latest::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone(),
                    storage_view.clone(),
                );
                let vm = VmInstanceVersion::VmTimelessHistory(Box::new(vm));
                Self {
                    vm,
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
        }
    }
}
