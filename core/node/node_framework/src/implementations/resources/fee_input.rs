use std::sync::Arc;

use zksync_core::fee_model::BatchFeeModelInputProvider;

use crate::resource::{Resource, ResourceId};

/// Wrapper for the batch fee model input provider.
#[derive(Debug, Clone)]
pub struct FeeInputResource(pub Arc<dyn BatchFeeModelInputProvider>);

impl Resource for FeeInputResource {
    fn resource_id() -> ResourceId {
        "common/fee_input".into()
    }
}
