//! Merkle tree reader.

use std::fmt;

use zksync_basic_types::H256;

use crate::{
    hasher::BatchTreeProof,
    types::{NodeKey, RawNode},
    Database, DefaultTreeParams, MerkleTree, RocksDBWrapper, TreeParams,
};

pub struct MerkleTreeReader<DB, P: TreeParams = DefaultTreeParams>(MerkleTree<DB, P>);

impl<DB: fmt::Debug, P: TreeParams> fmt::Debug for MerkleTreeReader<DB, P>
where
    P::Hasher: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<DB: Database + Clone, P: TreeParams> Clone for MerkleTreeReader<DB, P>
where
    P::Hasher: Clone,
{
    fn clone(&self) -> Self {
        Self(MerkleTree {
            db: self.0.db.clone(),
            hasher: self.0.hasher.clone(),
        })
    }
}

impl<DB: Database> MerkleTreeReader<DB> {
    /// Creates a tree reader based on the provided database.
    ///
    /// # Errors
    ///
    /// Errors if sanity checks fail.
    pub fn new(db: DB) -> anyhow::Result<Self> {
        MerkleTree::new(db).map(Self)
    }
}

impl<DB: Database, P: TreeParams> MerkleTreeReader<DB, P> {
    /// Returns a reference to the database.
    pub fn db(&self) -> &DB {
        &self.0.db
    }

    /// Converts this reader to the underlying DB.
    pub fn into_db(self) -> DB {
        self.0.db
    }

    /// Returns the latest version of the tree present in the database, or `None` if
    /// no versions are present yet.
    pub fn latest_version(&self) -> anyhow::Result<Option<u64>> {
        self.0.latest_version()
    }

    /// Returns the root hash and leaf count at the specified version.
    pub fn root_info(&self, version: u64) -> anyhow::Result<Option<(H256, u64)>> {
        self.0.root_info(version)
    }

    /// Creates a batch proof for `keys` at the specified tree version.
    ///
    /// # Errors
    ///
    /// - Returns an error if the version doesn't exist.
    /// - Proxies database errors.
    pub fn prove(&self, version: u64, keys: &[H256]) -> anyhow::Result<BatchTreeProof> {
        self.0.prove(version, keys)
    }
}

impl<P: TreeParams> MerkleTreeReader<RocksDBWrapper, P> {
    /// Returns raw nodes for the specified `keys`.
    pub fn raw_nodes(&self, node_keys: &[NodeKey]) -> anyhow::Result<Vec<Option<RawNode>>> {
        let raw_nodes = self.0.db.raw_nodes(node_keys).into_iter();
        let raw_nodes = node_keys.iter().zip(raw_nodes).map(|(key, slice)| {
            let slice = slice?;
            Some(if key.nibble_count == 0 {
                RawNode::deserialize_root(&slice)
            } else {
                RawNode::deserialize(&slice)
            })
        });
        Ok(raw_nodes.collect())
    }
}
