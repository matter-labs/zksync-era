//! Tests for the reorg detector component.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use test_casing::{test_casing, Product};
use tokio::sync::mpsc;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::StorageProcessor;
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    Address, L2ChainId, ProtocolVersionId,
};

use super::*;
use crate::genesis::{ensure_genesis_state, GenesisParams};

async fn store_miniblock(storage: &mut StorageProcessor<'_>, number: u32, hash: H256) {
    let header = MiniblockHeader {
        number: MiniblockNumber(number),
        timestamp: number.into(),
        hash,
        l1_tx_count: 0,
        l2_tx_count: 0,
        base_fee_per_gas: 0,
        l1_gas_price: 0,
        l2_fair_gas_price: 0,
        base_system_contracts_hashes: BaseSystemContractsHashes::default(),
        protocol_version: Some(ProtocolVersionId::latest()),
        virtual_blocks: 1,
    };
    storage
        .blocks_dal()
        .insert_miniblock(&header)
        .await
        .unwrap();
}

async fn seal_l1_batch(storage: &mut StorageProcessor<'_>, number: u32, hash: H256) {
    let header = L1BatchHeader::new(
        L1BatchNumber(number),
        number.into(),
        Address::default(),
        BaseSystemContractsHashes::default(),
        ProtocolVersionId::latest(),
    );
    storage
        .blocks_dal()
        .insert_l1_batch(&header, &[], BlockGasCount::default(), &[], &[])
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
async fn test_binary_search() {
    for divergence_point in [1, 50, 51, 100] {
        let mut f = |x| async move { Ok::<_, ()>(x < divergence_point) };
        let result = binary_search_with(0, 100, &mut f).await;
        assert_eq!(result, Ok(divergence_point - 1));
    }
}

type ResponsesMap<K> = HashMap<K, H256>;

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
    miniblock_responses: ResponsesMap<MiniblockNumber>,
    l1_batch_responses: ResponsesMap<L1BatchNumber>,
    error_kind: Arc<Mutex<Option<RpcErrorKind>>>,
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn miniblock_hash(&self, number: MiniblockNumber) -> Result<Option<H256>, RpcError> {
        if let &Some(error_kind) = &*self.error_kind.lock().unwrap() {
            return Err(error_kind.into());
        }

        if let Some(response) = self.miniblock_responses.get(&number) {
            Ok(Some(*response))
        } else {
            Ok(None)
        }
    }

    async fn l1_batch_root_hash(&self, number: L1BatchNumber) -> Result<Option<H256>, RpcError> {
        if let &Some(error_kind) = &*self.error_kind.lock().unwrap() {
            return Err(error_kind.into());
        }

        if let Some(response) = self.l1_batch_responses.get(&number) {
            Ok(Some(*response))
        } else {
            Ok(None)
        }
    }
}

impl UpdateCorrectBlock for mpsc::UnboundedSender<(MiniblockNumber, L1BatchNumber)> {
    fn update_correct_block(
        &mut self,
        last_correct_miniblock: MiniblockNumber,
        last_correct_l1_batch: L1BatchNumber,
    ) {
        self.send((last_correct_miniblock, last_correct_l1_batch))
            .ok();
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn normal_reorg_function(with_transient_errors: bool) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let mut client = MockMainNodeClient::default();
    let miniblock_and_l1_batch_hashes = (1_u32..=10).map(|number| {
        let miniblock_hash = H256::from_low_u64_be(number.into());
        client
            .miniblock_responses
            .insert(MiniblockNumber(number), miniblock_hash);
        let l1_batch_hash = H256::repeat_byte(number as u8);
        client
            .l1_batch_responses
            .insert(L1BatchNumber(number), l1_batch_hash);
        (number, miniblock_hash, l1_batch_hash)
    });
    let miniblock_and_l1_batch_hashes: Vec<_> = miniblock_and_l1_batch_hashes.collect();

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
        client: Box::new(client),
        block_updater: Box::new(block_update_sender),
        pool: pool.clone(),
        stop_receiver,
        sleep_interval: Duration::from_millis(10),
    };
    let detector_task = tokio::spawn(detector.run());

    for (number, miniblock_hash, l1_batch_hash) in miniblock_and_l1_batch_hashes {
        store_miniblock(&mut storage, number, miniblock_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    while let Some((miniblock, l1_batch)) = block_update_receiver.recv().await {
        assert!(miniblock <= MiniblockNumber(10));
        assert!(l1_batch <= L1BatchNumber(10));
        if miniblock == MiniblockNumber(10) && l1_batch == L1BatchNumber(10) {
            break;
        }
    }

    // Check detector shutdown
    stop_sender.send_replace(true);
    let task_result = detector_task.await.unwrap();
    assert_eq!(task_result.unwrap(), None);
}

#[tokio::test]
async fn handling_fatal_rpc_error() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let client = MockMainNodeClient::default();
    *client.error_kind.lock().unwrap() = Some(RpcErrorKind::Fatal);

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let detector = ReorgDetector {
        client: Box::new(client),
        block_updater: Box::new(()),
        pool: pool.clone(),
        stop_receiver,
        sleep_interval: Duration::from_millis(10),
    };
    detector.run().await.unwrap_err();
}

#[tokio::test]
async fn detecting_reorg_by_batch_hash() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut client = MockMainNodeClient::default();
    let miniblock_hash = H256::from_low_u64_be(23);
    client
        .miniblock_responses
        .insert(MiniblockNumber(1), miniblock_hash);
    client
        .l1_batch_responses
        .insert(L1BatchNumber(1), H256::repeat_byte(1));
    client
        .miniblock_responses
        .insert(MiniblockNumber(2), miniblock_hash);
    client
        .l1_batch_responses
        .insert(L1BatchNumber(2), H256::repeat_byte(2));

    let detector = ReorgDetector {
        client: Box::new(client),
        block_updater: Box::new(()),
        pool: pool.clone(),
        stop_receiver,
        sleep_interval: Duration::from_millis(10),
    };
    let detector_task = tokio::spawn(detector.run());

    store_miniblock(&mut storage, 1, miniblock_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_miniblock(&mut storage, 2, miniblock_hash).await;
    seal_l1_batch(&mut storage, 2, H256::repeat_byte(0xff)).await;
    // ^ Hash of L1 batch #2 differs from that on the main node.

    let task_result = detector_task.await.unwrap();
    let last_correct_l1_batch = task_result.unwrap();
    assert_eq!(last_correct_l1_batch, Some(L1BatchNumber(1)));
}

#[tokio::test]
async fn detecting_reorg_by_miniblock_hash() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut client = MockMainNodeClient::default();
    let miniblock_hash = H256::from_low_u64_be(23);
    client
        .miniblock_responses
        .insert(MiniblockNumber(1), miniblock_hash);
    client
        .l1_batch_responses
        .insert(L1BatchNumber(1), H256::repeat_byte(1));
    client
        .miniblock_responses
        .insert(MiniblockNumber(2), miniblock_hash);
    client
        .miniblock_responses
        .insert(MiniblockNumber(3), miniblock_hash);

    let detector = ReorgDetector {
        client: Box::new(client),
        block_updater: Box::new(()),
        pool: pool.clone(),
        stop_receiver,
        sleep_interval: Duration::from_millis(10),
    };
    let detector_task = tokio::spawn(detector.run());

    store_miniblock(&mut storage, 1, miniblock_hash).await;
    seal_l1_batch(&mut storage, 1, H256::repeat_byte(1)).await;
    store_miniblock(&mut storage, 2, miniblock_hash).await;
    store_miniblock(&mut storage, 3, H256::repeat_byte(42)).await;
    // ^ Hash of the miniblock #3 differs from that on the main node.

    let task_result = detector_task.await.unwrap();
    let last_correct_l1_batch = task_result.unwrap();
    assert_eq!(last_correct_l1_batch, Some(L1BatchNumber(1)));
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

#[test_casing(8, Product(([2, 3, 5, 8], StorageUpdateStrategy::ALL)))]
#[tokio::test]
async fn detecting_deep_reorg(
    last_correct_batch: u32,
    storage_update_strategy: StorageUpdateStrategy,
) {
    assert!(last_correct_batch < 10);

    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let mut client = MockMainNodeClient::default();
    let miniblock_and_l1_batch_hashes = (1_u32..=10).map(|number| {
        let mut miniblock_hash = H256::from_low_u64_be(number.into());
        client
            .miniblock_responses
            .insert(MiniblockNumber(number), miniblock_hash);
        let mut l1_batch_hash = H256::repeat_byte(number as u8);
        client
            .l1_batch_responses
            .insert(L1BatchNumber(number), l1_batch_hash);

        if number > last_correct_batch {
            miniblock_hash = H256::zero();
            l1_batch_hash = H256::zero();
        }
        (number, miniblock_hash, l1_batch_hash)
    });
    let mut miniblock_and_l1_batch_hashes: Vec<_> = miniblock_and_l1_batch_hashes.collect();

    if matches!(storage_update_strategy, StorageUpdateStrategy::Prefill) {
        for &(number, miniblock_hash, l1_batch_hash) in &miniblock_and_l1_batch_hashes {
            store_miniblock(&mut storage, number, miniblock_hash).await;
            seal_l1_batch(&mut storage, number, l1_batch_hash).await;
        }
    }

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let (block_update_sender, mut block_update_receiver) =
        mpsc::unbounded_channel::<(MiniblockNumber, L1BatchNumber)>();
    let detector = ReorgDetector {
        client: Box::new(client),
        block_updater: Box::new(block_update_sender),
        pool: pool.clone(),
        stop_receiver,
        sleep_interval: Duration::from_millis(10),
    };
    let detector_task = tokio::spawn(detector.run());

    if matches!(storage_update_strategy, StorageUpdateStrategy::Sequential) {
        let mut last_number = 0;
        while let Some((miniblock, l1_batch)) = block_update_receiver.recv().await {
            if miniblock == MiniblockNumber(last_number) && l1_batch == L1BatchNumber(last_number) {
                let (number, miniblock_hash, l1_batch_hash) =
                    miniblock_and_l1_batch_hashes.remove(0);
                assert_eq!(number, last_number + 1);
                store_miniblock(&mut storage, number, miniblock_hash).await;
                seal_l1_batch(&mut storage, number, l1_batch_hash).await;
                last_number = number;
            }
        }
    }

    let task_result = detector_task.await.unwrap();
    let last_correct_l1_batch = task_result.unwrap();
    assert_eq!(
        last_correct_l1_batch,
        Some(L1BatchNumber(last_correct_batch))
    );
}
