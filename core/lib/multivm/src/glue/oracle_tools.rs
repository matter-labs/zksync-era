use crate::VmVersion;

use zksync_state::{ReadStorage, StorageView};

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum OracleTools<'a> {
    M5(vm_m5::OracleTools<'a, false>),
    M6(vm_m6::OracleTools<'a, false, vm_m6::HistoryEnabled>),
    Vm1_3_2(vm_vm1_3_2::OracleTools<'a, false, vm_vm1_3_2::HistoryEnabled>),
}

impl<'a> OracleTools<'a> {
    pub fn new<S>(version: VmVersion, state: &'a mut StorageView<S>) -> Self
    where
        S: ReadStorage + std::fmt::Debug + Send + Sync,
    {
        match version {
            VmVersion::M5WithoutRefunds => {
                let oracle_tools = vm_m5::OracleTools::new(
                    state as &mut dyn vm_m5::storage::Storage,
                    vm_m5::vm::MultiVMSubversion::V1,
                );
                OracleTools::M5(oracle_tools)
            }
            VmVersion::M5WithRefunds => {
                let oracle_tools = vm_m5::OracleTools::new(
                    state as &mut dyn vm_m5::storage::Storage,
                    vm_m5::vm::MultiVMSubversion::V2,
                );
                OracleTools::M5(oracle_tools)
            }
            VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
                let oracle_tools = vm_m6::OracleTools::new(
                    state as &mut dyn vm_m6::storage::Storage,
                    vm_m6::HistoryEnabled,
                );
                OracleTools::M6(oracle_tools)
            }
            VmVersion::Vm1_3_2 => {
                let oracle_tools = vm_vm1_3_2::OracleTools::new(state, vm_vm1_3_2::HistoryEnabled);
                OracleTools::Vm1_3_2(oracle_tools)
            }
        }
    }

    pub fn vm1_3_2(
        &mut self,
    ) -> &mut vm_vm1_3_2::OracleTools<'a, false, vm_vm1_3_2::HistoryEnabled> {
        let OracleTools::Vm1_3_2(oracle_tools) = self else {
            panic!("OracleTools::latest() called on non-latest version")
        };
        oracle_tools
    }

    pub fn m6(&mut self) -> &mut vm_m6::OracleTools<'a, false, vm_m6::HistoryEnabled> {
        let OracleTools::M6(oracle_tools) = self else {
            panic!("OracleTools::m6() called on non-m6 version")
        };
        oracle_tools
    }

    pub fn m5(&mut self) -> &mut vm_m5::OracleTools<'a, false> {
        let OracleTools::M5(oracle_tools) = self else {
            panic!("OracleTools::m5() called on non-m5 version")
        };
        oracle_tools
    }
}
