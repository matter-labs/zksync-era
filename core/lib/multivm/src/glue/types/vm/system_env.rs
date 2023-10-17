use crate::glue::{GlueFrom, GlueInto};

impl GlueFrom<crate::vm_latest::SystemEnv> for crate::vm_virtual_blocks::SystemEnv {
    fn glue_from(value: crate::vm_latest::SystemEnv) -> Self {
        Self {
            zk_porter_available: value.zk_porter_available,
            version: value.version,
            base_system_smart_contracts: value.base_system_smart_contracts,
            gas_limit: value.gas_limit,
            execution_mode: value.execution_mode.glue_into(),
            default_validation_computational_gas_limit: value
                .default_validation_computational_gas_limit,
            chain_id: value.chain_id,
        }
    }
}
