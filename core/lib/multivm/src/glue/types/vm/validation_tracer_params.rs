use crate::glue::GlueFrom;

impl GlueFrom<vm_virtual_blocks::oracles::tracer::ValidationTracerParams>
    for vm_m5::oracles::tracer::ValidationTracerParams
{
    fn glue_from(value: vm_virtual_blocks::oracles::tracer::ValidationTracerParams) -> Self {
        Self {
            user_address: value.user_address,
            paymaster_address: value.paymaster_address,
            trusted_slots: value.trusted_slots,
            trusted_addresses: value.trusted_addresses,
            trusted_address_slots: value.trusted_address_slots,
        }
    }
}

impl GlueFrom<vm_virtual_blocks::oracles::tracer::ValidationTracerParams>
    for vm_m6::oracles::tracer::ValidationTracerParams
{
    fn glue_from(value: vm_virtual_blocks::oracles::tracer::ValidationTracerParams) -> Self {
        Self {
            user_address: value.user_address,
            paymaster_address: value.paymaster_address,
            trusted_slots: value.trusted_slots,
            trusted_addresses: value.trusted_addresses,
            trusted_address_slots: value.trusted_address_slots,
            computational_gas_limit: value.computational_gas_limit,
        }
    }
}

impl GlueFrom<vm_virtual_blocks::oracles::tracer::ValidationTracerParams>
    for vm_1_3_2::oracles::tracer::ValidationTracerParams
{
    fn glue_from(value: vm_virtual_blocks::oracles::tracer::ValidationTracerParams) -> Self {
        Self {
            user_address: value.user_address,
            paymaster_address: value.paymaster_address,
            trusted_slots: value.trusted_slots,
            trusted_addresses: value.trusted_addresses,
            trusted_address_slots: value.trusted_address_slots,
            computational_gas_limit: value.computational_gas_limit,
        }
    }
}
