use super::GlueInto;
use crate::glue::history_mode::HistoryMode;
use crate::interface::{L1BatchEnv, SystemEnv, VmInterface};
use crate::vm_instance::VmInstanceVersion;
use crate::VmInstance;
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::VmVersion;

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
                let vm =
                    crate::vm_m5::Vm::new(l1_batch_env, system_env.clone(), storage_view.clone());
                let vm = VmInstanceVersion::VmM5(vm);
                Self { vm }
            }
            VmVersion::M5WithRefunds => {
                let vm =
                    crate::vm_m5::Vm::new(l1_batch_env, system_env.clone(), storage_view.clone());
                let vm = VmInstanceVersion::VmM5(vm);
                Self { vm }
            }
            VmVersion::M6Initial => {
                let vm =
                    crate::vm_m6::Vm::new(l1_batch_env, system_env.clone(), storage_view.clone());
                let vm = VmInstanceVersion::VmM6(vm);
                Self { vm }
            }
            VmVersion::M6BugWithCompressionFixed => {
                let vm =
                    crate::vm_m6::Vm::new(l1_batch_env, system_env.clone(), storage_view.clone());
                let vm = VmInstanceVersion::VmM6(vm);
                Self { vm }
            }
            VmVersion::Vm1_3_2 => {
                let vm = crate::vm_1_3_2::Vm::new(
                    l1_batch_env,
                    system_env.clone(),
                    storage_view.clone(),
                );
                let vm = VmInstanceVersion::Vm1_3_2(vm);
                Self { vm }
            }
            VmVersion::VmVirtualBlocks => {
                let vm = crate::vm_virtual_blocks::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone().glue_into(),
                    storage_view.clone(),
                );
                let vm = VmInstanceVersion::VmVirtualBlocks(vm);
                Self { vm }
            }
            VmVersion::VmVirtualBlocksRefundsEnhancement => {
                let vm = crate::vm_latest::Vm::new(
                    l1_batch_env.glue_into(),
                    system_env.clone(),
                    storage_view.clone(),
                );
                let vm = VmInstanceVersion::VmVirtualBlocksRefundsEnhancement(vm);
                Self { vm }
            }
        }
    }
}
