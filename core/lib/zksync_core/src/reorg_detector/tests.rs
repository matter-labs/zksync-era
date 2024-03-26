//! Tests for the reorg detector component.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use assert_matches::assert_matches;
use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_dal::{Connection, CoreDal};
use zksync_types::{
    block::{MiniblockHasher, MiniblockHeader},
    ProtocolVersion,
};
use zksync_web3_decl::jsonrpsee::core::ClientError as RpcError;

use super::*;
use crate::{
    genesis::{insert_genesis_batch, GenesisParams},
    utils::testonly::{create_l1_batch, create_miniblock},
};

async fn store_miniblock(storage: &mut Connection<'_, Core>, number: u32, hash: H256) {
    let header = MiniblockHeader {
        hash,
        ..create_miniblock(number)
    };
    storage
        .blocks_dal()
        .insert_miniblock(&header)
        .await
        .unwrap();
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
        .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(number))
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
    miniblock_hashes: BTreeMap<MiniblockNumber, H256>,
    l1_batch_root_hashes: BTreeMap<L1BatchNumber, H256>,
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
    async fn sealed_miniblock_number(&self) -> EnrichedClientResult<MiniblockNumber> {
        self.check_error("sealed_miniblock_number")?;
        Ok(self
            .miniblock_hashes
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

    async fn miniblock_hash(&self, number: MiniblockNumber) -> EnrichedClientResult<Option<H256>> {
        self.check_error("miniblock_hash")
            .map_err(|err| err.with_arg("number", &number))?;
        Ok(self.miniblock_hashes.get(&number).copied())
    }

    async fn l1_batch_root_hash(
        &self,
        number: L1BatchNumber,
    ) -> EnrichedClientResult<Option<H256>> {
        self.check_error("l1_batch_root_hash")
            .map_err(|err| err.with_arg("number", &number))?;
        Ok(self.l1_batch_root_hashes.get(&number).copied())
    }
}

impl HandleReorgDetectorEvent for mpsc::UnboundedSender<(MiniblockNumber, L1BatchNumber)> {
    fn initialize(&mut self) {
        // Do nothing
    }

    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        self.send((last_correct_miniblock, last_correct_l1_batch))
            .ok();
    }

    fn report_divergence(&mut self, _diverged_l1_batch: L1BatchNumber) {
        // Do nothing
    }

    fn start_shutting_down(&mut self) {
        // Do nothing
    }
}

fn create_mock_detector(client: MockMainNodeClient, pool: ConnectionPool<Core>) -> ReorgDetector {
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
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
    } else {
        let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        client.miniblock_hashes.insert(
            MiniblockNumber(0),
            MiniblockHasher::legacy_hash(MiniblockNumber(0)),
        );
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(0), genesis_batch.root_hash);
    }

    let l1_batch_numbers = if snapshot_recovery {
        11_u32..=20
    } else {
        1_u32..=10
    };
    let last_l1_batch_number = L1BatchNumber(*l1_batch_numbers.end());
    let last_miniblock_number = MiniblockNumber(*l1_batch_numbers.end());
    let miniblock_and_l1_batch_hashes: Vec<_> = l1_batch_numbers
        .map(|number| {
            let miniblock_hash = H256::from_low_u64_be(number.into());
            client
                .miniblock_hashes
                .insert(MiniblockNumber(number), miniblock_hash);
            let l1_batch_hash = H256::repeat_byte(number as u8);
            client
                .l1_batch_root_hashes
                .insert(L1BatchNumber(number), l1_batch_hash);
            (number, miniblock_hash, l1_batch_hash)
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
        mpsc::unbounded_channel::<(MiniblockNumber, L1BatchNumber)>();
    let detector = ReorgDetector {
        event_handler: Box::new(block_update_sender),
        ..create_mock_detector(client, pool.clone())
    };
    let detector_task = tokio::spawn(detector.run(stop_receiver));

    for (number, miniblock_hash, l1_batch_hash) in miniblock_and_l1_batch_hashes {
        store_miniblock(&mut storage, number, miniblock_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    while let Some((miniblock, l1_batch)) = block_update_receiver.recv().await {
        assert!(miniblock <= last_miniblock_number);
        assert!(l1_batch <= last_l1_batch_number);
        if miniblock == last_miniblock_number && l1_batch == last_l1_batch_number {
            break;
        }
    }

    // Check detector shutdown
    stop_sender.send_replace(true);
    detector_task.await.unwrap().unwrap();
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
    detector.run(stop).await.unwrap_err();
}

#[tokio::test]
async fn reorg_is_detected_on_batch_hash_mismatch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    let mut client = MockMainNodeClient::default();
    client.miniblock_hashes.insert(
        MiniblockNumber(0),
        MiniblockHasher::legacy_hash(MiniblockNumber(0)),
    );
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), genesis_batch.root_hash);

    let miniblock_hash = H256::from_low_u64_be(23);
    client
        .miniblock_hashes
        .insert(MiniblockNumber(1), miniblock_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(1), H256::repeat_byte(1));
    client
        .miniblock_hashes
        .insert(MiniblockNumber(2), miniblock_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(2), H256::repeat_byte(2));

    let mut detector = create_mock_detector(client, pool.clone());

    store_miniblock(&mut storage, 1, miniblock_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_miniblock(&mut storage, 2, miniblock_hash).await;
    detector.check_consistency().await.unwrap();

    seal_l1_batch(&mut storage, 2, H256::repeat_byte(0xff)).await;
    // ^ Hash of L1 batch #2 differs from that on the main node.
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(1)))
    );
}

#[tokio::test]
async fn reorg_is_detected_on_miniblock_hash_mismatch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let mut client = MockMainNodeClient::default();
    let genesis_batch = insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    client.miniblock_hashes.insert(
        MiniblockNumber(0),
        MiniblockHasher::legacy_hash(MiniblockNumber(0)),
    );
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), genesis_batch.root_hash);

    let miniblock_hash = H256::from_low_u64_be(23);
    client
        .miniblock_hashes
        .insert(MiniblockNumber(1), miniblock_hash);
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(1), H256::repeat_byte(1));
    client
        .miniblock_hashes
        .insert(MiniblockNumber(2), miniblock_hash);
    client
        .miniblock_hashes
        .insert(MiniblockNumber(3), miniblock_hash);

    let mut detector = create_mock_detector(client, pool.clone());

    store_miniblock(&mut storage, 1, miniblock_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_miniblock(&mut storage, 2, miniblock_hash).await;
    detector.check_consistency().await.unwrap();

    store_miniblock(&mut storage, 3, H256::repeat_byte(42)).await;
    // ^ Hash of the miniblock #3 differs from that on the main node.
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
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        store_miniblock(&mut storage, earliest_l1_batch_number, H256::zero()).await;
        seal_l1_batch(&mut storage, earliest_l1_batch_number, H256::zero()).await;
    }
    let mut client = MockMainNodeClient::default();
    client
        .miniblock_hashes
        .insert(MiniblockNumber(earliest_l1_batch_number), H256::zero());
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(earliest_l1_batch_number), H256::zero());

    let miniblock_and_l1_batch_hashes = l1_batch_numbers.clone().map(|number| {
        let mut miniblock_hash = H256::from_low_u64_be(number.into());
        client
            .miniblock_hashes
            .insert(MiniblockNumber(number), miniblock_hash);
        let mut l1_batch_hash = H256::repeat_byte(number as u8);
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(number), l1_batch_hash);

        if number > last_correct_batch {
            miniblock_hash = H256::zero();
            l1_batch_hash = H256::zero();
        }
        (number, miniblock_hash, l1_batch_hash)
    });
    let mut miniblock_and_l1_batch_hashes: Vec<_> = miniblock_and_l1_batch_hashes.collect();

    if matches!(storage_update_strategy, StorageUpdateStrategy::Prefill) {
        let mut storage = pool.connection().await.unwrap();
        for &(number, miniblock_hash, l1_batch_hash) in &miniblock_and_l1_batch_hashes {
            store_miniblock(&mut storage, number, miniblock_hash).await;
            seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        }
    }

    let (block_update_sender, mut block_update_receiver) =
        mpsc::unbounded_channel::<(MiniblockNumber, L1BatchNumber)>();
    let detector = ReorgDetector {
        event_handler: Box::new(block_update_sender),
        ..create_mock_detector(client, pool.clone())
    };

    if matches!(storage_update_strategy, StorageUpdateStrategy::Sequential) {
        tokio::spawn(async move {
            let mut storage = pool.connection().await.unwrap();
            let mut last_number = earliest_l1_batch_number;
            while let Some((miniblock, l1_batch)) = block_update_receiver.recv().await {
                if miniblock == MiniblockNumber(last_number)
                    && l1_batch == L1BatchNumber(last_number)
                {
                    let (number, miniblock_hash, l1_batch_hash) =
                        miniblock_and_l1_batch_hashes.remove(0);
                    assert_eq!(number, last_number + 1);
                    store_miniblock(&mut storage, number, miniblock_hash).await;
                    seal_l1_batch(&mut storage, number, l1_batch_hash).await;
                    last_number = number;
                }
            }
        });
    }

    assert_matches!(
        detector.run(watch::channel(false).1).await,
        Err(Error::ReorgDetected(got)) if got.0 == last_correct_batch
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
    detector_task.await.unwrap().unwrap();
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
        .insert(L1BatchNumber(0), H256::zero());
    client
        .miniblock_hashes
        .insert(MiniblockNumber(0), H256::zero());

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
        .insert(L1BatchNumber(3), H256::zero());
    client
        .miniblock_hashes
        .insert(MiniblockNumber(3), H256::zero());
    let detector = create_mock_detector(client, pool.clone());

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let mut storage = pool.connection().await.unwrap();
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        store_miniblock(&mut storage, 3, H256::from_low_u64_be(3)).await;
        seal_l1_batch(&mut storage, 3, H256::from_low_u64_be(3)).await;
    });

    assert_matches!(
        detector.run(watch::channel(false).1).await,
        Err(Error::EarliestL1BatchMismatch(L1BatchNumber(3)))
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
        store_miniblock(&mut storage, number, H256::zero()).await;
        seal_l1_batch(&mut storage, number, H256::zero()).await;
    }
    drop(storage);

    let mut client = MockMainNodeClient::default();
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(0), genesis_batch.root_hash);
    for number in 1..3 {
        client
            .miniblock_hashes
            .insert(MiniblockNumber(number), H256::zero());
        client
            .l1_batch_root_hashes
            .insert(L1BatchNumber(number), H256::zero());
    }
    client
        .miniblock_hashes
        .insert(MiniblockNumber(3), H256::zero());
    client
        .l1_batch_root_hashes
        .insert(L1BatchNumber(3), H256::repeat_byte(0xff));

    let mut detector = create_mock_detector(client, pool);
    assert_matches!(
        detector.check_consistency().await,
        Err(Error::ReorgDetected(L1BatchNumber(2)))
    );
}
