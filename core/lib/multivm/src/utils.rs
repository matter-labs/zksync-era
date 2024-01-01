use zksync_system_constants::{MAX_GAS_PER_PUBDATA_BYTE_1_4_1, MAX_GAS_PER_PUBDATA_BYTE_PRE_1_4_1};
use zksync_types::{fee_model::BatchFeeModelInput, ProtocolVersionId, VmVersion, U256};

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub fn derive_base_fee_and_gas_per_pubdata(
    base_fee_input: BatchFeeModelInput,
    vm_version: VmVersion,
) -> (u64, u64) {
    match vm_version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                base_fee_input.into_pegged(),
            )
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                base_fee_input.into_pegged(),
            )
        }
        VmVersion::Vm1_3_2 => {
            crate::vm_1_3_2::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                base_fee_input.into_pegged(),
            )
        }
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::utils::fee::derive_base_fee_and_gas_per_pubdata(
                base_fee_input.into_pegged(),
            )
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::utils::fee::derive_base_fee_and_gas_per_pubdata(
                base_fee_input.into_pegged(),
            )
        }
        // FIXME: once the boojum VM is merged into a separate folder, this place needs to be adjusteds
        VmVersion::VmBoojumIntegration => {
            crate::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata(base_fee_input)
        }
    }
}

/// Changes the fee model output so that the expected gas per pubdata is smaller than or the `tx_gas_per_pubdata_limit`.
pub fn adjust_pubdata_price_for_tx(
    batch_fee_input: &mut BatchFeeModelInput,
    tx_gas_per_pubdata_limit: U256,
    vm_version: VmVersion,
) {
    if U256::from(derive_base_fee_and_gas_per_pubdata(*batch_fee_input, vm_version).1)
        <= tx_gas_per_pubdata_limit
    {
        return;
    }

    // The latest VM supports adjusting the pubdata price for all the types of the fee models.
    crate::vm_latest::utils::fee::adjust_pubdata_price_for_tx(
        batch_fee_input,
        tx_gas_per_pubdata_limit,
    )
}

// pub fn max_gas_per_pubdata_byte(
//     vm_version: VmVersion
// ) -> U256 {
//     match vm_version {
//         // FIXME: once the boojum VM is merged into a separate folder, this place needs to be adjusteds
//         VmVersion::VmBoojumIntegration => MAX_GAS_PER_PUBDATA_BYTE_1_4_1,
//         _ => MAX_GAS_PER_PUBDATA_BYTE_PRE_1_4_1
//     }
// }
