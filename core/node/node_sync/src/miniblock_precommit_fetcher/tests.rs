//! Tests for miniblock precommit fetcher.

use std::{collections::HashMap, sync::Arc};

use tokio::sync::Mutex;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::create_l2_block;
use zksync_types::{
    aggregated_operations::{AggregatedActionType, L2BlockAggregatedActionType},
    L2BlockNumber, SLChainId, H256,
};
use zksync_web3_decl::error::EnrichedClientResult;

use super::*;

const TEST_SL: SLChainId = SLChainId(1);

/// Helper function to create an L2 block in the storage with optional precommit data
async fn insert_l2_block(
    storage: &mut Connection<'_, Core>,
    number: u32,
    rolling_txs_hash: Option<H256>,
) {
    let mut block = create_l2_block(number);
    block.rolling_txs_hash = rolling_txs_hash;
    storage.blocks_dal().insert_l2_block(&block).await.unwrap();
}

async fn insert_l2_block_with_precommit(storage: &mut Connection<'_, Core>, number: u32) {
    let hash = get_deterministic_hash(L2BlockNumber(number));
    insert_l2_block(storage, number, Some(hash)).await;
    storage
        .eth_sender_dal()
        .insert_pending_received_precommit_eth_tx(L2BlockNumber(number), hash, Some(TEST_SL))
        .await
        .unwrap();
}

#[derive(Debug, Clone)]
struct MockMainNodeClient {
    precommit: Arc<Mutex<HashMap<L2BlockNumber, MiniblockPrecommitDetails>>>,
    safe_block: Arc<Mutex<L2BlockNumber>>,
}

impl std::ops::Deref for MockMainNodeClient {
    type Target = Arc<Mutex<HashMap<L2BlockNumber, MiniblockPrecommitDetails>>>;

    fn deref(&self) -> &Self::Target {
        &self.precommit
    }
}

impl MockMainNodeClient {
    fn new(safe_block: L2BlockNumber) -> Self {
        Self {
            precommit: Arc::new(Mutex::new(HashMap::new())),
            safe_block: Arc::new(Mutex::new(safe_block)),
        }
    }

    async fn add_precommit(&self, number: L2BlockNumber, hash: H256, chain_id: SLChainId) {
        let details = MiniblockPrecommitDetails { hash, chain_id };
        let mut precommits = self.precommit.lock().await;
        precommits.insert(number, details);
    }

    async fn set_safe_block(&self, number: L2BlockNumber) {
        let mut safe_block = self.safe_block.lock().await;
        *safe_block = number;
    }
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn get_miniblock_precommit_hash(
        &self,
        number: L2BlockNumber,
    ) -> EnrichedClientResult<Option<MiniblockPrecommitDetails>> {
        let precommits = self.precommit.lock().await;
        let result = precommits.get(&number);
        Ok(result.cloned())
    }

    async fn get_safe_block(&self) -> EnrichedClientResult<L2BlockNumber> {
        let safe_block = *self.safe_block.lock().await;
        Ok(safe_block)
    }
}

/// Helper function to generate deterministic precommit hash for a block number
fn get_deterministic_hash(block_number: L2BlockNumber) -> H256 {
    H256::repeat_byte(block_number.0 as u8)
}

/// Helper function to generate MiniblockPrecommitDetails for a block number
fn precommit_details_for_block(block_number: L2BlockNumber) -> MiniblockPrecommitDetails {
    MiniblockPrecommitDetails {
        hash: get_deterministic_hash(block_number),
        chain_id: TEST_SL,
    }
}

#[tokio::test]
async fn fetcher_cursor_initialization() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Test with no precommits in storage
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.last_processed_miniblock, L2BlockNumber(0));

    insert_l2_block(&mut storage, 1, None).await;
    insert_l2_block_with_precommit(&mut storage, 2).await;
    insert_l2_block_with_precommit(&mut storage, 3).await;
    insert_l2_block(&mut storage, 4, None).await;

    // Cursor should now point to the block with a precommit
    let cursor = FetcherCursor::new(&mut storage).await.unwrap();
    assert_eq!(cursor.last_processed_miniblock, L2BlockNumber(3));
}

async fn assert_block_precommit(
    storage: &mut Connection<'_, Core>,
    miniblock_number: L2BlockNumber,
    expected_precommit: &Option<MiniblockPrecommitDetails>,
) {
    let block = storage
        .blocks_web3_dal()
        .get_block_details_incl_unverified_transactions(miniblock_number)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        block.base.precommit_tx_hash,
        expected_precommit.as_ref().map(|p| p.hash)
    );
    assert_eq!(
        block.base.precommit_chain_id,
        expected_precommit.as_ref().map(|p| p.chain_id)
    );
}

/// Helper function to assert precommit states for a range of blocks
/// It checks that blocks in `blocks_with_precommits` have precommits with deterministic hashes
/// and all other blocks in the range have no precommits
async fn assert_blocks_in_range(
    storage: &mut Connection<'_, Core>,
    range: std::ops::RangeInclusive<u32>,
    blocks_with_precommits: &[L2BlockNumber],
) {
    for block_number in range {
        let l2_block = L2BlockNumber(block_number);
        let has_precommit = blocks_with_precommits.contains(&l2_block);

        if has_precommit {
            assert_block_precommit(
                storage,
                l2_block,
                &Some(precommit_details_for_block(l2_block)),
            )
            .await;
        } else {
            assert_block_precommit(storage, l2_block, &None).await;
        }
    }
}

#[tokio::test]
async fn normal_fetcher_operation() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Insert some L2 blocks without precommits
    for i in 1..=10 {
        insert_l2_block(&mut storage, i, None).await;
    }

    // Create mock client with safe block at 10 and precommits for blocks 3, 5, and 7
    let client = MockMainNodeClient::new(L2BlockNumber(10));
    client
        .add_precommit(L2BlockNumber(3), H256::repeat_byte(3), TEST_SL)
        .await;
    client
        .add_precommit(L2BlockNumber(5), H256::repeat_byte(5), TEST_SL)
        .await;
    client
        .add_precommit(L2BlockNumber(7), H256::repeat_byte(7), TEST_SL)
        .await;

    let fetcher = MiniblockPrecommitFetcher::new(Box::new(client), pool.clone());
    let mut cursor = FetcherCursor::new(&mut storage).await.unwrap();
    fetcher.fetch_precommits(&mut cursor).await.unwrap();

    // Check that all the precommits were stored in the database
    assert_block_precommit(
        &mut storage,
        L2BlockNumber(3),
        &Some(MiniblockPrecommitDetails {
            hash: H256::repeat_byte(3),
            chain_id: TEST_SL,
        }),
    )
    .await;
    assert_block_precommit(
        &mut storage,
        L2BlockNumber(5),
        &Some(MiniblockPrecommitDetails {
            hash: H256::repeat_byte(5),
            chain_id: TEST_SL,
        }),
    )
    .await;
    assert_block_precommit(
        &mut storage,
        L2BlockNumber(7),
        &Some(MiniblockPrecommitDetails {
            hash: H256::repeat_byte(7),
            chain_id: TEST_SL,
        }),
    )
    .await;
}

#[tokio::test]
async fn fetcher_with_incremental_safe_block() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap();
    // Insert L2 blocks without precommits
    for i in 1..=15 {
        insert_l2_block(&mut storage, i, None).await;
    }

    // Create mock client with safe block initially at 5
    let client = MockMainNodeClient::new(L2BlockNumber(5));

    // Add precommits for blocks 2 and 4
    client
        .add_precommit(
            L2BlockNumber(2),
            get_deterministic_hash(L2BlockNumber(2)),
            TEST_SL,
        )
        .await;
    client
        .add_precommit(
            L2BlockNumber(4),
            get_deterministic_hash(L2BlockNumber(4)),
            TEST_SL,
        )
        .await;

    // Add precommits that are not finalized, but will be synced when safe block is increased
    client
        .add_precommit(
            L2BlockNumber(8),
            get_deterministic_hash(L2BlockNumber(8)),
            TEST_SL,
        )
        .await;
    client
        .add_precommit(
            L2BlockNumber(12),
            get_deterministic_hash(L2BlockNumber(12)),
            TEST_SL,
        )
        .await;

    let fetcher = MiniblockPrecommitFetcher::new(Box::new(client.clone()), pool);
    let mut cursor = FetcherCursor::new(&mut storage).await.unwrap();
    fetcher.fetch_precommits(&mut cursor).await.unwrap();

    // After first fetch, blocks 2 and 4 should have precommits (up to safe block 5)
    assert_blocks_in_range(&mut storage, 1..=15, &[L2BlockNumber(2), L2BlockNumber(4)]).await;
    // Increase safe block to 10
    client.set_safe_block(L2BlockNumber(10)).await;
    fetcher.fetch_precommits(&mut cursor).await.unwrap();

    // Now blocks 2, 4, and 8 should have precommits (up to safe block 10)
    assert_blocks_in_range(
        &mut storage,
        1..=15,
        &[L2BlockNumber(2), L2BlockNumber(4), L2BlockNumber(8)],
    )
    .await;

    // Increase safe block to 15
    client.set_safe_block(L2BlockNumber(15)).await;
    fetcher.fetch_precommits(&mut cursor).await.unwrap();

    // Now blocks 2, 4, 8, and 12 should have precommits (up to safe block 15)
    assert_blocks_in_range(
        &mut storage,
        1..=15,
        &[
            L2BlockNumber(2),
            L2BlockNumber(4),
            L2BlockNumber(8),
            L2BlockNumber(12),
        ],
    )
    .await;
}

#[tokio::test]
async fn fetcher_ignores_unsynced_blocks() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    insert_genesis_batch(&mut storage, &GenesisParams::mock())
        .await
        .unwrap(); // Generate a transaction hash for precommit

    // Insert L2 blocks only up to block 10
    for i in 1..=10 {
        insert_l2_block(&mut storage, i, None).await;
    }

    // Create mock client with safe block at 15 (beyond our last synced block)
    let client = MockMainNodeClient::new(L2BlockNumber(15));

    // Add precommits for blocks that exist in our database
    client
        .add_precommit(
            L2BlockNumber(5),
            get_deterministic_hash(L2BlockNumber(5)),
            TEST_SL,
        )
        .await;

    // Add precommits for blocks beyond our last synced block (10)
    // Block 12 is under safe block but doesn't exist in our DB
    client
        .add_precommit(
            L2BlockNumber(12),
            get_deterministic_hash(L2BlockNumber(12)),
            TEST_SL,
        )
        .await;

    let fetcher = MiniblockPrecommitFetcher::new(Box::new(client), pool);
    let mut cursor = FetcherCursor::new(&mut storage).await.unwrap();

    // Fetching should succeed and not error out even with unsynced blocks
    fetcher.fetch_precommits(&mut cursor).await.unwrap();
    assert_eq!(cursor.last_processed_miniblock, L2BlockNumber(10));
    // only one transaction fetched
    assert_eq!(
        storage
            .eth_sender_dal()
            .count_eth_txs_by_type(AggregatedActionType::L2Block(
                L2BlockAggregatedActionType::Precommit
            ))
            .await,
        1
    );
    // Only block 5 should have a precommit
    // Blocks 1-10 are synced, 11-15 are not synced
    assert_blocks_in_range(&mut storage, 1..=10, &[L2BlockNumber(5)]).await;
}
