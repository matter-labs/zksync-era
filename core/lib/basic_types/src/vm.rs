//! Basic VM types that shared widely enough to not put them in the `multivm` crate.

use serde::{Deserialize, Serialize};

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
    Vm1_5_0SmallBootloaderMemory,
    Vm1_5_0IncreasedBootloaderMemory,
    VmGateway,
    VmEvmEmulator,
    VmEcPrecompiles,
    VmInterop,
    VmFullInterop,
}

impl VmVersion {
    /// Returns the latest supported VM version.
    pub const fn latest() -> VmVersion {
        Self::VmInterop
    }
}

/// Mode in which to run the new fast VM implementation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FastVmMode {
    /// Run only the old VM.
    #[default]
    Old,
    /// Run only the new VM.
    New,
    /// Run both the new and old VM and compare their outputs for each transaction execution.
    /// The VM will panic on divergence.
    Shadow,
}
