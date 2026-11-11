use serde::{Deserialize, Serialize};
use zkevm_test_harness::{
    witness::tree::{
        BinaryHasher, BinarySparseStorageTree, EnumeratedBinaryLeaf, LeafQuery, ZkSyncStorageLeaf,
    },
    zk_evm::blake2::Blake2s256,
};
use zksync_prover_interface::inputs::{StorageLogMetadata, WitnessInputMerklePaths};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PrecalculatedMerklePathsProvider {
    // We keep the root hash of the last processed leaf, as it is needed by the witness generator.
    pub root_hash: [u8; 32],
    // The ordered list of expected leaves to be interacted with
    pub pending_leaves: Vec<StorageLogMetadata>,
    // The index that would be assigned to the next new leaf
    pub next_enumeration_index: u64,
    // For every Storage Write Log we expect two invocations: `get_leaf` and `insert_leaf`.
    // We set this flag to `true` after the initial `get_leaf` is invoked.
    pub is_get_leaf_invoked: bool,
}

impl PrecalculatedMerklePathsProvider {
    pub fn new(input: WitnessInputMerklePaths, root_hash: [u8; 32]) -> Self {
        let next_enumeration_index = input.next_enumeration_index();
        tracing::debug!("Initializing PrecalculatedMerklePathsProvider. Initial root_hash: {:?}, initial next_enumeration_index: {:?}", root_hash, next_enumeration_index);
        Self {
            root_hash,
            pending_leaves: input.into_merkle_paths().collect(),
            next_enumeration_index,
            is_get_leaf_invoked: false,
        }
    }
}

impl BinarySparseStorageTree<256, 32, 32, 8, 32, Blake2s256, ZkSyncStorageLeaf>
    for PrecalculatedMerklePathsProvider
{
    fn empty() -> Self {
        unreachable!("`empty` must not be invoked by the witness generator code");
    }

    fn next_enumeration_index(&self) -> u64 {
        self.next_enumeration_index
    }

    fn set_next_enumeration_index(&mut self, _value: u64) {
        unreachable!(
            "`set_next_enumeration_index` must not be invoked by the witness generator code"
        );
    }

    fn root(&self) -> [u8; 32] {
        self.root_hash
    }

    fn get_leaf(&mut self, index: &[u8; 32]) -> LeafQuery<256, 32, 32, 32, ZkSyncStorageLeaf> {
        tracing::trace!(
            "Invoked get_leaf({:?}). pending leaves size: {:?}. current root: {:?}",
            index,
            self.pending_leaves.len(),
            self.root()
        );
        assert!(
            !self.is_get_leaf_invoked,
            "`get_leaf()` invoked more than once or get_leaf is invoked when insert_leaf was expected"
        );
        let next = self.pending_leaves.first().unwrap_or_else(|| {
            panic!(
                "invoked `get_leaf({:?})` with empty `pending_leaves`",
                index
            )
        });
        self.root_hash = next.root_hash;

        assert_eq!(
            &next.leaf_hashed_key_array(),
            index,
            "`get_leaf` hashed key mismatch"
        );

        let mut res = LeafQuery {
            leaf: ZkSyncStorageLeaf {
                index: next.leaf_enumeration_index,
                value: next.value_read,
            },
            first_write: next.first_write,
            index: *index,
            merkle_path: next.clone().into_merkle_paths_array(),
        };

        if next.is_write {
            // If it is a write, the next invocation will be `insert_leaf` with the very same parameters
            self.is_get_leaf_invoked = true;
            if res.first_write {
                res.leaf.index = 0;
            }
        } else {
            // If it is a read, the next invocation will relate to the next `pending_leaf`
            self.pending_leaves.remove(0);
        };

        res
    }

    fn insert_leaf(
        &mut self,
        index: &[u8; 32],
        leaf: ZkSyncStorageLeaf,
    ) -> LeafQuery<256, 32, 32, 32, ZkSyncStorageLeaf> {
        tracing::trace!(
            "Invoked insert_leaf({:?}). pending leaves size: {:?}. current root: {:?}",
            index,
            self.pending_leaves.len(),
            self.root()
        );

        assert!(
            self.is_get_leaf_invoked,
            "`get_leaf()` is expected to be invoked before `insert_leaf()`"
        );
        let next = self.pending_leaves.remove(0);
        self.root_hash = next.root_hash;

        assert!(
            next.is_write,
            "invoked `insert_leaf({:?})`, but get_leaf() expected",
            index
        );

        assert_eq!(
            &next.leaf_hashed_key_array(),
            index,
            "insert_leaf hashed key mismatch",
        );

        assert_eq!(
            &next.value_written, &leaf.value,
            "insert_leaf enumeration index mismatch",
        );

        // reset `is_get_leaf_invoked` for the next get/insert invocation
        self.is_get_leaf_invoked = false;

        // if this insert was in fact the very first insert, it should bump the `next_enumeration_index`
        self.next_enumeration_index = self
            .next_enumeration_index
            .max(next.leaf_enumeration_index + 1);

        LeafQuery {
            leaf: ZkSyncStorageLeaf {
                index: next.leaf_enumeration_index,
                value: next.value_written,
            },
            first_write: next.first_write,
            index: *index,
            merkle_path: next.into_merkle_paths_array(),
        }
    }

    // Method to segregate the given leafs into 2 types:
    //  * leafs that are updated for first time
    //  * leafs that are not updated for the first time.
    // The consumer of method must ensure that the length of passed argument indexes and leafs are same,
    // and the merkle paths specified during the initialization must contains same number of write
    // leaf nodes as that of the leafs passed as argument.
    fn filter_renumerate<'a>(
        &self,
        mut indexes: impl Iterator<Item = &'a [u8; 32]>,
        mut leafs: impl Iterator<Item = ZkSyncStorageLeaf>,
    ) -> (
        u64,
        Vec<([u8; 32], ZkSyncStorageLeaf)>,
        Vec<ZkSyncStorageLeaf>,
    ) {
        tracing::trace!(
            "invoked filter_renumerate(), pending leaves size: {:?}",
            self.pending_leaves.len()
        );
        let mut first_writes = vec![];
        let mut updates = vec![];
        let write_pending_leaves = self
            .pending_leaves
            .iter()
            .filter(|&l| l.is_write)
            .collect::<Vec<&StorageLogMetadata>>();
        let write_pending_leaves_iter = write_pending_leaves.iter();
        let mut length = 0;
        for (&pending_leaf, (idx, mut leaf)) in
            write_pending_leaves_iter.zip((&mut indexes).zip(&mut leafs))
        {
            leaf.set_index(pending_leaf.leaf_enumeration_index);
            if pending_leaf.first_write {
                first_writes.push((*idx, leaf));
            } else {
                updates.push(leaf);
            }
            length += 1;
        }
        assert_eq!(
            length,
            write_pending_leaves.len(),
            "pending leaves: len({}) must be of same length as leafs and indexes: len({})",
            write_pending_leaves.len(),
            length
        );
        assert!(
            indexes.next().is_none(),
            "indexes must be of same length as leafs and pending leaves: len({})",
            write_pending_leaves.len()
        );
        assert!(
            leafs.next().is_none(),
            "leafs must be of same length as indexes and pending leaves: len({})",
            write_pending_leaves.len()
        );
        (self.next_enumeration_index, first_writes, updates)
    }

    fn verify_inclusion(
        root: &[u8; 32],
        query: &LeafQuery<256, 32, 32, 32, ZkSyncStorageLeaf>,
    ) -> bool {
        //copied from `zkevm_test_harness/src/witness/tree/mod.rs` with minor changes
        tracing::trace!(
            "invoked verify_inclusion. Index: {:?}, root: {:?})",
            query.index,
            root
        );

        let mut leaf_bytes = vec![0u8; 8 + 32]; // can make a scratch space somewhere later on
        leaf_bytes[8..].copy_from_slice(query.leaf.value());

        let leaf_index_bytes = query.leaf.current_index().to_be_bytes();
        leaf_bytes[0..8].copy_from_slice(&leaf_index_bytes);

        let leaf_hash = Blake2s256::leaf_hash(&leaf_bytes);

        let mut current_hash = leaf_hash;
        for level in 0..256 {
            let (l, r) = if is_right_side_node(&query.index, level) {
                (&query.merkle_path[level], &current_hash)
            } else {
                (&current_hash, &query.merkle_path[level])
            };

            let this_level_hash = Blake2s256::node_hash(level, l, r);

            current_hash = this_level_hash;
        }

        root == &current_hash
    }
}

fn is_right_side_node<const N: usize>(index: &[u8; N], depth: usize) -> bool {
    debug_assert!(depth < N * 8);
    let byte_idx = depth / 8;
    let bit_idx = depth % 8;

    index[byte_idx] & (1u8 << bit_idx) != 0
}
