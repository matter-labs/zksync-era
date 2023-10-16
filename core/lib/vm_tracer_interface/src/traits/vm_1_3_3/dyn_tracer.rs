use zk_evm_1_3_3::tracing::{
    AfterDecodingData, AfterExecutionData, BeforeExecutionData, VmLocalStateData,
};
use zksync_state::{StoragePtr, WriteStorage};

/// Version of zk_evm::Tracer suitable for dynamic dispatch.
pub trait DynTracer<S: WriteStorage> {
    type Memory: zk_evm_1_3_3::zk_evm_abstractions::vm::Memory;
    fn before_decoding(&mut self, _state: VmLocalStateData<'_>, _memory: Self::Memory) {}
    fn after_decoding(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterDecodingData,
        _memory: &Self::Memory,
    ) {
    }
    fn before_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: BeforeExecutionData,
        _memory: &Self::Memory,
        _storage: StoragePtr<S>,
    ) {
    }
    fn after_execution(
        &mut self,
        _state: VmLocalStateData<'_>,
        _data: AfterExecutionData,
        _memory: &Self::Memory,
        _storage: StoragePtr<S>,
    ) {
    }
}
