//! Producer of Airbender read proofs: Merkle proofs, against the previous batch root, for the
//! declared keys of a batch that `merkle_paths` does not cover.
//!
//! The guest serves only proven slots, and `merkle_paths` omits a fully rolled-back write (priced
//! from its pre-state, observable through the pubdata counter), a no-op write (the value written
//! equals the old one), and a slot the host reads outside the VM (the `KnownCodesStorage` marker
//! behind bytecode compression). Proofs are fetched and verified ahead of time, so the request
//! handler only loads a blob.

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
    time::Duration,
};

use tokio::{sync::watch, time::Instant};
use zksync_airbender_prover_interface::inputs::{AirbenderReadProofs, StorageReadProof};
use zksync_crypto_primitives::hasher::blake2::Blake2Hasher;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_merkle_tree::TreeEntry;
use zksync_object_store::{ObjectStore, ObjectStoreError};
use zksync_prover_interface::inputs::{VMRunWitnessInputData, WitnessInputMerklePaths};
use zksync_shared_resources::tree::{TreeApiClient, TreeApiError, TreeEntryWithProof};
use zksync_types::{L1BatchNumber, H256, U256};

use crate::metrics::{ReadProofsErrorKind, METRICS};

/// Keys per tree API request, to keep a single response bounded.
const TREE_API_CHUNK_SIZE: usize = 1_000;
/// Caps a failing batch's backoff at `poll_interval * 2^6`.
const MAX_BACKOFF_SHIFT: u32 = 6;
/// Depth of the state tree, so the longest valid Merkle path.
const TREE_DEPTH: usize = 256;

#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadProofsError {
    /// The tree has not reached the required version yet, or is not up; retry later.
    #[error("tree API not ready: {0:#}")]
    Transient(anyhow::Error),
    #[error("{kind:?}: {source:#}")]
    Fatal {
        kind: ReadProofsErrorKind,
        source: anyhow::Error,
    },
}

impl ReadProofsError {
    fn fatal(kind: ReadProofsErrorKind, source: impl Into<anyhow::Error>) -> Self {
        Self::Fatal {
            kind,
            source: source.into(),
        }
    }
}

/// What the witness claims about a declared key's pre-state, used to cross-check each proof.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct DeclaredPrestate {
    /// From `read_storage_key`: the value the sequencer read.
    value: Option<H256>,
    /// From `is_write_initial`: whether the slot had no leaf.
    is_initial: Option<bool>,
}

/// Declared keys (`read_storage_key ∪ is_write_initial`, as `hashed_key_u256()`) with no entry in
/// `merkle_paths`, each with what the witness says about it. Sorted by key: the guest requires
/// read proofs in strictly ascending key order.
fn gap_keys(
    vm_run_data: &VMRunWitnessInputData,
    merkle_paths: &WitnessInputMerklePaths,
) -> Vec<(U256, DeclaredPrestate)> {
    let state = &vm_run_data.witness_block_state;
    let mut declared: HashMap<U256, DeclaredPrestate> = HashMap::new();
    for (key, value) in &state.read_storage_key {
        declared.entry(key.hashed_key_u256()).or_default().value = Some(*value);
    }
    for (key, is_initial) in &state.is_write_initial {
        declared
            .entry(key.hashed_key_u256())
            .or_default()
            .is_initial = Some(*is_initial);
    }
    let covered: BTreeSet<U256> = merkle_paths
        .merkle_paths
        .iter()
        .map(|log| log.leaf_hashed_key)
        .collect();
    let mut gaps: Vec<_> = declared
        .into_iter()
        .filter(|(key, _)| !covered.contains(key))
        .collect();
    gaps.sort_unstable_by_key(|(key, _)| *key);
    gaps
}

/// Checks a tree API entry against `old_root` and the witness, and converts it to the wire form.
fn verified_read_proof(
    key: U256,
    declared: DeclaredPrestate,
    entry: TreeEntryWithProof,
    old_root: H256,
) -> Result<StorageReadProof, ReadProofsError> {
    // `verify` panics on a path longer than the tree.
    if entry.merkle_path.len() > TREE_DEPTH {
        return Err(ReadProofsError::fatal(
            ReadProofsErrorKind::Verification,
            anyhow::anyhow!(
                "key {key:#x}: Merkle path has {} hashes, tree depth is {TREE_DEPTH}",
                entry.merkle_path.len()
            ),
        ));
    }
    // The tree API enumerates the path root-to-leaf; the guest folds it leaf-to-root.
    let mut merkle_path = entry.merkle_path;
    merkle_path.reverse();
    let proof = zksync_merkle_tree::TreeEntryWithProof {
        base: TreeEntry::new(key, entry.index, entry.value),
        merkle_path,
    };
    proof
        .verify(&Blake2Hasher, old_root)
        .map_err(|err| ReadProofsError::fatal(ReadProofsErrorKind::Verification, err))?;

    if let Some(value) = declared.value {
        if value != entry.value {
            return Err(ReadProofsError::fatal(
                ReadProofsErrorKind::WitnessMismatch,
                anyhow::anyhow!(
                    "key {key:#x}: witness read {value:?}, tree holds {:?}",
                    entry.value
                ),
            ));
        }
    }
    if let Some(is_initial) = declared.is_initial {
        if is_initial != (entry.index == 0) {
            return Err(ReadProofsError::fatal(
                ReadProofsErrorKind::WitnessMismatch,
                anyhow::anyhow!(
                    "key {key:#x}: witness says initial = {is_initial}, tree leaf index is {}",
                    entry.index
                ),
            ));
        }
    }

    Ok(StorageReadProof {
        leaf_hashed_key: key,
        leaf_enumeration_index: entry.index,
        value: entry.value.0,
        merkle_path: proof.merkle_path.iter().map(|hash| hash.0).collect(),
    })
}

fn map_tree_error(err: TreeApiError) -> ReadProofsError {
    match err {
        TreeApiError::NotReady(source) => ReadProofsError::Transient(
            source.unwrap_or_else(|| anyhow::anyhow!("tree is not initialized")),
        ),
        // Not built yet; `missing_version < version_count` means pruned, which will not recover.
        TreeApiError::NoVersion {
            missing_version,
            version_count,
        } if missing_version >= version_count => ReadProofsError::Transient(anyhow::anyhow!(
            "tree version {missing_version} not built yet ({version_count} versions)"
        )),
        err @ TreeApiError::NoVersion { .. } => {
            ReadProofsError::fatal(ReadProofsErrorKind::TreeApi, err)
        }
        // A failed request (5xx, bad response) says nothing about the batch.
        err => ReadProofsError::fatal(ReadProofsErrorKind::TreeApiRequest, err),
    }
}

/// Only a missing or undecodable blob is about the batch; anything else hits every batch alike.
fn map_store_error(err: ObjectStoreError) -> ReadProofsError {
    let kind = match &err {
        ObjectStoreError::KeyNotFound(_) | ObjectStoreError::Serialization(_) => {
            ReadProofsErrorKind::BatchBlob
        }
        _ => ReadProofsErrorKind::ObjectStore,
    };
    ReadProofsError::fatal(kind, err)
}

/// Proofs for every gap key of `batch`, verified against `old_root` (the root after `batch - 1`).
async fn read_proofs_for_batch(
    tree_api: &dyn TreeApiClient,
    batch: L1BatchNumber,
    vm_run_data: &VMRunWitnessInputData,
    merkle_paths: &WitnessInputMerklePaths,
    old_root: H256,
) -> Result<Vec<StorageReadProof>, ReadProofsError> {
    let gaps = gap_keys(vm_run_data, merkle_paths);
    let prev_batch = L1BatchNumber(batch.0 - 1);
    let mut proofs = Vec::with_capacity(gaps.len());
    for chunk in gaps.chunks(TREE_API_CHUNK_SIZE) {
        let keys = chunk.iter().map(|(key, _)| *key).collect();
        let entries = tree_api
            .get_proofs(prev_batch, keys)
            .await
            .map_err(map_tree_error)?;
        if entries.len() != chunk.len() {
            return Err(ReadProofsError::fatal(
                ReadProofsErrorKind::TreeApi,
                anyhow::anyhow!(
                    "tree API returned {} proofs for {} keys",
                    entries.len(),
                    chunk.len()
                ),
            ));
        }
        for ((key, declared), entry) in chunk.iter().zip(entries) {
            proofs.push(verified_read_proof(*key, *declared, entry, old_root)?);
        }
    }
    Ok(proofs)
}

/// A batch whose last attempt failed. It is skipped until `retry_at`, so it cannot hold up the
/// batches after it.
#[derive(Debug)]
struct FailedBatch {
    failures: u32,
    retry_at: Instant,
}

#[derive(Debug)]
pub struct AirbenderReadProofsProducer {
    pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    tree_api: Arc<dyn TreeApiClient>,
    first_processed_batch: L1BatchNumber,
    max_proving_attempts: u32,
    poll_interval: Duration,
    failed: HashMap<L1BatchNumber, FailedBatch>,
}

impl AirbenderReadProofsProducer {
    pub(crate) fn new(
        pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        tree_api: Arc<dyn TreeApiClient>,
        first_processed_batch: L1BatchNumber,
        max_proving_attempts: u32,
        poll_interval: Duration,
    ) -> Self {
        Self {
            pool,
            blob_store,
            tree_api,
            first_processed_batch,
            max_proving_attempts,
            poll_interval,
            failed: HashMap::new(),
        }
    }

    pub(crate) async fn run(
        mut self,
        mut stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        while !*stop_receiver.borrow() {
            let delay = match self.produce_next().await {
                Ok(Some(_)) => continue,
                Ok(None) => self.poll_interval,
                Err(ReadProofsError::Transient(err)) => {
                    tracing::info!("Airbender read proofs waiting on the tree: {err:#}");
                    self.poll_interval
                }
                Err(ReadProofsError::Fatal { kind, source }) => {
                    METRICS.airbender_read_proofs_errors[&kind].inc();
                    tracing::error!(
                        "Failed to produce Airbender read proofs ({kind:?}): {source:#}"
                    );
                    if kind.is_batch_specific() {
                        // That batch is backed off; the next one may well succeed.
                        continue;
                    }
                    self.poll_interval
                }
            };
            tokio::time::timeout(delay, stop_receiver.changed())
                .await
                .ok();
        }
        tracing::info!("Stop request received, Airbender read proofs producer is shutting down");
        Ok(())
    }

    /// Produces and stores the read proofs of the next batch that needs them, if any. A batch that
    /// fails for a batch-specific reason is backed off; its URL stays unset, so the lock never
    /// hands it out meanwhile.
    pub(crate) async fn produce_next(&mut self) -> Result<Option<L1BatchNumber>, ReadProofsError> {
        let now = Instant::now();
        let skipped: Vec<_> = self
            .failed
            .iter()
            .filter(|(_, failed)| failed.retry_at > now)
            .map(|(batch, _)| *batch)
            .collect();
        let Some(batch) = self
            .pool
            .connection_tagged("airbender_read_proofs_producer")
            .await
            .map_err(dal_err)?
            .proof_generation_dal()
            .get_next_batch_without_airbender_read_proofs(
                self.first_processed_batch,
                self.max_proving_attempts,
                &skipped,
            )
            .await
            .map_err(dal_err)?
        else {
            return Ok(None);
        };

        match self.produce(batch).await {
            Ok(()) => {
                self.failed.remove(&batch);
                Ok(Some(batch))
            }
            Err(err) => {
                if matches!(&err, ReadProofsError::Fatal { kind, .. } if kind.is_batch_specific()) {
                    let failed = self.failed.entry(batch).or_insert(FailedBatch {
                        failures: 0,
                        retry_at: now,
                    });
                    failed.failures += 1;
                    let backoff = self
                        .poll_interval
                        .saturating_mul(1 << failed.failures.min(MAX_BACKOFF_SHIFT));
                    // From the failure, so a slow attempt cannot use up the backoff.
                    failed.retry_at = Instant::now() + backoff;
                }
                Err(err)
            }
        }
    }

    async fn produce(&self, batch: L1BatchNumber) -> Result<(), ReadProofsError> {
        let mut connection = self
            .pool
            .connection_tagged("airbender_read_proofs_producer")
            .await
            .map_err(dal_err)?;
        let prev_batch = L1BatchNumber(batch.0 - 1);
        let old_root = connection
            .blocks_dal()
            .get_l1_batch_state_root(prev_batch)
            .await
            .map_err(dal_err)?
            .ok_or_else(|| {
                ReadProofsError::fatal(
                    ReadProofsErrorKind::MissingStateRoot,
                    anyhow::anyhow!("no state root for batch {prev_batch}"),
                )
            })?;
        // Not held across the object store and tree calls below.
        drop(connection);

        let vm_run_data: VMRunWitnessInputData =
            self.blob_store.get(batch).await.map_err(map_store_error)?;
        let merkle_paths: WitnessInputMerklePaths =
            self.blob_store.get(batch).await.map_err(map_store_error)?;
        let proofs = read_proofs_for_batch(
            self.tree_api.as_ref(),
            batch,
            &vm_run_data,
            &merkle_paths,
            old_root,
        )
        .await?;
        let proof_count = proofs.len();

        // Stored even when empty: the lock waits for the URL, not for proofs.
        let blob_url = self
            .blob_store
            .put(batch, &AirbenderReadProofs { proofs })
            .await
            .map_err(map_store_error)?;
        self.pool
            .connection_tagged("airbender_read_proofs_producer")
            .await
            .map_err(dal_err)?
            .proof_generation_dal()
            .save_airbender_read_proofs_artifacts_metadata(batch, &blob_url)
            .await
            .map_err(dal_err)?;

        METRICS
            .airbender_read_proofs_stored
            .inc_by(proof_count as u64);
        tracing::info!("Stored {proof_count} Airbender read proofs for batch {batch}");
        Ok(())
    }
}

fn dal_err(err: impl Into<anyhow::Error>) -> ReadProofsError {
    ReadProofsError::fatal(ReadProofsErrorKind::Dal, err)
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    };

    use async_trait::async_trait;
    use zksync_dal::Connection;
    use zksync_merkle_tree::{MerkleTree, PatchSet};
    use zksync_object_store::MockObjectStore;
    use zksync_prover_interface::inputs::StorageLogMetadata;
    use zksync_shared_resources::tree::MerkleTreeInfo;
    use zksync_types::{
        block::L1BatchHeader, settlement::SettlementLayer, AccountTreeId, Address, ProtocolVersion,
        ProtocolVersionId, StorageKey,
    };

    use super::*;

    /// In-memory tree behind the real API's conventions: positional results, root-to-leaf paths.
    #[derive(Debug)]
    struct TestTreeApi {
        tree: Mutex<MerkleTree<PatchSet>>,
        ready: AtomicBool,
    }

    impl TestTreeApi {
        fn new(tree: MerkleTree<PatchSet>) -> Self {
            Self {
                tree: Mutex::new(tree),
                ready: AtomicBool::new(true),
            }
        }
    }

    #[async_trait]
    impl TreeApiClient for TestTreeApi {
        async fn get_info(&self) -> Result<MerkleTreeInfo, TreeApiError> {
            unimplemented!("not used by the producer")
        }

        async fn get_proofs(
            &self,
            l1_batch_number: L1BatchNumber,
            hashed_keys: Vec<U256>,
        ) -> Result<Vec<TreeEntryWithProof>, TreeApiError> {
            if !self.ready.load(Ordering::SeqCst) {
                return Err(TreeApiError::NotReady(None));
            }
            let entries = self
                .tree
                .lock()
                .unwrap()
                .entries_with_proofs(l1_batch_number.0.into(), &hashed_keys)
                .map_err(|err| TreeApiError::NoVersion {
                    missing_version: err.missing_version,
                    version_count: err.version_count,
                })?;
            Ok(entries
                .into_iter()
                .map(|entry| {
                    let mut merkle_path = entry.merkle_path;
                    merkle_path.reverse();
                    TreeEntryWithProof {
                        value: entry.base.value,
                        index: entry.base.leaf_index,
                        merkle_path,
                    }
                })
                .collect())
        }
    }

    fn storage_key(n: u64) -> StorageKey {
        StorageKey::new(
            AccountTreeId::new(Address::repeat_byte(0x42)),
            H256::from_low_u64_be(n),
        )
    }

    const EXISTING: u64 = 1;
    const ZERO_VALUED: u64 = 2;
    const MISSING: u64 = 3;
    const COVERED: u64 = 4;
    const EXISTING_VALUE: H256 = H256::repeat_byte(0x11);

    /// Tree versions 0 and 1 (so batch 2 proves against version 1), with `EXISTING`,
    /// `ZERO_VALUED` (an existing leaf holding zero) and `COVERED` populated, and 64 fillers.
    fn test_tree() -> MerkleTree<PatchSet> {
        let mut tree = MerkleTree::new(PatchSet::default()).unwrap();
        let fillers = (100..164)
            .map(|n| {
                TreeEntry::new(
                    storage_key(n).hashed_key_u256(),
                    n - 99,
                    H256::repeat_byte(7),
                )
            })
            .collect();
        tree.extend(fillers).unwrap();
        tree.extend(vec![
            TreeEntry::new(storage_key(EXISTING).hashed_key_u256(), 65, EXISTING_VALUE),
            TreeEntry::new(storage_key(ZERO_VALUED).hashed_key_u256(), 66, H256::zero()),
            TreeEntry::new(
                storage_key(COVERED).hashed_key_u256(),
                67,
                H256::repeat_byte(9),
            ),
        ])
        .unwrap();
        tree
    }

    /// A witness declaring every test key, with only `COVERED` in `merkle_paths`.
    fn witness() -> (VMRunWitnessInputData, WitnessInputMerklePaths) {
        let mut vm_run_data = VMRunWitnessInputData {
            l1_batch_number: L1BatchNumber(2),
            used_bytecodes: Default::default(),
            initial_heap_content: vec![],
            protocol_version: Default::default(),
            bootloader_code: vec![],
            default_account_code_hash: Default::default(),
            evm_emulator_code_hash: None,
            storage_refunds: vec![],
            pubdata_costs: vec![],
            witness_block_state: Default::default(),
        };
        let state = &mut vm_run_data.witness_block_state;
        state
            .read_storage_key
            .insert(storage_key(EXISTING), EXISTING_VALUE);
        state
            .read_storage_key
            .insert(storage_key(COVERED), H256::repeat_byte(9));
        // A rolled-back write, declared only through its initialness.
        state
            .is_write_initial
            .insert(storage_key(ZERO_VALUED), false);
        state.is_write_initial.insert(storage_key(MISSING), true);

        let mut merkle_paths = WitnessInputMerklePaths::new(68);
        merkle_paths.push_merkle_path(StorageLogMetadata {
            root_hash: [0; 32],
            is_write: false,
            first_write: false,
            merkle_paths: vec![],
            leaf_hashed_key: storage_key(COVERED).hashed_key_u256(),
            leaf_enumeration_index: 67,
            value_written: [0; 32],
            value_read: [9; 32],
        });
        (vm_run_data, merkle_paths)
    }

    #[test]
    fn gap_keys_are_declared_minus_merkle_paths() {
        let (vm_run_data, merkle_paths) = witness();
        let gaps = gap_keys(&vm_run_data, &merkle_paths);
        let mut expected: Vec<_> = [EXISTING, ZERO_VALUED, MISSING]
            .map(|n| storage_key(n).hashed_key_u256())
            .into();
        expected.sort_unstable();
        assert_eq!(
            gaps.iter().map(|(key, _)| *key).collect::<Vec<_>>(),
            expected
        );

        let declared: HashMap<_, _> = gaps.into_iter().collect();
        assert_eq!(
            declared[&storage_key(EXISTING).hashed_key_u256()],
            DeclaredPrestate {
                value: Some(EXISTING_VALUE),
                is_initial: None
            }
        );
        assert_eq!(
            declared[&storage_key(MISSING).hashed_key_u256()],
            DeclaredPrestate {
                value: None,
                is_initial: Some(true)
            }
        );
    }

    #[tokio::test]
    async fn proofs_for_existing_and_missing_keys_verify_against_old_root() {
        let tree = test_tree();
        let old_root = tree.latest_root_hash();
        let api = TestTreeApi::new(tree);
        let (vm_run_data, merkle_paths) = witness();

        let proofs = read_proofs_for_batch(
            &api,
            L1BatchNumber(2),
            &vm_run_data,
            &merkle_paths,
            old_root,
        )
        .await
        .unwrap();
        assert_eq!(proofs.len(), 3);

        let by_key: HashMap<_, _> = proofs.iter().map(|p| (p.leaf_hashed_key, p)).collect();
        let existing = by_key[&storage_key(EXISTING).hashed_key_u256()];
        assert_eq!(
            (existing.leaf_enumeration_index, existing.value),
            (65, EXISTING_VALUE.0)
        );
        let zero = by_key[&storage_key(ZERO_VALUED).hashed_key_u256()];
        assert_eq!((zero.leaf_enumeration_index, zero.value), (66, [0; 32]));
        let missing = by_key[&storage_key(MISSING).hashed_key_u256()];
        assert_eq!(
            (missing.leaf_enumeration_index, missing.value),
            (0, [0; 32])
        );

        // The stored form is leaf-to-root, exactly what the guest folds.
        for proof in &proofs {
            let entry = zksync_merkle_tree::TreeEntryWithProof {
                base: TreeEntry::new(
                    proof.leaf_hashed_key,
                    proof.leaf_enumeration_index,
                    H256(proof.value),
                ),
                merkle_path: proof.merkle_path.iter().copied().map(H256).collect(),
            };
            entry.verify(&Blake2Hasher, old_root).unwrap();
        }
    }

    #[tokio::test]
    async fn wrong_path_direction_is_rejected() {
        let tree = test_tree();
        let old_root = tree.latest_root_hash();
        let key = storage_key(EXISTING).hashed_key_u256();
        let mut entry = TestTreeApi::new(tree)
            .get_proofs(L1BatchNumber(1), vec![key])
            .await
            .unwrap()
            .pop()
            .unwrap();
        assert!(
            entry.merkle_path.len() > 1,
            "the test needs a non-trivial path"
        );
        entry.merkle_path.reverse();
        let err =
            verified_read_proof(key, DeclaredPrestate::default(), entry, old_root).unwrap_err();
        assert_matches_kind(err, ReadProofsErrorKind::Verification);
    }

    #[test]
    fn overlong_path_is_rejected() {
        let entry = TreeEntryWithProof {
            value: H256::zero(),
            index: 0,
            merkle_path: vec![H256::zero(); TREE_DEPTH + 1],
        };
        let err = verified_read_proof(
            U256::zero(),
            DeclaredPrestate::default(),
            entry,
            H256::zero(),
        )
        .unwrap_err();
        assert_matches_kind(err, ReadProofsErrorKind::Verification);
    }

    fn assert_matches_kind(err: ReadProofsError, expected: ReadProofsErrorKind) {
        match err {
            ReadProofsError::Fatal { kind, .. } => assert_eq!(kind, expected),
            err => panic!("expected a fatal {expected:?} error, got {err:?}"),
        }
    }

    #[tokio::test]
    async fn value_mismatch_with_witness_is_rejected() {
        let tree = test_tree();
        let old_root = tree.latest_root_hash();
        let api = TestTreeApi::new(tree);
        let (mut vm_run_data, merkle_paths) = witness();
        vm_run_data
            .witness_block_state
            .read_storage_key
            .insert(storage_key(EXISTING), H256::repeat_byte(0x12));

        let err = read_proofs_for_batch(
            &api,
            L1BatchNumber(2),
            &vm_run_data,
            &merkle_paths,
            old_root,
        )
        .await
        .unwrap_err();
        assert_matches_kind(err, ReadProofsErrorKind::WitnessMismatch);
    }

    #[tokio::test]
    async fn initialness_mismatch_is_rejected() {
        let tree = test_tree();
        let old_root = tree.latest_root_hash();
        let api = TestTreeApi::new(tree);
        let (mut vm_run_data, merkle_paths) = witness();
        // An existing leaf holding zero is not an initial write.
        vm_run_data
            .witness_block_state
            .is_write_initial
            .insert(storage_key(ZERO_VALUED), true);

        let err = read_proofs_for_batch(
            &api,
            L1BatchNumber(2),
            &vm_run_data,
            &merkle_paths,
            old_root,
        )
        .await
        .unwrap_err();
        assert_matches_kind(err, ReadProofsErrorKind::WitnessMismatch);
    }

    #[tokio::test]
    async fn wrong_old_root_is_rejected() {
        let api = TestTreeApi::new(test_tree());
        let (vm_run_data, merkle_paths) = witness();
        let err = read_proofs_for_batch(
            &api,
            L1BatchNumber(2),
            &vm_run_data,
            &merkle_paths,
            H256::repeat_byte(1),
        )
        .await
        .unwrap_err();
        assert_matches_kind(err, ReadProofsErrorKind::Verification);
    }

    #[test]
    fn tree_errors_are_classified() {
        let transient = |err| matches!(map_tree_error(err), ReadProofsError::Transient(_));
        assert!(transient(TreeApiError::NotReady(None)));
        assert!(transient(TreeApiError::NoVersion {
            missing_version: 5,
            version_count: 5
        }));
        assert!(transient(TreeApiError::NoVersion {
            missing_version: 9,
            version_count: 5
        }));
        // Pruned: will not come back.
        assert!(!transient(TreeApiError::NoVersion {
            missing_version: 2,
            version_count: 5
        }));
        assert_matches_kind(
            map_tree_error(TreeApiError::NoVersion {
                missing_version: 2,
                version_count: 5,
            }),
            ReadProofsErrorKind::TreeApi,
        );
        // Every batch fails alike while the request fails.
        assert_matches_kind(
            map_tree_error(TreeApiError::Internal(anyhow::anyhow!("boom"))),
            ReadProofsErrorKind::TreeApiRequest,
        );
        assert!(!ReadProofsErrorKind::TreeApiRequest.is_batch_specific());
    }

    #[test]
    fn store_errors_are_classified() {
        assert_matches_kind(
            map_store_error(ObjectStoreError::KeyNotFound("missing".into())),
            ReadProofsErrorKind::BatchBlob,
        );
        assert_matches_kind(
            map_store_error(ObjectStoreError::Serialization("garbage".into())),
            ReadProofsErrorKind::BatchBlob,
        );
        // Not retriable, yet not about the batch.
        let auth = ObjectStoreError::Other {
            source: "forbidden".into(),
            is_retriable: false,
        };
        assert_matches_kind(map_store_error(auth), ReadProofsErrorKind::ObjectStore);
        assert!(!ReadProofsErrorKind::ObjectStore.is_batch_specific());
    }

    // The tests below drive the whole producer and need the test Postgres.

    async fn seed_batches(pool: &ConnectionPool<Core>, old_root: H256) {
        let mut conn = pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        insert_batch(&mut conn, L1BatchNumber(1)).await;
        drop(conn);
        seed_batch(pool, L1BatchNumber(2), old_root).await;
    }

    async fn insert_batch(conn: &mut Connection<'_, Core>, number: L1BatchNumber) {
        let header = L1BatchHeader::new(
            number,
            100,
            Default::default(),
            ProtocolVersionId::latest(),
            SettlementLayer::for_tests(),
        );
        conn.blocks_dal()
            .insert_mock_l1_batch(&header)
            .await
            .unwrap();
    }

    /// Adds batch `number` with its witness blob URLs, after a batch whose root is `old_root`.
    async fn seed_batch(pool: &ConnectionPool<Core>, number: L1BatchNumber, old_root: H256) {
        let mut conn = pool.connection().await.unwrap();
        insert_batch(&mut conn, number).await;
        conn.blocks_dal()
            .save_l1_batch_tree_data(
                number - 1,
                &zksync_types::block::L1BatchTreeData {
                    hash: old_root,
                    rollup_last_leaf_index: 67,
                },
            )
            .await
            .unwrap();
        let dal = &mut conn.proof_generation_dal();
        dal.insert_proof_generation_details(number).await.unwrap();
        dal.save_vm_runner_artifacts_metadata(number, "vm_run")
            .await
            .unwrap();
        dal.save_merkle_paths_artifacts_metadata(number, "merkle_paths")
            .await
            .unwrap();
    }

    async fn producer_with(
        vm_run_data: VMRunWitnessInputData,
        merkle_paths: WitnessInputMerklePaths,
        api: TestTreeApi,
    ) -> (
        AirbenderReadProofsProducer,
        ConnectionPool<Core>,
        Arc<dyn ObjectStore>,
    ) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let old_root = api.tree.lock().unwrap().latest_root_hash();
        seed_batches(&pool, old_root).await;
        let store = MockObjectStore::arc();
        store.put(L1BatchNumber(2), &vm_run_data).await.unwrap();
        store.put(L1BatchNumber(2), &merkle_paths).await.unwrap();
        let producer = AirbenderReadProofsProducer::new(
            pool.clone(),
            store.clone(),
            Arc::new(api),
            L1BatchNumber(0),
            10,
            Duration::from_millis(10),
        );
        (producer, pool, store)
    }

    async fn next_without_read_proofs(pool: &ConnectionPool<Core>) -> Option<L1BatchNumber> {
        pool.connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .get_next_batch_without_airbender_read_proofs(L1BatchNumber(0), 10, &[])
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn producer_stores_blob_and_sets_url() {
        let (vm_run_data, merkle_paths) = witness();
        let (mut producer, pool, store) =
            producer_with(vm_run_data, merkle_paths, TestTreeApi::new(test_tree())).await;

        assert_eq!(
            producer.produce_next().await.unwrap(),
            Some(L1BatchNumber(2))
        );
        let blob: AirbenderReadProofs = store.get(L1BatchNumber(2)).await.unwrap();
        assert_eq!(blob.proofs.len(), 3);
        assert!(blob
            .proofs
            .windows(2)
            .all(|pair| pair[0].leaf_hashed_key < pair[1].leaf_hashed_key));
        assert_eq!(next_without_read_proofs(&pool).await, None);
        assert_eq!(producer.produce_next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn empty_gap_still_stores_blob() {
        let (mut vm_run_data, merkle_paths) = witness();
        let state = &mut vm_run_data.witness_block_state;
        state
            .read_storage_key
            .retain(|key, _| *key == storage_key(COVERED));
        state.is_write_initial.clear();
        let (mut producer, pool, store) =
            producer_with(vm_run_data, merkle_paths, TestTreeApi::new(test_tree())).await;

        assert_eq!(
            producer.produce_next().await.unwrap(),
            Some(L1BatchNumber(2))
        );
        let blob: AirbenderReadProofs = store.get(L1BatchNumber(2)).await.unwrap();
        assert!(blob.proofs.is_empty());
        assert_eq!(next_without_read_proofs(&pool).await, None);
    }

    #[tokio::test]
    async fn tree_not_ready_is_retried_without_storing() {
        let (vm_run_data, merkle_paths) = witness();
        let api = TestTreeApi::new(test_tree());
        api.ready.store(false, Ordering::SeqCst);
        let (mut producer, pool, store) = producer_with(vm_run_data, merkle_paths, api).await;

        let err = producer.produce_next().await.unwrap_err();
        assert!(matches!(err, ReadProofsError::Transient(_)), "{err:?}");
        assert!(store
            .get::<AirbenderReadProofs>(L1BatchNumber(2))
            .await
            .is_err());
        assert_eq!(
            next_without_read_proofs(&pool).await,
            Some(L1BatchNumber(2))
        );
    }

    /// Batches 2 and 3, with witness blobs stored only for batch 3.
    async fn producer_with_failing_batch(
        poll_interval: Duration,
    ) -> (
        AirbenderReadProofsProducer,
        ConnectionPool<Core>,
        Arc<dyn ObjectStore>,
    ) {
        let mut tree = test_tree();
        // Version 2 for batch 3 to prove against; same root as version 1.
        tree.extend(vec![]).unwrap();
        let old_root = tree.latest_root_hash();
        let pool = ConnectionPool::<Core>::test_pool().await;
        seed_batches(&pool, old_root).await;
        seed_batch(&pool, L1BatchNumber(3), old_root).await;
        let store = MockObjectStore::arc();
        let (vm_run_data, merkle_paths) = witness();
        store.put(L1BatchNumber(3), &vm_run_data).await.unwrap();
        store.put(L1BatchNumber(3), &merkle_paths).await.unwrap();
        let producer = AirbenderReadProofsProducer::new(
            pool.clone(),
            store.clone(),
            Arc::new(TestTreeApi::new(tree)),
            L1BatchNumber(0),
            10,
            poll_interval,
        );
        (producer, pool, store)
    }

    async fn has_read_proofs(pool: &ConnectionPool<Core>, batch: L1BatchNumber) -> bool {
        pool.connection()
            .await
            .unwrap()
            .proof_generation_dal()
            .is_batch_present_for_airbender_proof_inputs(batch, L1BatchNumber(0))
            .await
            .unwrap()
    }

    async fn wait_for_read_proofs(pool: &ConnectionPool<Core>, batch: L1BatchNumber) {
        let wait = async {
            while !has_read_proofs(pool, batch).await {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .unwrap_or_else(|_| panic!("no read proofs for batch {batch}"));
    }

    #[tokio::test]
    async fn failed_batch_is_backed_off() {
        let (mut producer, pool, _) = producer_with_failing_batch(Duration::from_secs(60)).await;

        let err = producer.produce_next().await.unwrap_err();
        assert!(
            matches!(
                err,
                ReadProofsError::Fatal {
                    kind: ReadProofsErrorKind::BatchBlob,
                    ..
                }
            ),
            "{err:?}"
        );
        assert_eq!(
            producer.produce_next().await.unwrap(),
            Some(L1BatchNumber(3))
        );
        assert_eq!(producer.produce_next().await.unwrap(), None);
        assert_eq!(
            next_without_read_proofs(&pool).await,
            Some(L1BatchNumber(2))
        );
    }

    #[tokio::test]
    async fn run_moves_past_failed_batch_and_retries_it() {
        let (producer, pool, store) = producer_with_failing_batch(Duration::from_millis(10)).await;
        let (stop_sender, stop_receiver) = watch::channel(false);
        let task = tokio::spawn(producer.run(stop_receiver));

        wait_for_read_proofs(&pool, L1BatchNumber(3)).await;
        assert!(!has_read_proofs(&pool, L1BatchNumber(2)).await);
        let (vm_run_data, merkle_paths) = witness();
        store.put(L1BatchNumber(2), &vm_run_data).await.unwrap();
        store.put(L1BatchNumber(2), &merkle_paths).await.unwrap();
        // Picked up again once its backoff expires.
        wait_for_read_proofs(&pool, L1BatchNumber(2)).await;

        stop_sender.send_replace(true);
        tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .expect("producer did not stop")
            .unwrap()
            .unwrap();
    }
}
