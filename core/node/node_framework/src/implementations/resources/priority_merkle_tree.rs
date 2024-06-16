use zksync_mini_merkle_tree::SyncMerkleTree;
use zksync_types::l1::L1Tx;

use crate::resource::Resource;

/// Wrapper for the l1 tx params provider.
#[derive(Debug, Clone)]
pub struct PriorityMerkleTreeResource(pub SyncMerkleTree<L1Tx>);

impl Resource for PriorityMerkleTreeResource {
    fn name() -> String {
        "priority_merkle_tree".into()
    }
}
