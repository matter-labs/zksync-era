//! Tests for the reorg detector component.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_dal::{Connection, CoreDal};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{create_l1_batch, create_l2_block};
use zksync_types::{
    block::{L2BlockHasher, L2BlockHeader},
    ProtocolVersion,
};
use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

use super::*;

#[tokio::test]
async fn test_binary_search() {
    for divergence_point in [1, 50, 51, 100] {
        let mut f = |x| async move { Ok::<_, ()>(x < divergence_point) };
        let result = binary_search_with(0, 100, &mut f).await;
        assert_eq!(result, Ok(divergence_point - 1));
    }
}

async fn store_l2_block(storage: &mut Connection<'_, Core>, number: u32, hash: H256) {
    let header = L2BlockHeader {
        hash,
        ..create_l2_block(number)
    };
    storage.blocks_dal().insert_l2_block(&header).await.unwrap();
}

async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: u32, hash: H256) {
    let header = create_l1_batch(number);
    storage
        .blocks_dal()
        .insert_mock_l1_batch(&header)
        .await
        .unwrap();
    storage
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(L1BatchNumber(number))
        .await
        .unwrap();
    storage
        .blocks_dal()
        .set_l1_batch_hash(L1BatchNumber(number), hash)
        .await
        .unwrap();
}

/// Tests the binary search algorithm.
#[tokio::test]
async fn binary_search_with_simple_predicate() {
    for divergence_point in [1, 50, 51, 100] {
        let mut f = |x| async move { Ok::<_, ()>(x < divergence_point) };
        let result = binary_search_with(0, 100, &mut f).await;
        assert_eq!(result, Ok(divergence_point - 1));
    }
}

#[derive(Debug, Clone, Copy)]
enum RpcErrorKind {
    Transient,
    Fatal,
}

impl From<RpcErrorKind> for RpcError {
    fn from(kind: RpcErrorKind) -> Self {
        match kind {
            RpcErrorKind::Transient => Self::RequestTimeout,
            RpcErrorKind::Fatal => Self::HttpNotImplemented,
        }
    }
}

#[derive(Debug, Default)]
struct MockMainNodeClient {
    l2_block_hashes: BTreeMap<L2BlockNumber, H256>,
    l1_batch_root_hashes: BTreeMap<L1BatchNumber, Result<H256, MissingData>>,
    error_kind: Arc<Mutex<Option<RpcErrorKind>>>,
}

impl MockMainNodeClient {
    fn check_error(&self, method: &'static str) -> EnrichedClientResult<()> {
        if let Some(error_kind) = *self.error_kind.lock().unwrap() {
            return Err(EnrichedClientError::new(error_kind.into(), method));
        }
        Ok(())
    }
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn sealed_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        self.check_error("sealed_l2_block_number")?;
        Ok(self
            .l2_block_hashes
            .last_key_value()
            .map(|x| *x.0)
            .unwrap_or_default())
    }

    async fn sealed_l1_batch_number(&self) -> EnrichedClientResult<L1BatchNumber> {
        self.check_error("sealed_l1_batch_number")?;
        Ok(self
            .l1_batch_root_hashes
            .last_key_value()
            .map(|x| *x.0)
            .unwrap_or_default())
    }

    async fn l2_block_hash(&self, number: L2BlockNumber) -> EnrichedClientResult<Option<H256>> {
        self.check_error("l2_block_hash")
            .map_err(|err| err.with_arg("number", &number))?;
        Ok(self.l2_block_hashes.get(&number).copied())
    }

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>> {
        self.check_error("l1_batch_root_hash")
            .map_err(|err| err.with_arg("number", &number))?;
        let state_hash = self.l1_batch_root_hashes.get(&number).copied();
        Ok(state_hash.unwrap_or(Err(MissingData::Batch)))
    }
}

impl HandleReorgDetectorEvent for mpsc::UnboundedSender<(L2BlockNumber, L1BatchNumber)> {
    fn initialize(&mut self) {
        // Do nothing
    }

    fn update_correct_block(
        &mut self,
        last_correct_l2_block: L2BlockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        self.send((last_correct_l2_block, last_correct_l1_batch))
            .ok();
    }

    fn report_divergence(&mut self, _diverged_l1_batch: L1BatchNumber) {
        // Do nothing
    }

    fn start_shutting_down(&mut self) {
        // Do nothing
    }
}

fn create_mock_detector(
    client: impl MainNodeClient + 'static,
    pool: ConnectionPool<Core>,
) -> ReorgDetector {
    let (health_check, health_updater) = ReactiveHealthCheck::new("reorg_detector");
    ReorgDetector {
        client: Box::new(client),
        event_handler: Box::new(health_updater),
        pool,
        sleep_interval: Duration::from_millis(10),
        health_check,
    }
}

#[test_casing(4, Product(([false, true], [false, true])))]
#[tokio::test]
async fn normal_reorg_function(snapshot_recovery: bool, with_transient_errors: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let mut client = MockMainNodeClient::default();
    if snapshot_recovery {
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
    } else {
        let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        client.l2_block_hashes.insert(
            L2BlockNumber(0),
            L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        );
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(0), Ok(genesis_batch.root_hash));
    }

    let l1_batch_numbers = if snapshot_recovery {
        11_u32..=20
    } else {
        1_u32..=10
    };
    let last_l1_batch_number = L1BatchNumber(*l1_batch_numbers.end());
    let last_l2_block_number = L2BlockNumber(*l1_batch_numbers.end());
    let l2_block_and_l1_batch_hashes: Vec<_> = l1_batch_numbers
        .map(|number| {
            let l2_block_hash = H256::from_low_u64_be(number.into());
            client
                .l2_block_hashes
                .insert(L2BlockNumber(number), l2_block_hash);
            let l1_batch_hash = H256::repeat_byte(number as u8);
            client
                .l1_batch_root_hashes
                .insert(L1BatchNumber(number), Ok(l1_batch_hash));
            (number, l2_block_hash, l1_batch_hash)
        })
        .collect();

    if with_transient_errors {
        *client.error_kind.lock().unwrap() = Some(RpcErrorKind::Transient);
        // "Fix" the client after a certain delay.
        let error_kind = Arc::clone(&client.error_kind);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            *error_kind.lock().unwrap() = None;
        });
    }

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (block_update_sender, mut block_update_receiver) =
        mpsc::unbounded_channel::<(L2BlockNumber, L1BatchNumber)>();
    let detector = ReorgDetector {
        event_handler: Box::new(block_update_sender),
        ..create_mock_detector(client, pool.clone())
    };
    let detector_task = tokio::spawn(detector.run(stop_receiver));

    for (number, l2_block_hash, l1_batch_hash) in l2_block_and_l1_batch_hashes {
        store_l2_block(&mut storage, number, l2_block_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    while let Some((l2_block, l1_batch)) = block_update_receiver.recv().await {
        assert!(l2_block <= last_l2_block_number);
        assert!(l1_batch <= last_l1_batch_number);
        if l2_block == last_l2_block_number && l1_batch == last_l1_batch_number {
            break;
        }
    }

    // Check detector shutdown
    stop_sender.send_replace(true);
    assert_matches!(detector_task.await.unwrap(), Err(OrStopped::Stopped));
}

#[tokio::test]
async fn detector_stops_on_fatal_rpc_error() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let client = MockMainNodeClient::default();
    *client.error_kind.lock().unwrap() = Some(RpcErrorKind::Fatal);

    let stop = watch::channel(false).1;
    let detector = create_mock_detector(client, pool.clone());
    // Check that the detector stops when a fatal RPC error is encountered.
    assert_matches!(detector.run(stop).await, Err(OrStopped::Internal(_)));
}

#[tokio::test]
async fn reorg_is_detected_on_batch_hash_mismatch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let mut client = MockMainNodeClient::default();
    client.l2_block_hashes.insert(
        L2BlockNumber(0),
        L2BlockHasher::legacy_hash(L2BlockNumber(0)),
    );
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), Ok(genesis_batch.root_hash));

    let l2_block_hash = H256::from_low_u64_be(23);
    client
        .l2_block_hashes
        .insert(L2BlockNumber(1), l2_block_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(1), Ok(H256::repeat_byte(1)));
    client
        .l2_block_hashes
        .insert(L2BlockNumber(2), l2_block_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(2), Ok(H256::repeat_byte(2)));

    let mut detector = create_mock_detector(client, pool.clone());

    store_l2_block(&mut storage, 1, l2_block_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_l2_block(&mut storage, 2, l2_block_hash).await;
    detector.check_consistency().await.unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    assert!(!detector
        .check_reorg_presence(stop_receiver.clone())
        .await
        .unwrap());

    seal_l1_batch(&mut storage, 2, H256::repeat_byte(0xff)).await;
    // ^ Hash of L1 batch #2 differs from that on the main node.
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(1)))
    );
    assert!(detector.check_reorg_presence(stop_receiver).await.unwrap());
}

#[tokio::test]
async fn reorg_is_detected_on_l2_block_hash_mismatch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let mut client = MockMainNodeClient::default();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    client.l2_block_hashes.insert(
        L2BlockNumber(0),
        L2BlockHasher::legacy_hash(L2BlockNumber(0)),
    );
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), Ok(genesis_batch.root_hash));

    let l2_block_hash = H256::from_low_u64_be(23);
    client
        .l2_block_hashes
        .insert(L2BlockNumber(1), l2_block_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(1), Ok(H256::repeat_byte(1)));
    client
        .l2_block_hashes
        .insert(L2BlockNumber(2), l2_block_hash);
    client
        .l2_block_hashes
        .insert(L2BlockNumber(3), l2_block_hash);

    let mut detector = create_mock_detector(client, pool.clone());

    store_l2_block(&mut storage, 1, l2_block_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_l2_block(&mut storage, 2, l2_block_hash).await;
    detector.check_consistency().await.unwrap();

    store_l2_block(&mut storage, 3, H256::repeat_byte(42)).await;
    // ^ Hash of the L2 block #3 differs from that on the main node.
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(1)))
    );
    // ^ All locally stored L1 batches should be correct.
}

#[derive(Debug, Clone, Copy)]
enum StorageUpdateStrategy {
    /// Prefill the local storage with all block data.
    Prefill,
    /// Sequentially add a new L1 batch after the previous one was checked.
    Sequential,
}

impl StorageUpdateStrategy {
    const ALL: [Self; 2] = [Self::Prefill, Self::Sequential];
}

#[test_casing(16, Product(([false, true], [2, 3, 5, 8], StorageUpdateStrategy::ALL)))]
#[tokio::test]
async fn reorg_is_detected_on_historic_batch_hash_mismatch(
    snapshot_recovery: bool,
    last_correct_batch: u32,
    storage_update_strategy: StorageUpdateStrategy,
) {
    assert!(last_correct_batch < 10);
    let (l1_batch_numbers, last_correct_batch) = if snapshot_recovery {
        (11_u32..=20, last_correct_batch + 10)
    } else {
        (1_u32..=10, last_correct_batch)
    };

    let pool = ConnectionPool::<Core>::test_pool().await;
    let earliest_l1_batch_number = l1_batch_numbers.start() - 1;
    {
        let mut storage = pool.connection().await.unwrap();
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        store_l2_block(&mut storage, earliest_l1_batch_number, H256::zero()).await;
        seal_l1_batch(&mut storage, earliest_l1_batch_number, H256::zero()).await;
    }
    let mut client = MockMainNodeClient::default();
    client
        .l2_block_hashes
        .insert(L2BlockNumber(earliest_l1_batch_number), H256::zero());
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(earliest_l1_batch_number), Ok(H256::zero()));

    let l2_block_and_l1_batch_hashes = l1_batch_numbers.clone().map(|number| {
        let mut l2_block_hash = H256::from_low_u64_be(number.into());
        client
            .l2_block_hashes
            .insert(L2BlockNumber(number), l2_block_hash);
        let mut l1_batch_hash = H256::repeat_byte(number as u8);
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(number), Ok(l1_batch_hash));

        if number > last_correct_batch {
            l2_block_hash = H256::zero();
            l1_batch_hash = H256::zero();
        }
        (number, l2_block_hash, l1_batch_hash)
    });
    let mut l2_block_and_l1_batch_hashes: Vec<_> = l2_block_and_l1_batch_hashes.collect();

    if matches!(storage_update_strategy, StorageUpdateStrategy::Prefill) {
        let mut storage = pool.connection().await.unwrap();
        for &(number, l2_block_hash, l1_batch_hash) in &l2_block_and_l1_batch_hashes {
            store_l2_block(&mut storage, number, l2_block_hash).await;
            seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        }
    }

    let (block_update_sender, mut block_update_receiver) =
        mpsc::unbounded_channel::<(L2BlockNumber, L1BatchNumber)>();
    let detector = ReorgDetector {
        event_handler: Box::new(block_update_sender),
        ..create_mock_detector(client, pool.clone())
    };

    if matches!(storage_update_strategy, StorageUpdateStrategy::Sequential) {
        tokio::spawn(async move {
            let mut storage = pool.connection().await.unwrap();
            let mut last_number = earliest_l1_batch_number;
            while let Some((l2_block, l1_batch)) = block_update_receiver.recv().await {
                if l2_block == L2BlockNumber(last_number) && l1_batch == L1BatchNumber(last_number)
                {
                    let (number, l2_block_hash, l1_batch_hash) =
                        l2_block_and_l1_batch_hashes.remove(0);
                    assert_eq!(number, last_number + 1);
                    store_l2_block(&mut storage, number, l2_block_hash).await;
                    seal_l1_batch(&mut storage, number, l1_batch_hash).await;
                    last_number = number;
                }
            }
        });
    }

    assert_matches!(
        detector.run(watch::channel(false).1).await,
        Err(OrStopped::Internal(Error::ReorgDetected(got))) if got.0 == last_correct_batch
    );
}

#[tokio::test]
async fn stopping_reorg_detector_while_waiting_for_l1_batch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    assert!(storage.blocks_dal().is_genesis_needed().await.unwrap());
    drop(storage);

    let (stop_sender, stop_receiver) = watch::channel(false);
    let detector = create_mock_detector(MockMainNodeClient::default(), pool);
    let detector_task = tokio::spawn(detector.run(stop_receiver));

    stop_sender.send_replace(true);
    assert_matches!(detector_task.await.unwrap(), Err(OrStopped::Stopped));
}

#[tokio::test]
async fn detector_errors_on_earliest_batch_hash_mismatch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    assert_ne!(genesis_batch.root_hash, H256::zero());

    let mut client = MockMainNodeClient::default();
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), Ok(H256::zero()));
    client
        .l2_block_hashes
        .insert(L2BlockNumber(0), H256::zero());

    let mut detector = create_mock_detector(client, pool.clone());
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::EarliestL1BatchMismatch(L1BatchNumber(0)))
    );
}

#[tokio::test]
async fn detector_errors_on_earliest_batch_hash_mismatch_with_snapshot_recovery() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut client = MockMainNodeClient::default();
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(3), Ok(H256::zero()));
    client
        .l2_block_hashes
        .insert(L2BlockNumber(3), H256::zero());
    let detector = create_mock_detector(client, pool.clone());

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut storage = pool.connection().await.unwrap();
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        store_l2_block(&mut storage, 3, H256::from_low_u64_be(3)).await;
        seal_l1_batch(&mut storage, 3, H256::from_low_u64_be(3)).await;
    });

    assert_matches!(
        detector.run(watch::channel(false).1).await,
        Err(OrStopped::Internal(Error::EarliestL1BatchMismatch(
            L1BatchNumber(3)
        )))
    );
}

#[tokio::test]
async fn reorg_is_detected_without_waiting_for_main_node_to_catch_up() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Fill in local storage with some data, so that it's ahead of the main node.
    for number in 1..5 {
        store_l2_block(&mut storage, number, H256::zero()).await;
        seal_l1_batch(&mut storage, number, H256::zero()).await;
    }
    drop(storage);

    let mut client = MockMainNodeClient::default();
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), Ok(genesis_batch.root_hash));
    for number in 1..3 {
        client
            .l2_block_hashes
            .insert(L2BlockNumber(number), H256::zero());
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(number), Ok(H256::zero()));
    }
    client
        .l2_block_hashes
        .insert(L2BlockNumber(3), H256::zero());
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(3), Ok(H256::repeat_byte(0xff)));

    let mut detector = create_mock_detector(client, pool);
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(2)))
    );
}

/// Tests the worst-case scenario w.r.t. L1 batch root hashes: *all* root hashes match locally and on the main node, only L2 block hashes diverge.
#[test_casing(3, [2, 5, 8])]
#[tokio::test]
async fn reorg_is_detected_based_on_l2_block_hashes(last_correct_l1_batch: u32) {
    const L1_BATCH_COUNT: u32 = 10;

    assert!(last_correct_l1_batch < L1_BATCH_COUNT);

    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();

    let mut client = MockMainNodeClient::default();
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), Ok(genesis_batch.root_hash));
    for number in 1..L1_BATCH_COUNT {
        let l2_block_hash = H256::from_low_u64_le(number.into());
        store_l2_block(&mut storage, number, l2_block_hash).await;
        let remote_l2_block_hash = if number <= last_correct_l1_batch {
            l2_block_hash
        } else {
            H256::zero()
        };
        client
            .l2_block_hashes
            .insert(L2BlockNumber(number), remote_l2_block_hash);

        let l1_batch_root_hash = H256::from_low_u64_be(number.into());
        seal_l1_batch(&mut storage, number, l1_batch_root_hash).await;
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(number), Ok(l1_batch_root_hash));
    }
    drop(storage);

    let mut detector = create_mock_detector(client, pool);
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(num))) if num == last_correct_l1_batch
    );

    let (_stop_sender, stop_receiver) = watch::channel(false);
    assert!(detector.check_reorg_presence(stop_receiver).await.unwrap());
}

#[derive(Debug)]
struct SlowMainNode {
    l1_batch_root_hash_call_count: Arc<AtomicUsize>,
    delay_call_count: usize,
    genesis_root_hash: H256,
}

impl SlowMainNode {
    fn new(genesis_root_hash: H256, delay_call_count: usize) -> Self {
        Self {
            l1_batch_root_hash_call_count: Arc::default(),
            delay_call_count,
            genesis_root_hash,
        }
    }
}

#[async_trait]
impl MainNodeClient for SlowMainNode {
    async fn sealed_l2_block_number(&self) -> EnrichedClientResult<L2BlockNumber> {
        Ok(L2BlockNumber(0))
    }

    async fn sealed_l1_batch_number(&self) -> EnrichedClientResult<L1BatchNumber> {
        Ok(L1BatchNumber(0))
    }

    async fn l2_block_hash(&self, number: L2BlockNumber) -> EnrichedClientResult<Option<H256>> {
        Ok(if number == L2BlockNumber(0) {
            Some(L2BlockHasher::legacy_hash(L2BlockNumber(0)))
        } else {
            None
        })
    }

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Result<H256, MissingData>> {
        if number > L1BatchNumber(0) {
            return Ok(Err(MissingData::Batch));
        }
        let count = self
            .l1_batch_root_hash_call_count
            .fetch_add(1, Ordering::Relaxed);
        Ok(if count >= self.delay_call_count {
            Ok(self.genesis_root_hash)
        } else {
            Err(MissingData::RootHash)
        })
    }
}

#[tokio::test]
async fn detector_waits_for_state_hash_on_main_node() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    drop(storage);

    let client = SlowMainNode::new(genesis_batch.root_hash, 5);
    let l1_batch_root_hash_call_count = client.l1_batch_root_hash_call_count.clone();
    let mut detector = create_mock_detector(client, pool);
    let (_stop_sender, stop_receiver) = watch::channel(false);
    detector.run_once(stop_receiver).await.unwrap();

    let call_count = l1_batch_root_hash_call_count.load(Ordering::Relaxed);
    assert!(call_count >= 5, "{call_count}");
}

#[tokio::test]
async fn detector_does_not_deadlock_if_main_node_is_not_available() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    drop(storage);

    // `client` will always return retriable errors making the detector to retry its checks indefinitely
    let client = MockMainNodeClient::default();
    *client.error_kind.lock().unwrap() = Some(RpcErrorKind::Transient);
    let mut detector = create_mock_detector(client, pool);
    let (stop_sender, stop_receiver) = watch::channel(false);

    let detector_handle = tokio::spawn(async move { detector.run_once(stop_receiver).await });
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!detector_handle.is_finished());

    stop_sender.send_replace(true);
    assert_matches!(
        detector_handle.await.unwrap().unwrap_err(),
        OrStopped::Stopped
    );
}
