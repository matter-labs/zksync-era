//! Parallel storage implementation.

use std::{
    any::Any,
    collections::{HashMap, VecDeque},
    error::Error as StdError,
    mem, ops,
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use anyhow::Context as _;

use super::{patch::PartialPatchSet, Database, NodeKeys, PatchSet};
use crate::{
    errors::DeserializeError,
    metrics::{RecoveryStage, RECOVERY_METRICS},
    types::{Manifest, Node, NodeKey, ProfiledTreeOperation, Root},
    PruneDatabase, PrunePatchSet,
};

/// Persistence command passed to a persistence thread over a bounded channel.
#[derive(Debug, Clone)]
struct PersistenceCommand {
    manifest: Manifest,
    patch: Arc<PartialPatchSet>,
    stale_keys: Vec<NodeKey>,
}

/// Command to a background persistence thread.
#[derive(Debug)]
enum Command {
    Persist(PersistenceCommand),
    Stop(mpsc::SyncSender<()>),
}

/// Handle allowing to control background persistence for Merkle tree.
#[derive(Debug)]
pub struct PersistenceThreadHandle {
    command_sender: mpsc::SyncSender<Command>,
}

impl PersistenceThreadHandle {
    /// Emulates stopping persisting updates; any updates afterwards will not actually be persisted.
    ///
    /// This method should only be used in tests. It is blocking (waits until all previous persistence commands are processed).
    pub fn test_stop_processing(self) {
        let (stop_sender, stop_receiver) = mpsc::sync_channel(0);
        self.command_sender.send(Command::Stop(stop_sender)).ok();
        stop_receiver.recv().ok();
    }
}

/// Thread join handle, or an error produced by the thread.
#[derive(Debug, Default)]
enum HandleOrError {
    #[default]
    Nothing,
    Handle(thread::JoinHandle<anyhow::Result<()>>),
    Err(Arc<dyn StdError + Send + Sync>),
}

impl HandleOrError {
    /// Checks whether the thread handle has exited, and returns an error if it exited with an error.
    /// If `join` is set, waits for the thread handle to exit.
    fn check(&mut self, join: bool) -> anyhow::Result<()> {
        let err_arc = match self {
            Self::Handle(handle) if join || handle.is_finished() => {
                let Self::Handle(handle) = mem::take(self) else {
                    unreachable!("just checked variant earlier");
                };
                let err = match handle.join() {
                    Err(_) => anyhow::anyhow!("persistence thread panicked"),
                    // Handling normal exits depends on whether we expect the thread to exit.
                    Ok(Ok(())) if join => return Ok(()),
                    Ok(Ok(())) => anyhow::anyhow!("persistence thread unexpectedly stopped"),
                    Ok(Err(err)) => err,
                };
                let err: Box<dyn StdError + Send + Sync> = err.into();
                let err: Arc<dyn StdError + Send + Sync> = err.into();
                *self = Self::Err(err.clone());
                err
            }
            Self::Handle(_) => return Ok(()),
            Self::Err(err) => err.clone(),
            Self::Nothing => unreachable!("only used temporarily to take out `JoinHandle`"),
        };
        Err(anyhow::Error::new(err_arc))
    }

    fn join(mut self) -> anyhow::Result<()> {
        self.check(true)
    }
}

/// Database implementation that persists changes in a background thread. Not yet applied changes
/// are queued up and are used in `Database` methods. A queue can sometimes be stale (i.e., changes
/// at its head may have been applied), but this is fine as long as changes are applied atomically and sequentially.
///
/// The only use case where this struct is used right now is tree recovery. Correspondingly, some reported metrics
/// are specific to recovery and would need to be reworked if this struct is eventually used for other use cases.
///
/// # Assumptions
///
/// - This is the only mutable database instance.
/// - All database updates update the same tree version.
/// - The application supports latest changes being dropped.
#[derive(Debug)]
pub(crate) struct ParallelDatabase<DB> {
    inner: DB,
    updated_version: u64,
    command_sender: mpsc::SyncSender<Command>,
    persistence_handle: HandleOrError,
    commands: VecDeque<PersistenceCommand>,
}

impl<DB: Database + Clone + 'static> ParallelDatabase<DB> {
    fn new(inner: DB, updated_version: u64, buffer_capacity: usize) -> Self {
        let (command_sender, command_receiver) = mpsc::sync_channel(buffer_capacity);
        let persistence_database = inner.clone();
        Self {
            inner,
            updated_version,
            command_sender,
            persistence_handle: HandleOrError::Handle(thread::spawn(move || {
                Self::run_persistence(persistence_database, updated_version, command_receiver)
            })),
            commands: VecDeque::with_capacity(buffer_capacity),
        }
    }

    fn persistence_thread_handle(&self) -> PersistenceThreadHandle {
        PersistenceThreadHandle {
            command_sender: self.command_sender.clone(),
        }
    }

    fn run_persistence(
        mut database: DB,
        updated_version: u64,
        command_receiver: mpsc::Receiver<Command>,
    ) -> anyhow::Result<()> {
        let mut persisted_count = 0;
        while let Ok(command) = command_receiver.recv() {
            let command = match command {
                Command::Persist(command) => command,
                Command::Stop(_sender) => {
                    // Ensure that `PersistenceThreadHandle::test_stop_processing()` returns after the processing loop terminates.
                    drop(command_receiver);
                    anyhow::bail!("emulated persistence crash");
                }
            };

            tracing::debug!(
                "Persisting patch #{persisted_count} with {} nodes and {} stale keys",
                command.patch.nodes.len(),
                command.stale_keys.len()
            );
            // Reconstitute a `PatchSet` and apply it to the underlying database.
            let patch = PatchSet {
                manifest: command.manifest,
                patches_by_version: HashMap::from([(updated_version, command.patch.cloned())]),
                updated_version: Some(updated_version),
                stale_keys_by_version: HashMap::from([(updated_version, command.stale_keys)]),
            };
            let stage_latency =
                RECOVERY_METRICS.stage_latency[&RecoveryStage::ParallelPersistence].start();
            database.apply_patch(patch)?;
            let stage_latency = stage_latency.observe();
            tracing::debug!("Persisted patch #{persisted_count} in {stage_latency:?}");
            persisted_count += 1;
        }
        Ok(())
    }
}

impl<DB: Database> ParallelDatabase<DB> {
    fn wait_sync(&mut self) -> anyhow::Result<()> {
        while !self.commands.is_empty() {
            self.commands
                .retain(|command| Arc::strong_count(&command.patch) > 1);
            thread::sleep(Duration::from_millis(50)); // TODO: more intelligent approach
        }
        RECOVERY_METRICS.parallel_persistence_buffer_size.set(0);

        // Check that the persistence thread hasn't panicked
        self.persistence_handle.check(false)
    }

    fn join(self) -> anyhow::Result<DB> {
        drop(self.command_sender);
        drop(self.commands);
        RECOVERY_METRICS.parallel_persistence_buffer_size.set(0);
        self.persistence_handle.join()?;
        Ok(self.inner)
    }
}

impl<DB: Database> Database for ParallelDatabase<DB> {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        let latest_command = self.commands.iter().next_back();
        if let Some(command) = latest_command {
            Ok(Some(command.manifest.clone()))
        } else {
            self.inner.try_manifest()
        }
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        if version != self.updated_version {
            return self.inner.try_root(version);
        }
        let root = self
            .commands
            .iter()
            .rev()
            .find_map(|command| command.patch.root.clone());
        if let Some(root) = root {
            Ok(Some(root))
        } else {
            self.inner.try_root(version)
        }
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        if key.version != self.updated_version {
            return self.inner.try_tree_node(key, is_leaf);
        }

        let node = self
            .commands
            .iter()
            .rev()
            .find_map(|command| command.patch.nodes.get(key).cloned());
        if let Some(node) = node {
            debug_assert_eq!(matches!(node, Node::Leaf(_)), is_leaf);
            Ok(Some(node))
        } else {
            self.inner.try_tree_node(key, is_leaf)
        }
    }

    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        let mut nodes = vec![None; keys.len()];
        for command in self.commands.iter().rev() {
            for (key_idx, (key, is_leaf)) in keys.iter().enumerate() {
                if nodes[key_idx].is_some() {
                    continue;
                }
                if let Some(node) = command.patch.nodes.get(key) {
                    debug_assert_eq!(matches!(node, Node::Leaf(_)), *is_leaf);
                    nodes[key_idx] = Some(node.clone());
                }
            }
        }

        // Load missing nodes from the underlying database
        let (key_indexes, missing_keys): (Vec<_>, Vec<_>) = keys
            .iter()
            .copied()
            .enumerate()
            .filter(|(i, _)| nodes[*i].is_none())
            .unzip();
        let inner_nodes = self.inner.tree_nodes(&missing_keys);
        for (key_idx, node) in key_indexes.into_iter().zip(inner_nodes) {
            nodes[key_idx] = node;
        }
        nodes
    }

    fn start_profiling(&self, operation: ProfiledTreeOperation) -> Box<dyn Any> {
        self.inner.start_profiling(operation)
    }

    fn apply_patch(&mut self, mut patch: PatchSet) -> anyhow::Result<()> {
        let partial_patch = if let Some(updated_version) = patch.updated_version {
            anyhow::ensure!(
                updated_version == self.updated_version,
                "Unsupported update: must update predefined version {}",
                self.updated_version
            );
            anyhow::ensure!(
                patch.patches_by_version.len() == 1,
                "Unsupported update: must *only* update version {updated_version}"
            );

            // Garbage-collect patches already applied by the persistence thread. This will remove all patches
            // if the persistence thread has failed, but this is OK because we'll propagate the failure below anyway.
            self.commands
                .retain(|command| Arc::strong_count(&command.patch) > 1);
            RECOVERY_METRICS
                .parallel_persistence_buffer_size
                .set(self.commands.len());
            tracing::debug!(
                "Retained {} buffered persistence command(s)",
                self.commands.len()
            );

            patch
                .patches_by_version
                .remove(&updated_version)
                .context("PatchSet invariant violated: missing patch for the updated version")?
        } else {
            // We only support manifest updates.
            anyhow::ensure!(
                patch.patches_by_version.is_empty(),
                "Invalid update: {patch:?}"
            );
            PartialPatchSet::empty()
        };

        let mut stale_keys_by_version = patch.stale_keys_by_version;
        anyhow::ensure!(
            stale_keys_by_version.is_empty()
                || (stale_keys_by_version.len() == 1
                    && stale_keys_by_version.contains_key(&self.updated_version)),
            "Invalid stale keys update: {stale_keys_by_version:?}"
        );
        let stale_keys = stale_keys_by_version
            .remove(&self.updated_version)
            .unwrap_or_default();

        let command = PersistenceCommand {
            manifest: patch.manifest,
            patch: Arc::new(partial_patch),
            stale_keys,
        };
        if self
            .command_sender
            .send(Command::Persist(command.clone()))
            .is_err()
        {
            self.persistence_handle.check(true)?;
            anyhow::bail!(
                "persistence thread never exits normally when `ParallelDatabase` is alive"
            );
        }
        self.commands.push_back(command);
        RECOVERY_METRICS.parallel_persistence_buffer_size.inc_by(1);
        Ok(())
    }
}

impl<DB: PruneDatabase> PruneDatabase for ParallelDatabase<DB> {
    fn min_stale_key_version(&self) -> Option<u64> {
        let commands_have_stale_keys = self
            .commands
            .iter()
            .any(|command| !command.stale_keys.is_empty());
        if commands_have_stale_keys {
            return Some(self.updated_version);
        }
        self.inner.min_stale_key_version()
    }

    fn stale_keys(&self, version: u64) -> Vec<NodeKey> {
        if version != self.updated_version {
            return self.inner.stale_keys(version);
        }
        self.commands
            .iter()
            .flat_map(|command| command.stale_keys.clone())
            .chain(self.inner.stale_keys(version))
            .collect()
    }

    fn prune(&mut self, patch: PrunePatchSet) -> anyhow::Result<()> {
        // Require the underlying database to be fully synced.
        self.wait_sync()
            .context("failed synchronizing database before pruning")?;
        self.inner.prune(patch)
    }

    fn truncate(
        &mut self,
        manifest: Manifest,
        truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()> {
        // Require the underlying database to be fully synced.
        self.wait_sync()
            .context("failed synchronizing database before truncation")?;
        self.inner.truncate(manifest, truncated_versions)
    }
}

/// Database with either sequential or parallel persistence.
#[derive(Debug)]
pub(crate) enum MaybeParallel<DB> {
    Sequential(DB),
    Parallel(ParallelDatabase<DB>),
}

impl<DB: PruneDatabase> MaybeParallel<DB> {
    pub fn join(self) -> anyhow::Result<DB> {
        match self {
            Self::Sequential(db) => Ok(db),
            Self::Parallel(db) => db.join(),
        }
    }
}

impl<DB: 'static + Clone + PruneDatabase> MaybeParallel<DB> {
    pub fn parallelize(
        &mut self,
        updated_version: u64,
        buffer_capacity: usize,
    ) -> Option<PersistenceThreadHandle> {
        if let Self::Sequential(db) = self {
            let db = ParallelDatabase::new(db.clone(), updated_version, buffer_capacity);
            let handle = db.persistence_thread_handle();
            *self = Self::Parallel(db);
            Some(handle)
        } else {
            None
        }
    }
}

impl<DB: Database> Database for MaybeParallel<DB> {
    fn try_manifest(&self) -> Result<Option<Manifest>, DeserializeError> {
        match self {
            Self::Sequential(db) => db.try_manifest(),
            Self::Parallel(db) => db.try_manifest(),
        }
    }

    fn try_root(&self, version: u64) -> Result<Option<Root>, DeserializeError> {
        match self {
            Self::Sequential(db) => db.try_root(version),
            Self::Parallel(db) => db.try_root(version),
        }
    }

    fn try_tree_node(
        &self,
        key: &NodeKey,
        is_leaf: bool,
    ) -> Result<Option<Node>, DeserializeError> {
        match self {
            Self::Sequential(db) => db.try_tree_node(key, is_leaf),
            Self::Parallel(db) => db.try_tree_node(key, is_leaf),
        }
    }

    fn tree_nodes(&self, keys: &NodeKeys) -> Vec<Option<Node>> {
        match self {
            Self::Sequential(db) => db.tree_nodes(keys),
            Self::Parallel(db) => db.tree_nodes(keys),
        }
    }

    fn start_profiling(&self, operation: ProfiledTreeOperation) -> Box<dyn Any> {
        match self {
            Self::Sequential(db) => db.start_profiling(operation),
            Self::Parallel(db) => db.start_profiling(operation),
        }
    }

    fn apply_patch(&mut self, patch: PatchSet) -> anyhow::Result<()> {
        match self {
            Self::Sequential(db) => db.apply_patch(patch),
            Self::Parallel(db) => db.apply_patch(patch),
        }
    }
}

impl<DB: PruneDatabase> PruneDatabase for MaybeParallel<DB> {
    fn min_stale_key_version(&self) -> Option<u64> {
        match self {
            Self::Sequential(db) => db.min_stale_key_version(),
            Self::Parallel(db) => db.min_stale_key_version(),
        }
    }

    fn stale_keys(&self, version: u64) -> Vec<NodeKey> {
        match self {
            Self::Sequential(db) => db.stale_keys(version),
            Self::Parallel(db) => db.stale_keys(version),
        }
    }

    fn prune(&mut self, patch: PrunePatchSet) -> anyhow::Result<()> {
        match self {
            Self::Sequential(db) => db.prune(patch),
            Self::Parallel(db) => db.prune(patch),
        }
    }

    fn truncate(
        &mut self,
        manifest: Manifest,
        truncated_versions: ops::RangeTo<u64>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Sequential(db) => db.truncate(manifest, truncated_versions),
            Self::Parallel(db) => db.truncate(manifest, truncated_versions),
        }
    }
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use tempfile::TempDir;

    use super::*;
    use crate::{
        storage::Operation,
        types::{ChildRef, InternalNode, LeafNode, Nibbles},
        Key, RocksDBWrapper, TreeEntry, ValueHash,
    };

    const UPDATED_VERSION: u64 = 10;

    fn mock_patch_set(start: u64, leaf_count: u64) -> PatchSet {
        assert!(start <= leaf_count);

        let manifest = Manifest::new(UPDATED_VERSION + 1, &());
        let mut root_node = InternalNode::default();
        root_node.insert_child_ref(0, ChildRef::leaf(UPDATED_VERSION));
        let root = Root::new(leaf_count, Node::Internal(root_node));
        let nodes = (start..leaf_count)
            .map(|i| {
                let key = Key::from(i);
                let node_key = Nibbles::new(&key, 64).with_version(UPDATED_VERSION);
                let leaf = LeafNode::new(TreeEntry {
                    key,
                    value: ValueHash::zero(),
                    leaf_index: i + 1,
                });
                (node_key, Node::from(leaf))
            })
            .collect();
        PatchSet::new(
            manifest,
            UPDATED_VERSION,
            root,
            nodes,
            vec![],
            Operation::Update,
        )
    }

    #[test]
    fn database_methods_with_parallel_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();

        let mut parallel_db = ParallelDatabase::new(db.clone(), UPDATED_VERSION, 1);
        assert!(parallel_db.manifest().is_none());
        let manifest = Manifest::new(UPDATED_VERSION, &());
        parallel_db
            .apply_patch(PatchSet::from_manifest(manifest))
            .unwrap();
        assert_eq!(parallel_db.commands.len(), 1);
        assert_eq!(
            parallel_db.manifest().unwrap().version_count,
            UPDATED_VERSION
        );

        parallel_db.apply_patch(mock_patch_set(0, 10)).unwrap();
        assert_eq!(parallel_db.root(UPDATED_VERSION).unwrap().leaf_count(), 10);

        let keys: Vec<_> = (0..20)
            .map(|i| {
                (
                    Nibbles::new(&Key::from(i), 64).with_version(UPDATED_VERSION),
                    true,
                )
            })
            .collect();

        let nodes = parallel_db.tree_nodes(&keys);
        for (i, node) in nodes[..10].iter().enumerate() {
            assert_matches!(
                node.as_ref().unwrap(),
                Node::Leaf(leaf) if leaf.leaf_index == i as u64 + 1
            );
        }
        for node in &nodes[10..] {
            assert!(node.is_none(), "{node:?}");
        }

        parallel_db.apply_patch(mock_patch_set(10, 15)).unwrap();

        let nodes = parallel_db.tree_nodes(&keys);
        for (i, node) in nodes[..15].iter().enumerate() {
            assert_matches!(
                node.as_ref().unwrap(),
                Node::Leaf(leaf) if leaf.leaf_index == i as u64 + 1
            );
        }
        for node in &nodes[15..] {
            assert!(node.is_none(), "{node:?}");
        }

        parallel_db.wait_sync().unwrap();

        let nodes = parallel_db.tree_nodes(&keys);
        for (i, node) in nodes[..15].iter().enumerate() {
            assert_matches!(
                node.as_ref().unwrap(),
                Node::Leaf(leaf) if leaf.leaf_index == i as u64 + 1
            );
        }
        for node in &nodes[15..] {
            assert!(node.is_none(), "{node:?}");
        }

        parallel_db.join().unwrap();
    }

    #[test]
    fn fault_injection_with_parallel_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let db = RocksDBWrapper::new(temp_dir.path()).unwrap();

        let mut parallel_db = ParallelDatabase::new(db.clone(), UPDATED_VERSION, 4);
        let handle = parallel_db.persistence_thread_handle();

        // Queue up a couple of patch sets
        parallel_db.apply_patch(mock_patch_set(0, 5)).unwrap();
        assert_eq!(parallel_db.root(UPDATED_VERSION).unwrap().leaf_count(), 5);
        parallel_db.apply_patch(mock_patch_set(5, 10)).unwrap();
        assert_eq!(parallel_db.root(UPDATED_VERSION).unwrap().leaf_count(), 10);
        // Emulate the persistence thread stopping (e.g., due to the process crashing)
        handle.test_stop_processing();

        // Queue another patch set.
        let err = parallel_db
            .apply_patch(mock_patch_set(10, 15))
            .unwrap_err()
            .to_string();
        assert!(err.contains("emulated persistence crash"), "{err}");

        let err = parallel_db.join().unwrap_err().to_string();
        assert!(err.contains("emulated persistence crash"), "{err}");

        // Check that the last patch set was dropped.
        assert_eq!(db.root(UPDATED_VERSION).unwrap().leaf_count(), 10);
    }
}
