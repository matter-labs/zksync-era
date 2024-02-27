use zksync_types::{
    fee_model::{BatchFeeInput, L1PeggedBatchFeeModelInput, PubdataIndependentBatchFeeModelInput},
    VmVersion, U256,
};

use crate::vm_latest::L1BatchEnv;

/// Calculates the base fee and gas per pubdata for the given L1 gas price.
pub fn derive_base_fee_and_gas_per_pubdata(
    batch_fee_input: BatchFeeInput,
    vm_version: VmVersion,
) -> (u64, u64) {
    match vm_version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::Vm1_3_2 => {
            crate::vm_1_3_2::vm_with_bootloader::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::utils::fee::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::utils::fee::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::utils::fee::derive_base_fee_and_gas_per_pubdata(
                batch_fee_input.into_l1_pegged(),
            )
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::utils::fee::derive_base_fee_and_gas_per_pubdata(
            batch_fee_input.into_pubdata_independent(),
        ),
        VmVersion::Vm1_4_2 => crate::vm_latest::utils::fee::derive_base_fee_and_gas_per_pubdata(
            batch_fee_input.into_pubdata_independent(),
        ),
    }
}

pub fn get_batch_base_fee(l1_batch_env: &L1BatchEnv, vm_version: VmVersion) -> u64 {
    match vm_version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::get_batch_base_fee(l1_batch_env)
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::get_batch_base_fee(l1_batch_env)
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::get_batch_base_fee(l1_batch_env),
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::utils::fee::get_batch_base_fee(l1_batch_env)
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::utils::fee::get_batch_base_fee(l1_batch_env)
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::utils::fee::get_batch_base_fee(l1_batch_env)
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::utils::fee::get_batch_base_fee(l1_batch_env),
        VmVersion::Vm1_4_2 => crate::vm_latest::utils::fee::get_batch_base_fee(l1_batch_env),
    }
}

/// Changes the batch fee input so that the expected gas per pubdata is smaller than or the `tx_gas_per_pubdata_limit`.
pub fn adjust_pubdata_price_for_tx(
    batch_fee_input: BatchFeeInput,
    tx_gas_per_pubdata_limit: U256,
    vm_version: VmVersion,
) -> BatchFeeInput {
    if U256::from(derive_base_fee_and_gas_per_pubdata(batch_fee_input, vm_version).1)
        <= tx_gas_per_pubdata_limit
    {
        // gas per pubdata is already smaller than or equal to `tx_gas_per_pubdata_limit`.
        return batch_fee_input;
    }

    match batch_fee_input {
        BatchFeeInput::L1Pegged(fee_input) => {
            // `gasPerPubdata = ceil(17 * l1gasprice / fair_l2_gas_price)`
            // `gasPerPubdata <= 17 * l1gasprice / fair_l2_gas_price + 1`
            // `fair_l2_gas_price(gasPerPubdata - 1) / 17 <= l1gasprice`
            let new_l1_gas_price = U256::from(fee_input.fair_l2_gas_price)
                * (tx_gas_per_pubdata_limit - U256::from(1u32))
                / U256::from(17);

            BatchFeeInput::L1Pegged(L1PeggedBatchFeeModelInput {
                l1_gas_price: new_l1_gas_price.as_u64(),
                ..fee_input
            })
        }
        BatchFeeInput::PubdataIndependent(fee_input) => {
            // `gasPerPubdata = ceil(fair_pubdata_price / fair_l2_gas_price)`
            // `gasPerPubdata <= fair_pubdata_price / fair_l2_gas_price + 1`
            // `fair_l2_gas_price(gasPerPubdata - 1) <= fair_pubdata_price`
            let new_fair_pubdata_price = U256::from(fee_input.fair_l2_gas_price)
                * (tx_gas_per_pubdata_limit - U256::from(1u32));

            BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
                fair_pubdata_price: new_fair_pubdata_price.as_u64(),
                ..fee_input
            })
        }
    }
}

pub fn derive_overhead(
    gas_limit: u32,
    gas_price_per_pubdata: u32,
    encoded_len: usize,
    tx_type: u8,
    vm_version: VmVersion,
) -> u32 {
    match vm_version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::transaction_data::derive_overhead(
                gas_limit,
                gas_price_per_pubdata,
                encoded_len,
            )
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::transaction_data::derive_overhead(
                gas_limit,
                gas_price_per_pubdata,
                encoded_len,
                crate::vm_m6::transaction_data::OverheadCoefficients::from_tx_type(tx_type),
            )
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::transaction_data::derive_overhead(
            gas_limit,
            gas_price_per_pubdata,
            encoded_len,
            crate::vm_1_3_2::transaction_data::OverheadCoefficients::from_tx_type(tx_type),
        ),
        VmVersion::VmVirtualBlocks => crate::vm_virtual_blocks::utils::overhead::derive_overhead(
            gas_limit,
            gas_price_per_pubdata,
            encoded_len,
            crate::vm_virtual_blocks::utils::overhead::OverheadCoefficients::from_tx_type(tx_type),
        ),
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::utils::overhead::derive_overhead(
                gas_limit,
                gas_price_per_pubdata,
                encoded_len,
                crate::vm_refunds_enhancement::utils::overhead::OverheadCoefficients::from_tx_type(
                    tx_type,
                ),
            )
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::utils::overhead::derive_overhead(
                gas_limit,
                gas_price_per_pubdata,
                encoded_len,
                crate::vm_boojum_integration::utils::overhead::OverheadCoefficients::from_tx_type(
                    tx_type,
                ),
            )
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::utils::overhead::derive_overhead(encoded_len),
        VmVersion::Vm1_4_2 => crate::vm_latest::utils::overhead::derive_overhead(encoded_len),
    }
}

pub fn get_bootloader_encoding_space(version: VmVersion) -> u32 {
    match version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::BOOTLOADER_TX_ENCODING_SPACE
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::BOOTLOADER_TX_ENCODING_SPACE
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::BOOTLOADER_TX_ENCODING_SPACE,
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::constants::BOOTLOADER_TX_ENCODING_SPACE
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::constants::BOOTLOADER_TX_ENCODING_SPACE
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::constants::BOOTLOADER_TX_ENCODING_SPACE
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::constants::BOOTLOADER_TX_ENCODING_SPACE,
        VmVersion::Vm1_4_2 => crate::vm_latest::constants::BOOTLOADER_TX_ENCODING_SPACE,
    }
}

pub fn get_bootloader_max_txs_in_batch(version: VmVersion) -> usize {
    match version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::MAX_TXS_IN_BLOCK
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::MAX_TXS_IN_BLOCK
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::MAX_TXS_IN_BLOCK,
        VmVersion::VmVirtualBlocks => crate::vm_virtual_blocks::constants::MAX_TXS_IN_BLOCK,
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::constants::MAX_TXS_IN_BLOCK
        }
        VmVersion::VmBoojumIntegration => crate::vm_boojum_integration::constants::MAX_TXS_IN_BLOCK,
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::constants::MAX_TXS_IN_BATCH,
        VmVersion::Vm1_4_2 => crate::vm_latest::constants::MAX_TXS_IN_BATCH,
    }
}

pub fn gas_bootloader_batch_tip_overhead(version: VmVersion) -> u32 {
    match version {
        VmVersion::M5WithRefunds
        | VmVersion::M5WithoutRefunds
        | VmVersion::M6Initial
        | VmVersion::M6BugWithCompressionFixed
        | VmVersion::Vm1_3_2
        | VmVersion::VmVirtualBlocks
        | VmVersion::VmVirtualBlocksRefundsEnhancement => {
            // For these versions the overhead has not been calculated and it has not been used with those versions.
            0
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::constants::BOOTLOADER_BATCH_TIP_OVERHEAD
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::constants::BOOTLOADER_BATCH_TIP_OVERHEAD,
        VmVersion::Vm1_4_2 => crate::vm_latest::constants::BOOTLOADER_BATCH_TIP_OVERHEAD,
    }
}

pub fn get_max_gas_per_pubdata_byte(version: VmVersion) -> u64 {
    match version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::MAX_GAS_PER_PUBDATA_BYTE
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::MAX_GAS_PER_PUBDATA_BYTE
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::MAX_GAS_PER_PUBDATA_BYTE,
        VmVersion::VmVirtualBlocks => crate::vm_virtual_blocks::constants::MAX_GAS_PER_PUBDATA_BYTE,
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::constants::MAX_GAS_PER_PUBDATA_BYTE
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::constants::MAX_GAS_PER_PUBDATA_BYTE
        }
        VmVersion::Vm1_4_1 => crate::vm_latest::constants::MAX_GAS_PER_PUBDATA_BYTE,
        VmVersion::Vm1_4_2 => crate::vm_1_4_1::constants::MAX_GAS_PER_PUBDATA_BYTE,
    }
}

pub fn get_used_bootloader_memory_bytes(version: VmVersion) -> usize {
    match version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::USED_BOOTLOADER_MEMORY_BYTES
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::USED_BOOTLOADER_MEMORY_BYTES
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::USED_BOOTLOADER_MEMORY_BYTES,
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::constants::USED_BOOTLOADER_MEMORY_BYTES
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::constants::USED_BOOTLOADER_MEMORY_BYTES
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::constants::USED_BOOTLOADER_MEMORY_BYTES
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::constants::USED_BOOTLOADER_MEMORY_BYTES,
        VmVersion::Vm1_4_2 => crate::vm_latest::constants::USED_BOOTLOADER_MEMORY_BYTES,
    }
}

pub fn get_used_bootloader_memory_words(version: VmVersion) -> usize {
    match version {
        VmVersion::M5WithRefunds | VmVersion::M5WithoutRefunds => {
            crate::vm_m5::vm_with_bootloader::USED_BOOTLOADER_MEMORY_WORDS
        }
        VmVersion::M6Initial | VmVersion::M6BugWithCompressionFixed => {
            crate::vm_m6::vm_with_bootloader::USED_BOOTLOADER_MEMORY_WORDS
        }
        VmVersion::Vm1_3_2 => crate::vm_1_3_2::vm_with_bootloader::USED_BOOTLOADER_MEMORY_WORDS,
        VmVersion::VmVirtualBlocks => {
            crate::vm_virtual_blocks::constants::USED_BOOTLOADER_MEMORY_WORDS
        }
        VmVersion::VmVirtualBlocksRefundsEnhancement => {
            crate::vm_refunds_enhancement::constants::USED_BOOTLOADER_MEMORY_WORDS
        }
        VmVersion::VmBoojumIntegration => {
            crate::vm_boojum_integration::constants::USED_BOOTLOADER_MEMORY_WORDS
        }
        VmVersion::Vm1_4_1 => crate::vm_1_4_1::constants::USED_BOOTLOADER_MEMORY_WORDS,
        VmVersion::Vm1_4_2 => crate::vm_latest::constants::USED_BOOTLOADER_MEMORY_WORDS,
    }
}
