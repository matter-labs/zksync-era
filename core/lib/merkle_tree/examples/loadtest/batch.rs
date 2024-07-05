//! `Database` implementation that flushes changes at the specified batches.

use std::any::Any;

use zksync_merkle_tree::{
    unstable::{DeserializeError, Manifest, Node, NodeKey, ProfiledTreeOperation, Root},
    Database, PatchSet, Patched,
};

pub struct WithBatching<'a> {
    inner: Patched<&'a mut dyn Database>,
    batch_size: usize,
    in_memory_batch_size: usize,
}

impl<'a> WithBatching<'a> {
    pub fn new(db: &'a mut dyn Database, batch_size: usize) -> Self {
        assert!(batch_size > 0, "Batch size must be positive");
        Self {
            inner: Patched::new(db),
            batch_size,
            in_memory_batch_size: 0,
        }
    }
}

impl Database for WithBatching<'_> {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        self.inner.try_manifest()
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        self.inner.try_root(version)
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        self.inner.try_tree_node(key, is_leaf)
    }

    fn tree_nodes(&self, keys: &[(NodeKey, bool)]) -> Vec<Option<Node>> {
        self.inner.tree_nodes(keys)
    }

    fn start_profiling(&self, operation: ProfiledTreeOperation) -> Box<dyn Any> {
        self.inner.start_profiling(operation)
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        self.inner.apply_patch(patch)?;

        self.in_memory_batch_size += 1;
        if self.in_memory_batch_size >= self.batch_size {
            println!("Flushing changes to underlying DB");
            self.inner.flush()?;
            self.in_memory_batch_size = 0;
        }
        Ok(())
    }
}
