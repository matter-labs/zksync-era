use std::{
    collections::HashMap,
    ops,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use assert_matches::assert_matches;
use async_trait::async_trait;
use test_casing::test_casing;
use zksync_dal::Connection;
use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
use zksync_node_test_utils::{create_l1_batch, create_l2_block, prepare_recovery_snapshot};
use zksync_types::{AccountTreeId, Address, L2BlockNumber, StorageKey, StorageLog, H256};
use zksync_web3_decl::jsonrpsee::core::ClientError;

use super::{metrics::StepOutcomeLabel, provider::TreeDataProviderResult, *};

#[derive(Debug, Default)]
pub(super) struct MockMainNodeClient {
    transient_error: Arc<AtomicBool>,
    batch_details_responses: HashMap<L1BatchNumber, H256>,
}

impl MockMainNodeClient {
    pub fn insert_batch(&mut self, number: L1BatchNumber, root_hash: H256) {
        self.batch_details_responses.insert(number, root_hash);
    }
}

#[async_trait]
impl TreeDataProvider for MockMainNodeClient {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        _last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult {
        if self.transient_error.fetch_and(false, Ordering::Relaxed) {
            let err = ClientError::RequestTimeout;
            return Err(EnrichedClientError::new(err, "batch_details").into());
        }
        Ok(self
            .batch_details_responses
            .get(&number)
            .copied()
            .ok_or(MissingData::Batch))
    }
}

async fn seal_l1_batch(storage: &mut Connection<'_, Core>, number: L1BatchNumber) {
    seal_l1_batch_with_timestamp(storage, number, number.0.into()).await;
}

pub(super) async fn seal_l1_batch_with_timestamp(
    storage: &mut Connection<'_, Core>,
    number: L1BatchNumber,
    timestamp: u64,
) {
    let mut transaction = storage.start_transaction().await.unwrap();
    // Insert a single L2 block belonging to the batch.
    let mut block_header = create_l2_block(number.0);
    block_header.timestamp = timestamp;
    transaction
        .blocks_dal()
        .insert_l2_block(&block_header)
        .await
        .unwrap();

    let mut batch_header = create_l1_batch(number.0);
    batch_header.timestamp = timestamp;
    transaction
        .blocks_dal()
        .insert_mock_l1_batch(&batch_header)
        .await
        .unwrap();
    transaction
        .blocks_dal()
        .mark_l2_blocks_as_executed_in_l1_batch(batch_header.number)
        .await
        .unwrap();

    // One initial write per L1 batch
    let initial_writes = [StorageKey::new(
        AccountTreeId::new(Address::repeat_byte(1)),
        H256::from_low_u64_be(number.0.into()),
    )
    .hashed_key()];
    transaction
        .storage_logs_dedup_dal()
        .insert_initial_writes(number, &initial_writes)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

pub(super) async fn get_last_l2_block(
    storage: &mut Connection<'_, Core>,
    number: L1BatchNumber,
) -> L2BlockHeader {
    TreeDataFetcher::get_last_l2_block(storage, number)
        .await
        .unwrap()
}

#[derive(Debug)]
struct FetcherHarness {
    fetcher: TreeDataFetcher,
    updates_receiver: mpsc::UnboundedReceiver<L1BatchNumber>,
    metrics: &'static TreeDataFetcherMetrics,
}

impl FetcherHarness {
    fn new(client: impl TreeDataProvider, pool: ConnectionPool<Core>) -> Self {
        let (updates_sender, updates_receiver) = mpsc::unbounded_channel();
        let metrics = &*Box::leak(Box::<TreeDataFetcherMetrics>::default());
        let fetcher = TreeDataFetcher {
            data_provider: CombinedDataProvider::new(client),
            diamond_proxy_address: None,
            pool: pool.clone(),
            metrics,
            health_updater: ReactiveHealthCheck::new("tree_data_fetcher").1,
            poll_interval: Duration::from_millis(10),
            updates_sender,
        };
        Self {
            fetcher,
            updates_receiver,
            metrics,
        }
    }
}

#[tokio::test]
async fn tree_data_fetcher_steps() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis = insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let mut client = MockMainNodeClient::default();
    for number in 1..=5 {
        let number = L1BatchNumber(number);
        client
            .batch_details_responses
            .insert(number, H256::from_low_u64_be(number.0.into()));
        seal_l1_batch(&mut storage, number).await;
    }

    let mut fetcher = FetcherHarness::new(client, pool.clone()).fetcher;
    for number in 1..=5 {
        let step_outcome = fetcher.step().await.unwrap();
        assert_matches!(
            step_outcome,
            StepOutcome::UpdatedBatch(updated_number) if updated_number == L1BatchNumber(number)
        );
    }
    let step_outcome = fetcher.step().await.unwrap();
    assert_matches!(step_outcome, StepOutcome::NoProgress);

    // Check tree data in updated batches.
    assert_batch_tree_data(&mut storage, 1..=5, genesis.rollup_last_leaf_index).await;
}

async fn assert_batch_tree_data(
    storage: &mut Connection<'_, Core>,
    batch_numbers: ops::RangeInclusive<u32>,
    starting_rollup_last_leaf_index: u64,
) {
    for (i, number) in batch_numbers.enumerate() {
        let tree_data = storage
            .blocks_dal()
            .get_l1_batch_tree_data(L1BatchNumber(number))
            .await
            .unwrap();
        let tree_data = tree_data.unwrap_or_else(|| {
            panic!("No tree data persisted for L1 batch #{number}");
        });
        assert_eq!(tree_data.hash, H256::from_low_u64_be(number.into()));
        assert_eq!(
            tree_data.rollup_last_leaf_index,
            starting_rollup_last_leaf_index + i as u64 + 1 // expecting 1 initial write per batch
        );
    }
}

#[tokio::test]
async fn tree_data_fetcher_steps_after_snapshot_recovery() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let account = AccountTreeId::new(Address::zero());
    let snapshot_storage_logs: Vec<_> = (0..20)
        .map(|i| {
            let key = StorageKey::new(account, H256::repeat_byte(i));
            StorageLog::new_write_log(key, H256::repeat_byte(0xff))
        })
        .collect();
    let snapshot = prepare_recovery_snapshot(
        &mut storage,
        L1BatchNumber(23),
        L2BlockNumber(42),
        &snapshot_storage_logs,
    )
    .await;

    let mut client = MockMainNodeClient::default();
    for i in 1..=5 {
        let number = snapshot.l1_batch_number + i;
        client
            .batch_details_responses
            .insert(number, H256::from_low_u64_be(number.0.into()));
        seal_l1_batch(&mut storage, number).await;
    }

    let mut fetcher = FetcherHarness::new(client, pool.clone()).fetcher;
    for i in 1..=5 {
        let step_outcome = fetcher.step().await.unwrap();
        assert_matches!(
            step_outcome,
            StepOutcome::UpdatedBatch(updated_number) if updated_number == snapshot.l1_batch_number + i
        );
    }
    let step_outcome = fetcher.step().await.unwrap();
    assert_matches!(step_outcome, StepOutcome::NoProgress);

    let batch_numbers = (snapshot.l1_batch_number.0 + 1)..=(snapshot.l1_batch_number.0 + 5);
    assert_batch_tree_data(&mut storage, batch_numbers, 21).await;
}

#[tokio::test]
async fn tree_data_fetcher_recovers_from_transient_errors() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis = insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();

    let mut client = MockMainNodeClient::default();
    for number in 1..=5 {
        let number = L1BatchNumber(number);
        client
            .batch_details_responses
            .insert(number, H256::from_low_u64_be(number.0.into()));
    }
    let transient_error = client.transient_error.clone();

    let FetcherHarness {
        fetcher,
        mut updates_receiver,
        metrics,
    } = FetcherHarness::new(client, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let fetcher_handle = tokio::spawn(fetcher.run(stop_receiver));

    for number in 1..=5 {
        transient_error.store(true, Ordering::Relaxed);
        // Insert L1 batch into a local storage and wait for its tree data to be updated.
        seal_l1_batch(&mut storage, L1BatchNumber(number)).await;
        let updated_batch = updates_receiver.recv().await.unwrap();
        assert_eq!(updated_batch, L1BatchNumber(number));

        let tree_data = storage
            .blocks_dal()
            .get_l1_batch_tree_data(L1BatchNumber(number))
            .await
            .unwrap();
        let tree_data = tree_data.unwrap_or_else(|| {
            panic!("No tree data persisted for L1 batch #{number}");
        });
        assert_eq!(tree_data.hash, H256::from_low_u64_be(number.into()));
        assert_eq!(
            tree_data.rollup_last_leaf_index,
            genesis.rollup_last_leaf_index + u64::from(number)
        );
    }

    // Check metrics.
    assert_eq!(metrics.last_updated_batch_number.get(), 5);
    assert_eq!(
        metrics.step_outcomes[&StepOutcomeLabel::TransientError].get(),
        5
    );
    assert_eq!(
        metrics.step_outcomes[&StepOutcomeLabel::UpdatedBatch].get(),
        5
    );

    stop_sender.send_replace(true);
    fetcher_handle.await.unwrap().unwrap();
}

#[derive(Debug)]
struct SlowMainNode {
    request_count: AtomicUsize,
    compute_root_hash_after: usize,
}

impl SlowMainNode {
    fn new(compute_root_hash_after: usize) -> Self {
        Self {
            request_count: AtomicUsize::new(0),
            compute_root_hash_after,
        }
    }
}

#[async_trait]
impl TreeDataProvider for SlowMainNode {
    async fn batch_details(
        &mut self,
        number: L1BatchNumber,
        _last_l2_block: &L2BlockHeader,
    ) -> TreeDataProviderResult {
        if number != L1BatchNumber(1) {
            return Ok(Err(MissingData::Batch));
        }
        let request_count = self.request_count.fetch_add(1, Ordering::Relaxed);
        Ok(if request_count >= self.compute_root_hash_after {
            Ok(H256::repeat_byte(1))
        } else {
            Err(MissingData::RootHash)
        })
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn tree_data_fetcher_with_missing_remote_hash(delayed_insertion: bool) {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let genesis = insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
        .await
        .unwrap();
    if !delayed_insertion {
        seal_l1_batch(&mut storage, L1BatchNumber(1)).await;
    }

    let client = SlowMainNode::new(3);
    let FetcherHarness {
        fetcher,
        mut updates_receiver,
        metrics,
    } = FetcherHarness::new(client, pool.clone());
    let (stop_sender, stop_receiver) = watch::channel(false);
    let fetcher_handle = tokio::spawn(fetcher.run(stop_receiver));

    if delayed_insertion {
        tokio::time::sleep(Duration::from_millis(10)).await;
        seal_l1_batch(&mut storage, L1BatchNumber(1)).await;
    }

    // Wait until the L1 batch is updated by the fetcher.
    let updated_batch = updates_receiver.recv().await.unwrap();
    assert_eq!(updated_batch, L1BatchNumber(1));

    let tree_data = storage
        .blocks_dal()
        .get_l1_batch_tree_data(L1BatchNumber(1))
        .await
        .unwrap();
    let tree_data = tree_data.expect("no tree data for batch");
    assert_eq!(tree_data.hash, H256::repeat_byte(1));
    assert_eq!(
        tree_data.rollup_last_leaf_index,
        genesis.rollup_last_leaf_index + 1
    );

    // Check metrics.
    assert_eq!(metrics.last_updated_batch_number.get(), 1);
    assert_eq!(
        metrics.step_outcomes[&StepOutcomeLabel::RemoteHashMissing].get(),
        3
    );
    assert_eq!(
        metrics.step_outcomes[&StepOutcomeLabel::UpdatedBatch].get(),
        1
    );

    // Check that the fetcher can be stopped.
    stop_sender.send_replace(true);
    fetcher_handle.await.unwrap().unwrap();
}
