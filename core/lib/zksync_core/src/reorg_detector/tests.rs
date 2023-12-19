//! Tests for the reorg detector component.

use std::collections::HashMap;

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

#[derive(Debug, Default)]
struct MockMaiNodeClient {
    miniblock_responses: ResponsesMap<MiniblockNumber>,
    l1_batch_responses: ResponsesMap<L1BatchNumber>,
}

#[async_trait]
impl MainNodeClient for MockMaiNodeClient {
    async fn miniblock_hash(&self, number: MiniblockNumber) -> Result<Option<H256>, RpcError> {
        dbg!(number);
        if let Some(response) = self.miniblock_responses.get(&number) {
            Ok(Some(*response))
        } else {
            Ok(None)
        }
    }

    async fn l1_batch_root_hash(&self, number: L1BatchNumber) -> Result<Option<H256>, RpcError> {
        dbg!(number);
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

#[tokio::test]
async fn normal_reorg_function() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let (stop_sender, stop_receiver) = watch::channel(false);
    let (block_update_sender, mut block_update_receiver) =
        mpsc::unbounded_channel::<(MiniblockNumber, L1BatchNumber)>();
    let mut client = MockMaiNodeClient::default();

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
async fn detecting_reorg_by_batch_hash() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis_state(&mut storage, L2ChainId::default(), &GenesisParams::mock())
        .await
        .unwrap();

    let (_stop_sender, stop_receiver) = watch::channel(false);
    let mut client = MockMaiNodeClient::default();
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

// FIXME: test mismatch by miniblock hash, transient and non-transient errors
