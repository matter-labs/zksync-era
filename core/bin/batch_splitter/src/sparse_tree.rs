//! Non-invasive Merkle-path regeneration via a sparse binary Merkle tree
//! reconstructed from batch N's own `WitnessInputMerklePaths` blob.
//!
//! We never touch the node's Merkle-tree RocksDB. The only sibling-hash source
//! is N's already-persisted, immutable multiproof (object store). From it we
//! rebuild the N-1 tree state restricted to the leaves N touched, then re-apply
//! piece A's writes (→ `root_A` + A's paths) and piece B's writes (→ `root_B` +
//! B's paths) on the same tree.
//!
//! ## Why this works
//! * **N-1 leaf values:** each touched key's *first* occurrence in N carries its
//!   pre-N value (`value_read`) and enumeration index — i.e., its N-1 state.
//! * **Boundary siblings:** a sibling subtree containing no touched leaf never
//!   changes during N, so its hash is identical in every op's proof and equals
//!   its N-1 value. We record those "off-trie" siblings; on-trie nodes are
//!   recomputed from the touched leaves.
//! * **Leaf indices:** existing keys keep their N-1 index; new keys get
//!   sequential indices from N's start index, continuing across A→B — the same
//!   order N used, which is why the final root matches.
//!
//! ## Hashing
//! The zkSync tree stores nodes 16-ary, but its proofs are the binarized
//! 256-level tree using `Blake2Hasher::{hash_leaf, hash_branch,
//! empty_subtree_hash}`. We reuse those exact primitives, so roots and paths
//! match byte-for-byte.
//!
//! ## Self-checks
//! [`SparseMerkle::build`] asserts the reconstructed N-1 root equals the batch's
//! `previous_batch_hash`; the caller asserts the post-B root equals N's
//! committed root. Together they validate the whole reconstruction.
//!
//! ## Known limitation (guarded)
//! A key that existed at N-1 but whose *only* interaction in N is a no-op update
//! (same value, which the tree omits from the witness) and is never read won't
//! appear in N's multiproof, so we'd misclassify it as absent. If a piece writes
//! such a key non-trivially the post-B root won't match N's and the self-check
//! fails loudly rather than emitting a bad witness. This is rare in practice.

use std::collections::{HashMap, HashSet};

use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;
use zksync_merkle_tree::HashTree;
use zksync_prover_interface::inputs::{StorageLogMetadata, WitnessInputMerklePaths};
use zksync_types::{StorageLog, H256, U256};

const TREE_DEPTH: usize = 256;

#[derive(Clone, Copy)]
struct Leaf {
    value: H256,
    leaf_index: u64,
}

pub struct SparseMerkle {
    hasher: Blake2Hasher,
    /// Touched keys (sorted, unique) — defines the "on-trie" region.
    sorted_keys: Vec<U256>,
    /// Current leaf state (starts at N-1, evolves as pieces are applied).
    leaves: HashMap<U256, Leaf>,
    /// Off-trie sibling hashes (invariant during N), keyed by (level, prefix).
    boundary: HashMap<(u16, U256), H256>,
    /// Current hashes of on-trie internal nodes (level >= 1), incl. the root.
    nodes: HashMap<(u16, U256), H256>,
    /// Next enumeration index for inserts; persists across A→B.
    next_index: u64,
}

impl SparseMerkle {
    /// Reconstruct the N-1 tree from N's multiproof and verify its root.
    ///
    /// * `orig` — batch N's `WitnessInputMerklePaths`.
    /// * `old_root` — N's `previous_batch_hash` (the N-1 root).
    /// * `start_index` — N's start enumeration index
    ///   (`orig.next_enumeration_index()`).
    pub fn build(
        orig: &WitnessInputMerklePaths,
        old_root: H256,
        start_index: u64,
    ) -> anyhow::Result<Self> {
        let logs: Vec<StorageLogMetadata> = orig.clone().into_merkle_paths().collect();

        // Pass 1: touched keys and their N-1 leaf state (first occurrence).
        let mut leaves: HashMap<U256, Leaf> = HashMap::new();
        let mut sorted_keys: Vec<U256> = Vec::new();
        let mut seen: HashSet<U256> = HashSet::new();
        for m in &logs {
            let key = m.leaf_hashed_key;
            if !seen.insert(key) {
                continue;
            }
            sorted_keys.push(key);
            // Existed at N-1 iff: a non-first write (Updated) or a read that hit
            // a leaf (enumeration index != 0). Inserts / ReadMissingKey => absent.
            let existed = if m.is_write {
                !m.first_write
            } else {
                m.leaf_enumeration_index != 0
            };
            if existed {
                leaves.insert(
                    key,
                    Leaf {
                        value: H256(m.value_read),
                        leaf_index: m.leaf_enumeration_index,
                    },
                );
            }
        }
        sorted_keys.sort_unstable();

        let mut me = Self {
            hasher: Blake2Hasher,
            sorted_keys,
            leaves,
            boundary: HashMap::new(),
            nodes: HashMap::new(),
            next_index: start_index,
        };

        // Pass 2: record off-trie boundary siblings from the (full) proofs.
        for m in &logs {
            anyhow::ensure!(
                m.merkle_paths.len() == TREE_DEPTH,
                "expected a full {TREE_DEPTH}-hash path after de-compaction, got {}",
                m.merkle_paths.len()
            );
            let key = m.leaf_hashed_key;
            for (d, sibling) in m.merkle_paths.iter().enumerate() {
                let sib_prefix = (key >> d) ^ U256::one();
                if !me.on_trie(d as u16, sib_prefix) {
                    me.boundary
                        .entry((d as u16, sib_prefix))
                        .or_insert_with(|| H256(*sibling));
                }
            }
        }

        // Build the on-trie node hashes for N-1 and verify the root.
        let root = me.compute(TREE_DEPTH as u16, U256::zero());
        anyhow::ensure!(
            root == old_root,
            "reconstructed N-1 root {root:?} != batch's previous_batch_hash {old_root:?}; \
             multiproof reconstruction is wrong",
        );
        Ok(me)
    }

    /// Apply one piece's storage logs, returning its witness paths and the
    /// resulting root. The witness's start index is the current `next_index`,
    /// which advances as inserts are applied (so a subsequent piece continues
    /// from the right index).
    pub fn process_piece(
        &mut self,
        logs: &[StorageLog],
    ) -> anyhow::Result<(WitnessInputMerklePaths, H256)> {
        let mut witness = WitnessInputMerklePaths::new(self.next_index);
        for log in logs {
            if let Some(meta) = self.apply(log) {
                witness.push_merkle_path(meta);
            }
        }
        Ok((witness, self.current_root()))
    }

    fn apply(&mut self, log: &StorageLog) -> Option<StorageLogMetadata> {
        let key = log.key.hashed_key_u256();
        if log.is_write() {
            let value = log.value;
            let (first_write, leaf_index, value_read) = match self.leaves.get(&key).copied() {
                Some(leaf) => {
                    if leaf.value == value {
                        // No-op update: the tree omits these from the witness.
                        return None;
                    }
                    (false, leaf.leaf_index, leaf.value)
                }
                None => {
                    let idx = self.next_index;
                    self.next_index += 1;
                    (true, idx, H256::zero())
                }
            };
            self.leaves.insert(key, Leaf { value, leaf_index });
            let (siblings, root) = self.recompute_path(key);
            Some(StorageLogMetadata {
                root_hash: root.0,
                is_write: true,
                first_write,
                merkle_paths: siblings.iter().map(|h| h.0).collect(),
                leaf_hashed_key: key,
                leaf_enumeration_index: leaf_index,
                value_written: value.0,
                value_read: value_read.0,
            })
        } else {
            let (leaf_index, value_read) = match self.leaves.get(&key) {
                Some(leaf) => (leaf.leaf_index, leaf.value),
                None => (0, H256::zero()),
            };
            let siblings = self.read_path(key);
            Some(StorageLogMetadata {
                root_hash: self.current_root().0,
                is_write: false,
                first_write: false,
                merkle_paths: siblings.iter().map(|h| h.0).collect(),
                leaf_hashed_key: key,
                leaf_enumeration_index: leaf_index,
                value_written: [0_u8; 32],
                value_read: value_read.0,
            })
        }
    }

    /// Update the nodes along `key`'s path after its leaf changed; returns the
    /// per-level sibling hashes (depth 0..256) and the new root.
    fn recompute_path(&mut self, key: U256) -> (Vec<H256>, H256) {
        let mut siblings = Vec::with_capacity(TREE_DEPTH);
        let mut node = self.leaf_hash(key);
        for d in 0..TREE_DEPTH {
            let prefix_d = key >> d;
            let sib = self.node_at(d as u16, prefix_d ^ U256::one());
            siblings.push(sib);
            node = if (prefix_d & U256::one()).is_zero() {
                self.hasher.hash_branch(&node, &sib)
            } else {
                self.hasher.hash_branch(&sib, &node)
            };
            self.nodes.insert(((d + 1) as u16, key >> (d + 1)), node);
        }
        (siblings, node)
    }

    /// Sibling hashes along `key`'s path without mutating the tree (for reads).
    fn read_path(&self, key: U256) -> Vec<H256> {
        (0..TREE_DEPTH)
            .map(|d| self.node_at(d as u16, (key >> d) ^ U256::one()))
            .collect()
    }

    /// Current hash of any node, reading the live `nodes`/`leaves`/`boundary`.
    fn node_at(&self, level: u16, prefix: U256) -> H256 {
        if level == 0 {
            return self.leaf_hash(prefix);
        }
        if self.on_trie(level, prefix) {
            *self
                .nodes
                .get(&(level, prefix))
                .expect("on-trie node must be present after build")
        } else {
            self.boundary
                .get(&(level, prefix))
                .copied()
                .unwrap_or_else(|| self.hasher.empty_subtree_hash(level as usize))
        }
    }

    fn current_root(&self) -> H256 {
        *self
            .nodes
            .get(&(TREE_DEPTH as u16, U256::zero()))
            .expect("root must be present after build")
    }

    fn leaf_hash(&self, key: U256) -> H256 {
        match self.leaves.get(&key) {
            Some(leaf) => self.hasher.hash_leaf(&leaf.value, leaf.leaf_index),
            None => self.hasher.empty_subtree_hash(0),
        }
    }

    /// Recursively compute (and memoize) an on-trie node hash during build.
    fn compute(&mut self, level: u16, prefix: U256) -> H256 {
        if level == 0 {
            return self.leaf_hash(prefix);
        }
        if !self.on_trie(level, prefix) {
            return self
                .boundary
                .get(&(level, prefix))
                .copied()
                .unwrap_or_else(|| self.hasher.empty_subtree_hash(level as usize));
        }
        if let Some(h) = self.nodes.get(&(level, prefix)) {
            return *h;
        }
        let left = self.compute(level - 1, prefix << 1);
        let right = self.compute(level - 1, (prefix << 1) | U256::one());
        let h = self.hasher.hash_branch(&left, &right);
        self.nodes.insert((level, prefix), h);
        h
    }

    /// Whether the subtree at `(level, prefix)` contains any touched key.
    fn on_trie(&self, level: u16, prefix: U256) -> bool {
        if self.sorted_keys.is_empty() {
            return false;
        }
        if level as usize >= TREE_DEPTH {
            return true;
        }
        let level = level as usize;
        let low = prefix << level;
        let high = low + ((U256::one() << level) - U256::one());
        let idx = self.sorted_keys.partition_point(|k| *k < low);
        idx < self.sorted_keys.len() && self.sorted_keys[idx] <= high
    }
}
