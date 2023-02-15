//! Merkle Tree shared configuration.

use core::fmt::Debug;
use core::iter::once;
use std::sync::Arc;
use zksync_config::constants::ROOT_TREE_DEPTH;

use crate::{types::*, Bytes, Hasher};

#[derive(Debug)]
struct TreeConfigInner<H> {
    /// Hash generator used to hash entries.
    pub(crate) hasher: H,
    /// Precalculated empty leaf tree hashes. Start from leaf.
    pub(crate) empty_tree: Vec<NodeEntry>,
}

/// Shared configuration for Sparse Merkle Tree.
#[derive(Clone, Debug)]
pub struct TreeConfig<H> {
    inner: Arc<TreeConfigInner<H>>,
}

impl<H> TreeConfig<H>
where
    H: Hasher<Bytes>,
{
    /// Creates new shared config with supplied params.
    pub fn new(hasher: H) -> Self {
        let empty_hashes = Self::calc_default_hashes(ROOT_TREE_DEPTH, &hasher);

        Self {
            inner: Arc::new(TreeConfigInner {
                empty_tree: Self::calc_empty_tree(&empty_hashes),
                hasher,
            }),
        }
    }

    /// Produces tree with all leaves having default value (empty).
    fn calc_empty_tree(hashes: &[Vec<u8>]) -> Vec<NodeEntry> {
        let mut empty_tree: Vec<_> = hashes
            .iter()
            .rev()
            .zip(once(None).chain(hashes.iter().rev().map(Some)))
            .map(|(hash, prev_hash)| match prev_hash {
                None => NodeEntry::Leaf { hash: hash.clone() },
                Some(prev_hash) => NodeEntry::Branch {
                    hash: hash.clone(),
                    left_hash: prev_hash.clone(),
                    right_hash: prev_hash.clone(),
                },
            })
            .collect();
        empty_tree.reverse();

        empty_tree
    }

    /// Returns reference to precalculated empty Merkle Tree hashes starting from leaf.
    pub fn empty_tree(&self) -> &[NodeEntry] {
        &self.inner.empty_tree
    }

    pub fn empty_leaf(hasher: &H) -> ZkHash {
        hasher.hash_bytes([0; 40])
    }

    pub fn default_root_hash(&self) -> ZkHash {
        self.empty_tree().first().cloned().unwrap().into_hash()
    }

    /// Returns current hasher.
    pub fn hasher(&self) -> &H {
        &self.inner.hasher
    }

    /// Calculates default empty leaf hashes for given types.
    fn calc_default_hashes(depth: usize, hasher: &H) -> Vec<Vec<u8>> {
        let mut def_hashes = Vec::with_capacity(depth + 1);
        def_hashes.push(Self::empty_leaf(hasher));
        for _ in 0..depth {
            let last_hash = def_hashes.last().unwrap();
            let hash = hasher.compress(last_hash, last_hash);

            def_hashes.push(hash);
        }
        def_hashes.reverse();

        def_hashes
    }
}
