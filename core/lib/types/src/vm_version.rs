#[derive(Debug, Clone, Copy)]
pub enum VmVersion {
    M5WithoutRefunds,
    M5WithRefunds,
    M6Initial,
    M6BugWithCompressionFixed,
    Vm1_3_2,
    VmVirtualBlocks,
    VmVirtualBlocksRefundsEnhancement,
    // kl todo delete local vm verion
    Local,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        // Self::VmVirtualBlocksRefundsEnhancement
        // kl todo delete local vm verion
        Self::Local
    }
}
