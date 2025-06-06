use crate::glue::{GlueFrom, GlueInto};

pub trait HistoryMode:
    Default
    + GlueInto<Self::VmM6Mode>
    + GlueInto<Self::Vm1_3_2Mode>
    + GlueInto<Self::VmVirtualBlocksMode>
    + GlueInto<Self::VmVirtualBlocksRefundsEnhancement>
    + GlueInto<Self::VmBoojumIntegration>
    + GlueInto<Self::Vm1_4_1>
    + GlueInto<Self::Vm1_4_2>
    + GlueInto<Self::Vm1_5_2>
{
    type VmM6Mode: crate::vm_m6::HistoryMode;
    type Vm1_3_2Mode: crate::vm_1_3_2::HistoryMode;
    type VmVirtualBlocksMode: crate::vm_virtual_blocks::HistoryMode;
    type VmVirtualBlocksRefundsEnhancement: crate::vm_refunds_enhancement::HistoryMode;
    type VmBoojumIntegration: crate::vm_boojum_integration::HistoryMode;
    type Vm1_4_1: crate::vm_1_4_1::HistoryMode;
    type Vm1_4_2: crate::vm_1_4_2::HistoryMode;
    type Vm1_5_2: crate::vm_latest::HistoryMode;
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_m6::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_1_3_2::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_virtual_blocks::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_refunds_enhancement::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_boojum_integration::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_1_4_1::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryEnabled> for crate::vm_1_4_2::HistoryEnabled {
    fn glue_from(_: crate::vm_latest::HistoryEnabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_m6::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_1_3_2::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_virtual_blocks::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled>
    for crate::vm_refunds_enhancement::HistoryDisabled
{
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_boojum_integration::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_1_4_1::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl GlueFrom<crate::vm_latest::HistoryDisabled> for crate::vm_1_4_2::HistoryDisabled {
    fn glue_from(_: crate::vm_latest::HistoryDisabled) -> Self {
        Self
    }
}

impl HistoryMode for crate::vm_latest::HistoryEnabled {
    type VmM6Mode = crate::vm_m6::HistoryEnabled;
    type Vm1_3_2Mode = crate::vm_1_3_2::HistoryEnabled;
    type VmVirtualBlocksMode = crate::vm_virtual_blocks::HistoryEnabled;
    type VmVirtualBlocksRefundsEnhancement = crate::vm_refunds_enhancement::HistoryEnabled;
    type VmBoojumIntegration = crate::vm_boojum_integration::HistoryEnabled;
    type Vm1_4_1 = crate::vm_1_4_1::HistoryEnabled;
    type Vm1_4_2 = crate::vm_1_4_2::HistoryEnabled;
    type Vm1_5_2 = crate::vm_latest::HistoryEnabled;
}

impl HistoryMode for crate::vm_latest::HistoryDisabled {
    type VmM6Mode = crate::vm_m6::HistoryDisabled;
    type Vm1_3_2Mode = crate::vm_1_3_2::HistoryDisabled;
    type VmVirtualBlocksMode = crate::vm_virtual_blocks::HistoryDisabled;
    type VmVirtualBlocksRefundsEnhancement = crate::vm_refunds_enhancement::HistoryDisabled;
    type VmBoojumIntegration = crate::vm_boojum_integration::HistoryDisabled;
    type Vm1_4_1 = crate::vm_1_4_1::HistoryDisabled;
    type Vm1_4_2 = crate::vm_1_4_2::HistoryDisabled;
    type Vm1_5_2 = crate::vm_latest::HistoryDisabled;
}
