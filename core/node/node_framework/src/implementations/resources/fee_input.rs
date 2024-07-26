use std::sync::Arc;

use zksync_node_fee_model::BatchFeeModelInputProvider;

use crate::resource::Resource;

/// A resource that provides [`BatchFeeModelInputProvider`] implementation to the service.
#[derive(Debug, Clone)]
pub struct FeeInputResource(pub Arc<dyn BatchFeeModelInputProvider>);

impl Resource for FeeInputResource {
    fn name() -> String {
        "common/fee_input".into()
    }
}

impl<T: BatchFeeModelInputProvider> From<Arc<T>> for FeeInputResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
