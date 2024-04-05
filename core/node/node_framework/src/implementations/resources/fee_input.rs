use std::sync::Arc;

use zksync_core::fee_model::BatchFeeModelInputProvider;

use crate::resource::Resource;

/// Wrapper for the batch fee model input provider.
#[derive(Debug, Clone)]
pub struct FeeInputResource(pub Arc<dyn BatchFeeModelInputProvider>);

impl Resource for FeeInputResource {
    fn name() -> String {
        "common/fee_input".into()
    }
}
