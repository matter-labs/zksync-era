use crate::glue::history_mode::HistoryMode;
use zksync_state::{ReadStorage, StoragePtr, StorageView};
use zksync_types::VmVersion;

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum OracleTools<S: ReadStorage, H: HistoryMode> {
    M5(crate::vm_m5::OracleTools<false, StorageView<S>>),
    M6(crate::vm_m6::OracleTools<false, StorageView<S>, H::VmM6Mode>),
}

impl<S, H: HistoryMode> OracleTools<S, H>
where
    S: ReadStorage + std::fmt::Debug,
{
    pub fn new(version: VmVersion, state: StoragePtr<StorageView<S>>, history: H) -> Self {
        match version {
            VmVersion::M5WithoutRefunds => {
                let oracle_tools = crate::vm_m5::OracleTools::new(
                    state,
                    crate::vm_m5::vm_instance::MultiVMSubversion::V1,
                );
                OracleTools::M5(oracle_tools)
            }
            VmVersion::M5WithRefunds => {
                let oracle_tools = crate::vm_m5::OracleTools::new(
                    state,
                    crate::vm_m5::vm_instance::MultiVMSubversion::V2,
                );
                OracleTools::M5(oracle_tools)
            }
            VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
                let oracle_tools = crate::vm_m6::OracleTools::new(state, history.glue_into());
                OracleTools::M6(oracle_tools)
            }
            VmVersion::VmVirtualBlocks
            | VmVersion::VmVirtualBlocksRefundsEnhancement
            | VmVersion::Vm1_3_2 => {
                panic!("oracle tools for after VM1.3.2 do not exist")
            }
        }
    }

    pub fn m6(&mut self) -> &mut crate::vm_m6::OracleTools<false, StorageView<S>, H::VmM6Mode> {
        let OracleTools::M6(oracle_tools) = self else {
            panic!("OracleTools::m6() called on non-m6 version")
        };
        oracle_tools
    }

    pub fn m5(&mut self) -> &mut crate::vm_m5::OracleTools<false, StorageView<S>> {
        let OracleTools::M5(oracle_tools) = self else {
            panic!("OracleTools::m5() called on non-m5 version")
        };
        oracle_tools
    }
}
