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
    Local,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        // During development of system contracts, one can replace this value with `Self::Local` to
        // see the system contract changes take effect without the hassle of moving the artifacts
        // into etc/multivm_bootloaders each time.
        // Don't forget to do the same change in ProtocolVersionId::latest()
        Self::Vm1_4_2
    }
}
