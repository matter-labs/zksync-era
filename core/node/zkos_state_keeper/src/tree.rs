use std::alloc::Global;
use std::collections::HashMap;
use zk_os_forward_system::run::{LeafProof, ReadStorage, ReadStorageTree};
use ruint::aliases::B160;
use zk_ee::utils::Bytes32;
use zk_os_basic_system::basic_system::simple_growable_storage::TestingTree;

#[derive(Debug, Clone)]
pub struct InMemoryTree {
    /// Hash map from a pair of Address, slot into values.
    pub cold_storage: HashMap<Bytes32, Bytes32>,
    pub storage_tree: TestingTree
}

impl InMemoryTree {
    pub fn new() -> Self {
        Self {
            cold_storage: HashMap::new(),
            storage_tree: TestingTree::new_in(Global),
        }
    }
}

impl ReadStorage for InMemoryTree {
    fn read(&self, key: Bytes32) -> Option<Bytes32> {
        self.cold_storage.get(&key).cloned()
    }
}

impl ReadStorageTree for InMemoryTree {
    fn tree_index(&self, key: Bytes32) -> Option<u64> {
        Some(self.storage_tree.get_index_for_existing(&key))
    }

    fn merkle_proof(&self, tree_index: u64) -> LeafProof {
        self.storage_tree.get_proof_for_position(tree_index)
    }

    fn neighbours_tree_indexes(&self, key: Bytes32) -> (u64, u64) {
        self.storage_tree.get_neighbours(&key)
    }
}