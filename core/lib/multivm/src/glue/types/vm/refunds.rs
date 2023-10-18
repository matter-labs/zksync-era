use crate::glue::GlueFrom;
use crate::vm_latest::Refunds;

impl GlueFrom<crate::vm_virtual_blocks::Refunds> for Refunds {
    fn glue_from(value: crate::vm_virtual_blocks::Refunds) -> Self {
        Self {
            gas_refunded: value.gas_refunded,
            operator_suggested_refund: value.operator_suggested_refund,
        }
    }
}
