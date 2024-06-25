use std::sync::Arc;

use zksync_block_reverter::BlockReverter;

use crate::resource::Resource;

/// A resource that provides [`BlockReverter`] to the service.
#[derive(Debug, Clone)]
pub struct BlockReverterResource(pub Arc<BlockReverter>);

impl Resource for BlockReverterResource {
    fn name() -> String {
        "common/block_reverter".into()
    }
}
