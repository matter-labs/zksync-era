use zksync_types::U256;

use crate::VmVersion;

/// Wrapper for block properties.
/// We have to have it as a public type, since VM requires it to be passed via reference with the same lifetime as other
/// parameters.
///
/// Note that unlike other wrappers, this one depends not on versions of VMs, but on the version of
/// `zk_evm` crate. Prior to `VM1_3_2`, a single version of `zk_evm` crate was used, so here we reduce the amount of
/// boilerplate leaving the necessary wrappers only.
pub enum BlockProperties {
    // M5 & M6 are covered by this variant.
    Vm1_3_1(vm_m6::zk_evm::block_properties::BlockProperties),
    Vm1_3_2(vm_vm1_3_2::zk_evm::block_properties::BlockProperties),
}

impl BlockProperties {
    pub fn new(vm_version: VmVersion, default_aa_code_hash: U256) -> Self {
        match vm_version {
            VmVersion::M5WithoutRefunds
            | VmVersion::M5WithRefunds
            | VmVersion::M6Initial
            | VmVersion::M6BugWithCompressionFixed => {
                let inner = vm_m6::zk_evm::block_properties::BlockProperties {
                    zkporter_is_available: false,
                    default_aa_code_hash,
                };
                Self::Vm1_3_1(inner)
            }
            VmVersion::Vm1_3_2 => {
                let inner = vm_vm1_3_2::zk_evm::block_properties::BlockProperties {
                    zkporter_is_available: false,
                    default_aa_code_hash,
                };
                Self::Vm1_3_2(inner)
            }
        }
    }

    pub fn m5(&self) -> &vm_m5::zk_evm::block_properties::BlockProperties {
        // This is not a typo, M5 is covered by this variant. See doc-comment for the enum.
        if let BlockProperties::Vm1_3_1(inner) = self {
            inner
        } else {
            panic!("BlockProperties::m5 called on non-M5 variant");
        }
    }

    pub fn m6(&self) -> &vm_m6::zk_evm::block_properties::BlockProperties {
        // This is not a typo, M6 is covered by this variant. See doc-comment for the enum.
        if let BlockProperties::Vm1_3_1(inner) = self {
            inner
        } else {
            panic!("BlockProperties::m5 called on non-M5 variant");
        }
    }

    pub fn vm1_3_2(&self) -> &vm_vm1_3_2::zk_evm::block_properties::BlockProperties {
        if let BlockProperties::Vm1_3_2(inner) = self {
            inner
        } else {
            panic!("BlockProperties::m5 called on non-M5 variant");
        }
    }
}
