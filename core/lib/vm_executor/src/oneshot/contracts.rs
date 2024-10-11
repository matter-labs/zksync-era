use zksync_contracts::BaseSystemContracts;
use zksync_types::ProtocolVersionId;

/// System contracts (bootloader and default account abstraction) for all supported VM versions.
#[derive(Debug, Clone)]
pub(super) struct MultiVMBaseSystemContracts {
    /// Contracts to be used for pre-virtual-blocks protocol versions.
    pre_virtual_blocks: BaseSystemContracts,
    /// Contracts to be used for post-virtual-blocks protocol versions.
    post_virtual_blocks: BaseSystemContracts,
    /// Contracts to be used for protocol versions after virtual block upgrade fix.
    post_virtual_blocks_finish_upgrade_fix: BaseSystemContracts,
    /// Contracts to be used for post-boojum protocol versions.
    post_boojum: BaseSystemContracts,
    /// Contracts to be used after the allow-list removal upgrade
    post_allowlist_removal: BaseSystemContracts,
    /// Contracts to be used after the 1.4.1 upgrade
    post_1_4_1: BaseSystemContracts,
    /// Contracts to be used after the 1.4.2 upgrade
    post_1_4_2: BaseSystemContracts,
    /// Contracts to be used during the `v23` upgrade. This upgrade was done on an internal staging environment only.
    vm_1_5_0_small_memory: BaseSystemContracts,
    /// Contracts to be used after the 1.5.0 upgrade
    vm_1_5_0_increased_memory: BaseSystemContracts,
}

impl MultiVMBaseSystemContracts {
    /// Gets contracts for a certain version.
    pub fn get_by_protocol_version(
        &self,
        version: ProtocolVersionId,
        use_evm_emulator: bool,
    ) -> BaseSystemContracts {
        let base = match version {
            ProtocolVersionId::Version0
            | ProtocolVersionId::Version1
            | ProtocolVersionId::Version2
            | ProtocolVersionId::Version3
            | ProtocolVersionId::Version4
            | ProtocolVersionId::Version5
            | ProtocolVersionId::Version6
            | ProtocolVersionId::Version7
            | ProtocolVersionId::Version8
            | ProtocolVersionId::Version9
            | ProtocolVersionId::Version10
            | ProtocolVersionId::Version11
            | ProtocolVersionId::Version12 => &self.pre_virtual_blocks,
            ProtocolVersionId::Version13 => &self.post_virtual_blocks,
            ProtocolVersionId::Version14
            | ProtocolVersionId::Version15
            | ProtocolVersionId::Version16
            | ProtocolVersionId::Version17 => &self.post_virtual_blocks_finish_upgrade_fix,
            ProtocolVersionId::Version18 => &self.post_boojum,
            ProtocolVersionId::Version19 => &self.post_allowlist_removal,
            ProtocolVersionId::Version20 => &self.post_1_4_1,
            ProtocolVersionId::Version21 | ProtocolVersionId::Version22 => &self.post_1_4_2,
            ProtocolVersionId::Version23 => &self.vm_1_5_0_small_memory,
            ProtocolVersionId::Version24 | ProtocolVersionId::Version25 => {
                &self.vm_1_5_0_increased_memory
            }
        };
        let base = base.clone();

        if version.is_post_1_5_0() && use_evm_emulator {
            // EVM emulator is not versioned now; the latest version is always checked out
            base.with_latest_evm_emulator()
        } else {
            base
        }
    }

    pub(super) fn load_estimate_gas_blocking() -> Self {
        Self {
            pre_virtual_blocks: BaseSystemContracts::estimate_gas_pre_virtual_blocks(),
            post_virtual_blocks: BaseSystemContracts::estimate_gas_post_virtual_blocks(),
            post_virtual_blocks_finish_upgrade_fix:
                BaseSystemContracts::estimate_gas_post_virtual_blocks_finish_upgrade_fix(),
            post_boojum: BaseSystemContracts::estimate_gas_post_boojum(),
            post_allowlist_removal: BaseSystemContracts::estimate_gas_post_allowlist_removal(),
            post_1_4_1: BaseSystemContracts::estimate_gas_post_1_4_1(),
            post_1_4_2: BaseSystemContracts::estimate_gas_post_1_4_2(),
            vm_1_5_0_small_memory: BaseSystemContracts::estimate_gas_1_5_0_small_memory(),
            vm_1_5_0_increased_memory:
                BaseSystemContracts::estimate_gas_post_1_5_0_increased_memory(),
        }
    }

    pub(super) fn load_eth_call_blocking() -> Self {
        Self {
            pre_virtual_blocks: BaseSystemContracts::playground_pre_virtual_blocks(),
            post_virtual_blocks: BaseSystemContracts::playground_post_virtual_blocks(),
            post_virtual_blocks_finish_upgrade_fix:
                BaseSystemContracts::playground_post_virtual_blocks_finish_upgrade_fix(),
            post_boojum: BaseSystemContracts::playground_post_boojum(),
            post_allowlist_removal: BaseSystemContracts::playground_post_allowlist_removal(),
            post_1_4_1: BaseSystemContracts::playground_post_1_4_1(),
            post_1_4_2: BaseSystemContracts::playground_post_1_4_2(),
            vm_1_5_0_small_memory: BaseSystemContracts::playground_1_5_0_small_memory(),
            vm_1_5_0_increased_memory: BaseSystemContracts::playground_post_1_5_0_increased_memory(
            ),
        }
    }
}
