use zksync_block_reverter::BlockReverter;

use crate::resource::{Resource, Unique};

/// A resource that provides [`BlockReverter`] to the service.
#[derive(Debug, Clone)]
pub struct BlockReverterResource(pub Unique<BlockReverter>);

impl Resource for BlockReverterResource {
    fn name() -> String {
        "common/block_reverter".into()
    }
}
