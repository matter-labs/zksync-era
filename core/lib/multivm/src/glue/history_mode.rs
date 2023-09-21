use crate::glue::{GlueFrom, GlueInto};

pub trait HistoryMode:
    Default
    + GlueInto<Self::VmM6Mode>
    + GlueInto<Self::Vm1_3_2Mode>
    + GlueInto<Self::VmVirtualBlocksMode>
{
    type VmM6Mode: vm_m6::HistoryMode;
    type Vm1_3_2Mode: vm_1_3_2::HistoryMode;
    type VmVirtualBlocksMode: vm_latest::HistoryMode;
}

impl GlueFrom<vm_latest::HistoryEnabled> for vm_m6::HistoryEnabled {
    fn glue_from(_: vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<vm_latest::HistoryEnabled> for vm_1_3_2::HistoryEnabled {
    fn glue_from(_: vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<vm_latest::HistoryDisabled> for vm_m6::HistoryDisabled {
    fn glue_from(_: vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<vm_latest::HistoryDisabled> for vm_1_3_2::HistoryDisabled {
    fn glue_from(_: vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl HistoryMode for vm_latest::HistoryEnabled {
    type VmM6Mode = vm_m6::HistoryEnabled;
    type Vm1_3_2Mode = vm_1_3_2::HistoryEnabled;
    type VmVirtualBlocksMode = vm_latest::HistoryEnabled;
}

impl HistoryMode for vm_latest::HistoryDisabled {
    type VmM6Mode = vm_m6::HistoryDisabled;
    type Vm1_3_2Mode = vm_1_3_2::HistoryDisabled;
    type VmVirtualBlocksMode = vm_latest::HistoryDisabled;
}
