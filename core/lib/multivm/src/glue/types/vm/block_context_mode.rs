use zksync_types::VmVersion;
use zksync_utils::h256_to_u256;

use crate::glue::GlueFrom;

impl GlueFrom<crate::interface::L1BatchEnv> for crate::vm_m5::vm_with_bootloader::BlockContextMode {
    fn glue_from(value: crate::interface::L1BatchEnv) -> Self {
        let fee_input = value.fee_input.into_l1_pegged();
        let derived = crate::vm_m5::vm_with_bootloader::DerivedBlockContext {
            context: crate::vm_m5::vm_with_bootloader::BlockContext {
                block_number: value.number.0,
                block_timestamp: value.timestamp,
                operator_address: value.fee_account,
                l1_gas_price: fee_input.l1_gas_price,
                fair_l2_gas_price: fee_input.fair_l2_gas_price,
            },
            // There are two VmVersion's that would work here: M5WithoutRefunds and M5WithRefunds.
            // Both work identically with regard to base fee, so eithr of them can be provided here.
            base_fee: value.base_fee(VmVersion::M5WithRefunds),
        };
        match value.previous_batch_hash {
            Some(hash) => Self::NewBlock(derived, h256_to_u256(hash)),
            None => Self::OverrideCurrent(derived),
        }
    }
}

impl GlueFrom<crate::interface::L1BatchEnv> for crate::vm_m6::vm_with_bootloader::BlockContextMode {
    fn glue_from(value: crate::interface::L1BatchEnv) -> Self {
        let fee_input = value.fee_input.into_l1_pegged();
        let derived = crate::vm_m6::vm_with_bootloader::DerivedBlockContext {
            context: crate::vm_m6::vm_with_bootloader::BlockContext {
                block_number: value.number.0,
                block_timestamp: value.timestamp,
                operator_address: value.fee_account,
                l1_gas_price: fee_input.l1_gas_price,
                fair_l2_gas_price: fee_input.fair_l2_gas_price,
            },
            // There are two VmVersion's that would work here: M6Initial and M6BugWithCompressionFixed.
            // Both work identically with regard to base fee, so eithr of them can be provided here.
            base_fee: value.base_fee(VmVersion::M6Initial),
        };
        match value.previous_batch_hash {
            Some(hash) => Self::NewBlock(derived, h256_to_u256(hash)),
            None => Self::OverrideCurrent(derived),
        }
    }
}

impl GlueFrom<crate::interface::L1BatchEnv>
    for crate::vm_1_3_2::vm_with_bootloader::BlockContextMode
{
    fn glue_from(value: crate::interface::L1BatchEnv) -> Self {
        let fee_input = value.fee_input.into_l1_pegged();
        let derived = crate::vm_1_3_2::vm_with_bootloader::DerivedBlockContext {
            context: crate::vm_1_3_2::vm_with_bootloader::BlockContext {
                block_number: value.number.0,
                block_timestamp: value.timestamp,
                operator_address: value.fee_account,
                l1_gas_price: fee_input.l1_gas_price,
                fair_l2_gas_price: fee_input.fair_l2_gas_price,
            },
            base_fee: value.base_fee(VmVersion::Vm1_3_2),
        };
        match value.previous_batch_hash {
            Some(hash) => Self::NewBlock(derived, h256_to_u256(hash)),
            None => Self::OverrideCurrent(derived),
        }
    }
}
