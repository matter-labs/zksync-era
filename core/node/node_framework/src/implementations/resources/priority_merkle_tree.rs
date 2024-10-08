use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_types::l1::L1Tx;

use crate::resource::Resource;

/// Wrapper for the `PriorityMerkleTree` provider.
#[derive(Debug, Clone)]
pub struct PriorityTreeResource(pub SyncMerkleTree<L1Tx>);

impl Resource for PriorityTreeResource {
    fn name() -> String {
        "priority_merkle_tree".into()
    }
}
