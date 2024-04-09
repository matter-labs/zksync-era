#[derive(Debug, Clone, Copy)]
pub enum VmVersion {
    M5WithoutRefunds,
    M5WithRefunds,
    M6Initial,
    M6BugWithCompressionFixed,
    Vm1_3_2,
    VmVirtualBlocks,
    VmVirtualBlocksRefundsEnhancement,
    VmBoojumIntegration,
    Vm1_4_1,
    Vm1_4_2,
    Vm1_5_0,
    // kl todo delete local vm verion
    Local,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        // kl todo delete local vm verion
        Self::Local
    }
}
