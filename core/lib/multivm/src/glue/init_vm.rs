use super::GlueInto;
use crate::glue::history_mode::HistoryMode;
use crate::vm_instance::VmInstanceVersion;
use crate::VmInstance;
use vm_latest::{L1BatchEnv, SystemEnv};
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::VmVersion;
use zksync_utils::h256_to_u256;

impl<S: ReadStorage, H: HistoryMode> VmInstance<S, H> {
    pub fn new(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
    ) -> Self {
        let protocol_version = system_env.version;
        let vm_version: VmVersion = protocol_version.into();
        Self::new_with_specific_version(l1_batch_env, system_env, storage_view, vm_version)
    }

    pub fn new_with_specific_version(
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        storage_view: StoragePtr<StorageView<S>>,
        vm_version: VmVersion,
    ) -> Self {
        match vm_version {
            VmVersion::M5WithoutRefunds => {
                let oracle_tools =
                    vm_m5::OracleTools::new(storage_view.clone(), vm_m5::vm::MultiVMSubversion::V1);
                let block_properties = vm_m5::zk_evm::block_properties::BlockProperties {
                    default_aa_code_hash: h256_to_u256(
                        system_env.base_system_smart_contracts.default_aa.hash,
                    ),
                    zkporter_is_available: false,
                };
                let inner_vm = vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
                    vm_m5::vm::MultiVMSubversion::V1,
                    oracle_tools,
                    l1_batch_env.glue_into(),
                    block_properties,
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
            VmVersion::M5WithRefunds => {
                let oracle_tools =
                    vm_m5::OracleTools::new(storage_view.clone(), vm_m5::vm::MultiVMSubversion::V2);
                let block_properties = vm_m5::zk_evm::block_properties::BlockProperties {
                    default_aa_code_hash: h256_to_u256(
                        system_env.base_system_smart_contracts.default_aa.hash,
                    ),
                    zkporter_is_available: false,
                };
                let inner_vm = vm_m5::vm_with_bootloader::init_vm_with_gas_limit(
                    vm_m5::vm::MultiVMSubversion::V2,
                    oracle_tools,
                    l1_batch_env.glue_into(),
                    block_properties,
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
            VmVersion::M6Initial => {
                let oracle_tools =
                    vm_m6::OracleTools::new(storage_view.clone(), H::VmM6Mode::default());
                let block_properties = vm_m6::zk_evm::block_properties::BlockProperties {
                    default_aa_code_hash: h256_to_u256(
                        system_env.base_system_smart_contracts.default_aa.hash,
                    ),
                    zkporter_is_available: false,
                };

                let inner_vm = vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
                    vm_m6::vm::MultiVMSubversion::V1,
                    oracle_tools,
                    l1_batch_env.glue_into(),
                    block_properties,
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
            VmVersion::M6BugWithCompressionFixed => {
                let oracle_tools =
                    vm_m6::OracleTools::new(storage_view.clone(), H::VmM6Mode::default());
                let block_properties = vm_m6::zk_evm::block_properties::BlockProperties {
                    default_aa_code_hash: h256_to_u256(
                        system_env.base_system_smart_contracts.default_aa.hash,
                    ),
                    zkporter_is_available: false,
                };

                let inner_vm = vm_m6::vm_with_bootloader::init_vm_with_gas_limit(
                    vm_m6::vm::MultiVMSubversion::V2,
                    oracle_tools,
                    l1_batch_env.glue_into(),
                    block_properties,
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
            VmVersion::Vm1_3_2 => {
                let oracle_tools = vm_1_3_2::OracleTools::new(storage_view.clone());
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
            VmVersion::VmVirtualBlocks => {
                let vm = vm_virtual_blocks::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone().glue_into(),
                    storage_view.clone(),
                    H::VmVirtualBlocksMode::default(),
                );
                let vm = VmInstanceVersion::VmVirtualBlocks(Box::new(vm));
                Self {
                    vm,
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
            VmVersion::VmVirtualBlocksRefundsEnhancement => {
                let vm = vm_latest::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone(),
                    storage_view.clone(),
                    H::VmVirtualBlocksRefundsEnhancement::default(),
                );
                let vm = VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(Box::new(vm));
                Self {
                    vm,
                    system_env,
                    last_tx_compressed_bytecodes: vec![],
                }
            }
        }
    }
}
