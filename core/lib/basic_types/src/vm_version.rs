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
    // To keep the name consistent with the previous version
    #[allow(non_camel_case_types)]
    Vm_1_5_0_IncreaedBootloaderMemory,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        Self::Vm_1_5_0_IncreaedBootloaderMemory
    }
}
