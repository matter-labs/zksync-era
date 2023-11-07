use crate::glue::tracer::{IntoVmRefundsEnhancementTracer, IntoVmVirtualBlocksTracer};
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

impl<S, H> IntoVmRefundsEnhancementTracer<S, H> for StorageInvocations
where
    H: crate::HistoryMode,
    S: WriteStorage,
{
    fn vm_refunds_enhancement(
        &self,
    ) -> Box<dyn crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>>
    {
        Box::new(crate::vm_refunds_enhancement::StorageInvocations::new(
            self.limit,
        ))
    }
}

impl<S, H> IntoVmRefundsEnhancementTracer<S, H> for CallTracer<H::VmBoojumIntegration>
where
    H: crate::HistoryMode + 'static,
    S: WriteStorage,
{
    fn vm_refunds_enhancement(
        &self,
    ) -> Box<dyn crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>>
    {
        Box::new(crate::vm_refunds_enhancement::CallTracer::new(
            self.result.clone(),
            H::VmVirtualBlocksRefundsEnhancement::default(),
        ))
    }
}

impl<S, H> IntoVmRefundsEnhancementTracer<S, H> for ValidationTracer<H::VmBoojumIntegration>
where
    H: crate::HistoryMode + 'static,
    S: WriteStorage,
{
    fn vm_refunds_enhancement(
        &self,
    ) -> Box<dyn crate::vm_refunds_enhancement::VmTracer<S, H::VmVirtualBlocksRefundsEnhancement>>
    {
        let params = self.params();
        Box::new(
            crate::vm_refunds_enhancement::ValidationTracer::new(
                crate::vm_refunds_enhancement::ValidationTracerParams {
                    user_address: params.user_address,
                    paymaster_address: params.paymaster_address,
                    trusted_slots: params.trusted_slots,
                    trusted_addresses: params.trusted_addresses,
                    trusted_address_slots: params.trusted_address_slots,
                    computational_gas_limit: params.computational_gas_limit,
                },
            )
            .0,
        )
    }
}
