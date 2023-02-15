use crate::patch::{TreePatch, Update, UpdatesBatch};
use crate::storage::{serialize_leaf_index, Storage};
use crate::tree_config::TreeConfig;
use crate::types::{
    LeafIndices, LevelIndex, NodeEntry, TreeKey, TreeMetadata, TreeOperation, TreeValue, ZkHash,
    ZkHasher,
};
use crate::utils::children_idxs;
use crate::{utils, TreeError};
use itertools::Itertools;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use std::borrow::Borrow;
use std::collections::{hash_map::Entry, HashMap, HashSet};
use std::iter::once;
use std::sync::Arc;
use tokio::time::Instant;
use zksync_config::constants::ROOT_TREE_DEPTH;
use zksync_crypto::hasher::Hasher;
use zksync_storage::RocksDB;
use zksync_types::proofs::StorageLogMetadata;
use zksync_types::{L1BatchNumber, StorageLogKind, WitnessStorageLog, H256};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TreeMode {
    Full,
    Lightweight,
}

#[derive(Debug)]
pub struct ZkSyncTree {
    storage: Storage,
    config: TreeConfig<ZkHasher>,
    root_hash: ZkHash,
    block_number: u32,
    mode: TreeMode,
}

impl ZkSyncTree {
    /// Creates new ZkSyncTree instance
    pub fn new_with_mode(db: RocksDB, mode: TreeMode) -> Self {
        let storage = Storage::new(db);
        let config = TreeConfig::new(ZkHasher::default());
        let (root_hash, block_number) = storage.fetch_metadata();
        let root_hash = root_hash.unwrap_or_else(|| config.default_root_hash());

        Self {
            storage,
            config,
            root_hash,
            block_number,
            mode,
        }
    }

    pub fn new(db: RocksDB) -> Self {
        Self::new_with_mode(db, TreeMode::Full)
    }

    pub fn new_lightweight(db: RocksDB) -> Self {
        Self::new_with_mode(db, TreeMode::Lightweight)
    }

    pub fn root_hash(&self) -> ZkHash {
        self.root_hash.clone()
    }

    pub fn is_empty(&self) -> bool {
        self.root_hash == self.config.default_root_hash()
    }

    pub fn block_number(&self) -> u32 {
        self.block_number
    }

    /// Returns current hasher.
    fn hasher(&self) -> &ZkHasher {
        self.config.hasher()
    }

    /// Processes an iterator of block logs, interpreting each nested iterator as a block.
    /// Before going to the next block, the current block will be sealed.
    /// Returns tree metadata for the corresponding blocks.
    ///
    /// - `storage_logs` - an iterator of storage logs for a given block
    pub fn process_block<I>(&mut self, storage_logs: I) -> TreeMetadata
    where
        I: IntoIterator,
        I::Item: Borrow<WitnessStorageLog>,
    {
        self.process_blocks(once(storage_logs)).pop().unwrap()
    }

    pub fn process_blocks<I>(&mut self, blocks: I) -> Vec<TreeMetadata>
    where
        I: IntoIterator,
        I::Item: IntoIterator,
        <I::Item as IntoIterator>::Item: Borrow<WitnessStorageLog>,
    {
        // Filter out reading logs and convert writing to the key-value pairs
        let tree_operations: Vec<_> = blocks
            .into_iter()
            .enumerate()
            .map(|(i, logs)| {
                let tree_operations: Vec<_> = logs
                    .into_iter()
                    .map(|log| {
                        let operation = match log.borrow().storage_log.kind {
                            StorageLogKind::Write => TreeOperation::Write {
                                value: log.borrow().storage_log.value,
                                previous_value: log.borrow().previous_value,
                            },
                            StorageLogKind::Read => {
                                TreeOperation::Read(log.borrow().storage_log.value)
                            }
                        };
                        (log.borrow().storage_log.key.hashed_key_u256(), operation)
                    })
                    .collect();

                vlog::info!(
                    "Tree {:?}, processing block {}, with {} logs",
                    self.mode,
                    self.block_number + i as u32,
                    tree_operations.len(),
                );

                tree_operations
            })
            .collect();

        assert!(
            self.mode == TreeMode::Full || tree_operations.len() == 1,
            "Tried to process multiple blocks in lightweight mode"
        );

        // Apply all tree operations
        self.apply_updates_batch(tree_operations)
            .expect("Failed to apply logs")
    }

    fn apply_updates_batch(
        &mut self,
        updates_batch: Vec<Vec<(TreeKey, TreeOperation)>>,
    ) -> Result<Vec<TreeMetadata>, TreeError> {
        let start = Instant::now();
        let total_blocks = updates_batch.len();

        let storage_logs_with_blocks: Vec<_> = updates_batch
            .into_iter()
            .enumerate()
            .flat_map(|(i, logs)| logs.into_iter().map(move |log| (i, log)))
            .collect();

        let mut leaf_indices = self
            .storage
            .process_leaf_indices(&storage_logs_with_blocks)?;

        let storage_logs_with_indices: Vec<_> = storage_logs_with_blocks
            .iter()
            .map(|&(block, (key, operation))| {
                let leaf_index = leaf_indices[block].leaf_indices[&key];
                (key, operation, leaf_index)
            })
            .collect();

        metrics::histogram!("merkle_tree.leaf_index_update", start.elapsed());
        let start = Instant::now();

        let prepared_updates = self.prepare_batch_update(storage_logs_with_indices)?;

        metrics::histogram!("merkle_tree.prepare_update", start.elapsed());

        let start = Instant::now();
        let updates = prepared_updates.calculate(self.hasher().clone())?;

        metrics::histogram!("merkle_tree.root_calculation", start.elapsed());

        let start = Instant::now();

        let tree_metadata = match self.mode {
            TreeMode::Full => {
                let patch_metadata =
                    self.apply_patch(updates, &storage_logs_with_blocks, &leaf_indices);

                self.root_hash = patch_metadata
                    .last()
                    .map(|metadata| metadata.root_hash.clone())
                    .unwrap_or_else(|| self.root_hash.clone());

                patch_metadata
                    .into_iter()
                    .zip(storage_logs_with_blocks)
                    .group_by(|(_, (block, _))| *block)
                    .into_iter()
                    .map(|(block, group)| {
                        let LeafIndices {
                            last_index,
                            initial_writes,
                            repeated_writes,
                            previous_index,
                            ..
                        } = std::mem::take(&mut leaf_indices[block]);

                        let metadata: Vec<_> =
                            group.into_iter().map(|(metadata, _)| metadata).collect();
                        let root_hash = metadata.last().unwrap().root_hash.clone();
                        let witness_input = bincode::serialize(&(metadata, previous_index))
                            .expect("witness serialization failed");

                        TreeMetadata {
                            root_hash,
                            rollup_last_leaf_index: last_index,
                            witness_input,
                            initial_writes,
                            repeated_writes,
                        }
                    })
                    .collect()
            }
            TreeMode::Lightweight => {
                self.root_hash = self.apply_patch_without_metadata_calculation(updates);

                let LeafIndices {
                    last_index,
                    initial_writes,
                    repeated_writes,
                    ..
                } = std::mem::take(&mut leaf_indices[0]);

                vec![TreeMetadata {
                    root_hash: self.root_hash.clone(),
                    rollup_last_leaf_index: last_index,
                    witness_input: Vec::new(),
                    initial_writes,
                    repeated_writes,
                }]
            }
        };

        metrics::histogram!("merkle_tree.patch_application", start.elapsed());

        self.block_number += total_blocks as u32;
        Ok(tree_metadata)
    }

    /// Prepares all the data which will be needed to calculate new Merkle Trees without storage access.
    /// This method doesn't perform any hashing operations.
    fn prepare_batch_update<I>(&self, storage_logs: I) -> Result<UpdatesBatch, TreeError>
    where
        I: IntoIterator<Item = (TreeKey, TreeOperation, u64)>,
    {
        let (op_idxs, updates): (Vec<_>, Vec<_>) = storage_logs
            .into_iter()
            .enumerate()
            .map(|(op_idx, (key, op, index))| ((op_idx, key), (key, op, index)))
            .unzip();

        let map = self
            .hash_paths_to_leaves(updates.into_iter())
            .zip(op_idxs.into_iter())
            .map(|(parent_nodes, (op_idx, key))| (key, Update::new(op_idx, parent_nodes, key)))
            .fold(HashMap::new(), |mut map: HashMap<_, Vec<_>>, (key, op)| {
                match map.entry(key) {
                    Entry::Occupied(mut entry) => entry.get_mut().push(op),
                    Entry::Vacant(entry) => {
                        entry.insert(vec![op]);
                    }
                }
                map
            });

        Ok(UpdatesBatch::new(map))
    }

    /// Accepts updated key-value pair and resolves to an iterator which produces
    /// new tree path containing leaf with branch nodes and full path to the top.
    /// This iterator will lazily emit leaf with needed path to the top node.
    /// At the moment of calling given function won't perform any hashing operation.
    /// Note: This method is public so that it can be used by the data availability repo.
    pub fn hash_paths_to_leaves<'a, 'b: 'a, I>(
        &'a self,
        storage_logs: I,
    ) -> impl Iterator<Item = Vec<Vec<u8>>> + 'a
    where
        I: Iterator<Item = (TreeKey, TreeOperation, u64)> + Clone + 'b,
    {
        let hasher = self.hasher().clone();
        let default_leaf = TreeConfig::empty_leaf(&hasher);

        self.get_leaves_paths(storage_logs.clone().map(|(key, _, _)| key))
            .zip(storage_logs)
            .map(move |(current_path, (_key, operation, leaf_index))| {
                let hash = match operation {
                    TreeOperation::Write { value, .. } => hasher.compress(
                        &serialize_leaf_index(leaf_index),
                        &value.to_fixed_bytes().to_vec(),
                    ),
                    TreeOperation::Delete => default_leaf.clone(),
                    TreeOperation::Read(value) => hasher.compress(
                        &serialize_leaf_index(leaf_index),
                        &value.to_fixed_bytes().to_vec(),
                    ),
                };
                current_path
                    .map(|(_, hash)| hash)
                    .chain(once(hash))
                    .collect()
            })
    }

    /// Retrieves leaf with a given key along with full tree path to it.
    /// Note: This method is public so that it can be used by the data availability repo.
    pub fn get_leaves_paths<'a, 'b: 'a, I>(
        &'a self,
        ids_iter: I,
    ) -> impl Iterator<Item = impl DoubleEndedIterator<Item = (TreeKey, Vec<u8>)> + Clone + 'b> + 'a
    where
        I: Iterator<Item = TreeKey> + Clone + 'a,
    {
        let empty_tree = Arc::new(self.config.empty_tree().to_vec());

        let idxs: HashSet<_> = ids_iter
            .clone()
            .flat_map(utils::idx_to_merkle_path)
            .collect();

        let branch_map: Arc<HashMap<_, _>> = Arc::new(
            idxs.iter()
                .cloned()
                .zip(self.storage.hashes(idxs.iter()).into_iter())
                .collect(),
        );

        let hash_by_lvl_idx = move |lvl_idx| {
            let value = branch_map
                .get(&lvl_idx)
                .and_then(|x| x.clone())
                .unwrap_or_else(|| empty_tree[lvl_idx.0 .0 as usize].hash().to_vec());

            (lvl_idx.0 .1, value)
        };

        ids_iter
            .into_iter()
            .map(move |idx| utils::idx_to_merkle_path(idx).map(hash_by_lvl_idx.clone()))
    }

    fn make_node(level: usize, key: TreeKey, node: NodeEntry) -> (LevelIndex, Vec<u8>) {
        (
            ((ROOT_TREE_DEPTH - level) as u16, key).into(),
            node.into_hash(),
        )
    }

    fn apply_patch_without_metadata_calculation(&mut self, patch: TreePatch) -> Vec<u8> {
        let branches = patch
            .into_iter()
            .fold(HashMap::new(), |mut branches, entries| {
                branches.extend(
                    entries
                        .into_iter()
                        .enumerate()
                        .map(|(level, (key, tree_value))| Self::make_node(level, key, tree_value)),
                );
                branches
            });

        let root_hash = branches[&(0, TreeKey::zero()).into()].clone();

        // Prepare database changes
        self.storage.pre_save(branches);
        root_hash
    }

    /// Applies each change from the given patch to the tree.
    fn apply_patch(
        &mut self,
        patch: TreePatch,
        storage_logs: &[(usize, (TreeKey, TreeOperation))],
        leaf_indices: &[LeafIndices],
    ) -> Vec<StorageLogMetadata> {
        let (branches, metadata) = patch.into_iter().zip(storage_logs).fold(
            (HashMap::new(), Vec::new()),
            |(mut branches, mut metadata), (entries, &(block, (_, storage_log)))| {
                let leaf_hashed_key = entries.first().unwrap().0;
                let leaf_index = leaf_indices[block].leaf_indices[&leaf_hashed_key];
                let mut merkle_paths = Vec::with_capacity(ROOT_TREE_DEPTH);

                branches.extend(entries.into_iter().enumerate().map(|(level, (key, node))| {
                    if let NodeEntry::Branch {
                        right_hash,
                        left_hash,
                        ..
                    } = &node
                    {
                        let witness_hash = if (leaf_hashed_key >> (level - 1)) % 2 == 0.into() {
                            right_hash
                        } else {
                            left_hash
                        };
                        merkle_paths.push(witness_hash.clone());
                    }
                    Self::make_node(level, key, node)
                }));

                let root_hash = branches.get(&(0, TreeKey::zero()).into()).unwrap().clone();
                let is_write = !matches!(storage_log, TreeOperation::Read(_));
                let first_write = is_write && leaf_index >= leaf_indices[block].previous_index;
                let value_written = match storage_log {
                    TreeOperation::Write { value, .. } => value,
                    _ => H256::zero(),
                };
                let value_read = match storage_log {
                    TreeOperation::Write { previous_value, .. } => previous_value,
                    TreeOperation::Read(value) => value,
                    TreeOperation::Delete => H256::zero(),
                };
                let metadata_log = StorageLogMetadata {
                    root_hash,
                    is_write,
                    first_write,
                    merkle_paths,
                    leaf_hashed_key,
                    leaf_enumeration_index: leaf_index,
                    value_written: value_written.to_fixed_bytes(),
                    value_read: value_read.to_fixed_bytes(),
                };
                metadata.push(metadata_log);

                (branches, metadata)
            },
        );

        // Prepare database changes
        self.storage.pre_save(branches);
        metadata
    }

    pub fn save(&mut self) -> Result<(), TreeError> {
        self.storage.save(self.block_number)
    }

    pub fn verify_consistency(&self) {
        let empty_tree = self.config.empty_tree().to_vec();
        let hasher = self.hasher().clone();

        let mut current_level =
            vec![(self.root_hash(), (1, 0.into()).into(), (1, 1.into()).into())];

        for node in empty_tree.iter().take(ROOT_TREE_DEPTH + 1).skip(1) {
            let default_hash = node.hash().to_vec();

            // fetch hashes for current level from rocksdb
            let hashes = {
                let nodes_iter = current_level
                    .iter()
                    .flat_map(|(_, left, right)| vec![left, right]);

                self.storage
                    .hashes(nodes_iter.clone())
                    .into_iter()
                    .map(|value| value.unwrap_or_else(|| default_hash.clone()))
                    .zip(nodes_iter)
                    .map(|(k, v)| (v.clone(), k))
                    .collect::<HashMap<_, _>>()
            };

            // verify in parallel that hashes do match with previous level
            // and create new data for next level
            current_level = current_level
                .into_par_iter()
                .map(|(parent_hash, left, right)| {
                    let mut children_checks = vec![];
                    let left_hash = hashes[&left].clone();
                    let right_hash = hashes[&right].clone();

                    assert_eq!(parent_hash, hasher.compress(&left_hash, &right_hash));
                    if left_hash != default_hash {
                        let (left_child, right_child) = children_idxs(&left);
                        children_checks.push((left_hash, left_child, right_child));
                    }
                    if right_hash != default_hash {
                        let (left_child, right_child) = children_idxs(&right);
                        children_checks.push((right_hash, left_child, right_child));
                    }
                    children_checks
                })
                .flatten()
                .collect();
        }
    }

    // while this function is used by the block reverter to revert to a previous state,
    // it actually applies "arbitrary logs" (without any context).
    // that is, it gets a list of (key,value) logs and applies them to the state
    pub fn revert_logs(
        &mut self,
        block_number: L1BatchNumber,
        logs: Vec<(TreeKey, Option<TreeValue>)>,
    ) {
        let tree_operations = logs
            .into_iter()
            .map(|(key, value)| {
                let operation = match value {
                    Some(value) => TreeOperation::Write {
                        value,
                        previous_value: H256::zero(),
                    },
                    None => TreeOperation::Delete,
                };
                (key, operation)
            })
            .collect();
        self.apply_updates_batch(vec![tree_operations])
            .expect("Failed to revert logs");
        self.block_number = block_number.0 + 1;
    }

    /// Resets state of the tree to the latest state in db
    pub fn reset(&mut self) {
        let (root_hash, block_number) = self.storage.fetch_metadata();
        self.root_hash =
            root_hash.unwrap_or_else(|| TreeConfig::new(ZkHasher::default()).default_root_hash());
        self.block_number = block_number;
        self.storage.pre_save(HashMap::new());
    }
}
