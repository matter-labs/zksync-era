use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_latest::L1BatchEnv> for crate::vm_virtual_blocks::L1BatchEnv {
    fn glue_from(value: crate::vm_latest::L1BatchEnv) -> Self {
        Self {
            previous_batch_hash: value.previous_batch_hash,
            number: value.number,
            timestamp: value.timestamp,
            l1_gas_price: value.l1_gas_price,
            fair_l2_gas_price: value.fair_l2_gas_price,
            fee_account: value.fee_account,
            enforced_base_fee: value.enforced_base_fee,
            first_l2_block: value.first_l2_block.glue_into(),
        }
    }
}
