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
    // kl todo delete local vm verion
    Local,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        // Self::VmBoojumIntegration
        // kl todo delete local vm verion
        Self::Local
    }
}
