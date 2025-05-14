use zksync_node_framework::resource::{Resource, Unique};

use crate::BlockReverter;

/// A resource that provides [`BlockReverter`] to the service.
#[derive(Debug, Clone)]
pub struct BlockReverterResource(pub Unique<BlockReverter>);

impl Resource for BlockReverterResource {
    fn name() -> String {
        "common/block_reverter".into()
    }
}

impl From<BlockReverter> for BlockReverterResource {
    fn from(reverter: BlockReverter) -> Self {
        Self(Unique::new(reverter))
    }
}
