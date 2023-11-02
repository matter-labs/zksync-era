use crate::glue::tracer::IntoVmVirtualBlocksTracer;
use crate::vm_latest::{CallTracer, StorageInvocations, ValidationTracer};
use zksync_state::WriteStorage;

impl<S, H> IntoVmVirtualBlocksTracer<S, H> for StorageInvocations
where
    H: crate::HistoryMode,
    S: WriteStorage,
{
    fn vm_virtual_blocks(
        &self,
    ) -> Box<dyn crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode>> {
        Box::new(crate::vm_virtual_blocks::StorageInvocations::new(
            self.limit,
        ))
    }
}

impl<S, H> IntoVmVirtualBlocksTracer<S, H> for CallTracer<H::VmBoojumIntegration>
where
    H: crate::HistoryMode + 'static,
    S: WriteStorage,
{
    fn vm_virtual_blocks(
        &self,
    ) -> Box<dyn crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode>> {
        Box::new(crate::vm_virtual_blocks::CallTracer::new(
            self.result.clone(),
            H::VmVirtualBlocksMode::default(),
        ))
    }
}

impl<S, H> IntoVmVirtualBlocksTracer<S, H> for ValidationTracer<H::VmBoojumIntegration>
where
    H: crate::HistoryMode + 'static,
    S: WriteStorage,
{
    fn vm_virtual_blocks(
        &self,
    ) -> Box<dyn crate::vm_virtual_blocks::VmTracer<S, H::VmVirtualBlocksMode>> {
        let params = self.params();
        Box::new(crate::vm_virtual_blocks::ValidationTracer::new(
            crate::vm_virtual_blocks::ValidationTracerParams {
                user_address: params.user_address,
                paymaster_address: params.paymaster_address,
                trusted_slots: params.trusted_slots,
                trusted_addresses: params.trusted_addresses,
                trusted_address_slots: params.trusted_address_slots,
                computational_gas_limit: params.computational_gas_limit,
            },
            self.result.clone(),
        ))
    }
}
