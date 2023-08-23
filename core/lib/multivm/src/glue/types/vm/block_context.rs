use crate::glue::GlueFrom;

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BlockContext>
    for vm_m5::vm_with_bootloader::BlockContext
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BlockContext) -> Self {
        Self {
            block_number: value.block_number,
            block_timestamp: value.block_timestamp,
            operator_address: value.operator_address,
            l1_gas_price: value.l1_gas_price,
            fair_l2_gas_price: value.fair_l2_gas_price,
        }
    }
}

impl GlueFrom<vm_vm1_3_2::vm_with_bootloader::BlockContext>
    for vm_m6::vm_with_bootloader::BlockContext
{
    fn glue_from(value: vm_vm1_3_2::vm_with_bootloader::BlockContext) -> Self {
        Self {
            block_number: value.block_number,
            block_timestamp: value.block_timestamp,
            operator_address: value.operator_address,
            l1_gas_price: value.l1_gas_price,
            fair_l2_gas_price: value.fair_l2_gas_price,
        }
    }
}
