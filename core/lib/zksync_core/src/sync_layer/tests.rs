//! High-level sync layer tests.

use async_trait::async_trait;
use tokio::{sync::watch, task::JoinHandle};

use std::{
    collections::{HashMap, VecDeque},
    iter,
    time::{Duration, Instant},
};

use zksync_config::configs::chain::NetworkConfig;
use zksync_contracts::{BaseSystemContractsHashes, SystemContractCode};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_types::{
    api, Address, L1BatchNumber, L2ChainId, MiniblockNumber, ProtocolVersionId, Transaction, H256,
};

use super::{
    fetcher::MainNodeFetcherCursor,
    sync_action::{ActionQueueSender, SyncAction},
    *,
};
use crate::{
    api_server::web3::tests::spawn_http_server,
    genesis::{ensure_genesis_state, GenesisParams},
    state_keeper::{
        tests::{create_l1_batch_metadata, create_l2_transaction, TestBatchExecutorBuilder},
        ZkSyncStateKeeper,
    },
};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Default)]
struct MockMainNodeClient {
    l2_blocks: Vec<api::en::SyncBlock>,
}

impl MockMainNodeClient {
    /// `miniblock_count` doesn't include a fictive miniblock. Returns hashes of generated transactions.
    fn push_l1_batch(&mut self, miniblock_count: u32) -> Vec<H256> {
        let l1_batch_number = self
            .l2_blocks
            .last()
            .map_or(L1BatchNumber(0), |block| block.l1_batch_number + 1);
        let number_offset = self.l2_blocks.len() as u32;

        let mut tx_hashes = vec![];
        let l2_blocks = (0..=miniblock_count).map(|number| {
            let is_fictive = number == miniblock_count;
            let transactions = if is_fictive {
                vec![]
            } else {
                let transaction = create_l2_transaction(10, 100);
                tx_hashes.push(transaction.hash());
                vec![transaction.into()]
            };
            let number = number + number_offset;

            api::en::SyncBlock {
                number: MiniblockNumber(number),
                l1_batch_number,
                last_in_batch: is_fictive,
                timestamp: number.into(),
                root_hash: Some(H256::repeat_byte(1)),
                l1_gas_price: 2,
                l2_fair_gas_price: 3,
                base_system_contracts_hashes: BaseSystemContractsHashes::default(),
                operator_address: Address::repeat_byte(2),
                transactions: Some(transactions),
                virtual_blocks: Some(!is_fictive as u32),
                hash: Some(H256::repeat_byte(1)),
                protocol_version: ProtocolVersionId::latest(),
                consensus: None,
            }
        });

        self.l2_blocks.extend(l2_blocks);
        tx_hashes
    }
}

#[async_trait]
impl MainNodeClient for MockMainNodeClient {
    async fn fetch_system_contract_by_hash(
        &self,
        _hash: H256,
    ) -> anyhow::Result<SystemContractCode> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_genesis_contract_bytecode(
        &self,
        _address: Address,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_protocol_version(
        &self,
        _protocol_version: ProtocolVersionId,
    ) -> anyhow::Result<api::ProtocolVersion> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_genesis_l1_batch_hash(&self) -> anyhow::Result<H256> {
        anyhow::bail!("Not implemented");
    }

    async fn fetch_l2_block_number(&self) -> anyhow::Result<MiniblockNumber> {
        if let Some(number) = self.l2_blocks.len().checked_sub(1) {
            Ok(MiniblockNumber(number as u32))
        } else {
            anyhow::bail!("Not implemented");
        }
    }

    async fn fetch_l2_block(
        &self,
        number: MiniblockNumber,
        with_transactions: bool,
    ) -> anyhow::Result<Option<api::en::SyncBlock>> {
        let Some(mut block) = self.l2_blocks.get(number.0 as usize).cloned() else {
            return Ok(None);
        };
        if !with_transactions {
            block.transactions = None;
        }
        Ok(Some(block))
    }
}

fn open_l1_batch(number: u32, timestamp: u64, first_miniblock_number: u32) -> SyncAction {
    SyncAction::OpenBatch {
        number: L1BatchNumber(number),
        timestamp,
        l1_gas_price: 2,
        l2_fair_gas_price: 3,
        operator_address: Default::default(),
        protocol_version: ProtocolVersionId::latest(),
        first_miniblock_info: (MiniblockNumber(first_miniblock_number), 1),
        prev_miniblock_hash: H256::default(),
    }
}

#[derive(Debug)]
struct StateKeeperHandles {
    actions_sender: ActionQueueSender,
    stop_sender: watch::Sender<bool>,
    sync_state: SyncState,
    task: JoinHandle<anyhow::Result<()>>,
}

impl StateKeeperHandles {
    async fn wait(self, mut condition: impl FnMut(&SyncState) -> bool) {
        let started_at = Instant::now();
        loop {
            assert!(
                started_at.elapsed() <= TEST_TIMEOUT,
                "Timed out waiting for miniblock to be sealed"
            );
            if self.task.is_finished() {
                match self.task.await {
                    Err(err) => panic!("State keeper panicked: {}", err),
                    Ok(Err(err)) => panic!("State keeper finished with an error: {}", err),
                    Ok(Ok(())) => unreachable!(),
                }
            }
            if condition(&self.sync_state) {
                break;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        self.stop_sender.send_replace(true);
        self.task.await.unwrap().unwrap();
    }
}

async fn ensure_genesis(storage: &mut StorageProcessor<'_>) {
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        ensure_genesis_state(storage, L2ChainId::default(), &GenesisParams::mock())
            .await
            .unwrap();
    }
}

/// `tx_hashes` are grouped by the L1 batch.
async fn run_state_keeper(pool: ConnectionPool, tx_hashes: &[&[H256]]) -> StateKeeperHandles {
    assert!(!tx_hashes.is_empty());
    assert!(tx_hashes.iter().all(|tx_hashes| !tx_hashes.is_empty()));

    ensure_genesis(&mut pool.access_storage().await.unwrap()).await;

    let (actions_sender, actions) = ActionQueue::new();
    let sync_state = SyncState::new();
    let io = ExternalIO::new(
        pool,
        actions,
        sync_state.clone(),
        Box::<MockMainNodeClient>::default(),
        Address::repeat_byte(1),
        u32::MAX,
        L2ChainId::default(),
    )
    .await;

    let (stop_sender, stop_receiver) = watch::channel(false);
    let mut batch_executor_base = TestBatchExecutorBuilder::default();
    for &tx_hashes_in_l1_batch in tx_hashes {
        batch_executor_base.push_successful_transactions(tx_hashes_in_l1_batch);
    }

    let state_keeper = ZkSyncStateKeeper::without_sealer(
        stop_receiver,
        Box::new(io),
        Box::new(batch_executor_base),
    );
    StateKeeperHandles {
        actions_sender,
        stop_sender,
        sync_state,
        task: tokio::spawn(state_keeper.run()),
    }
}

fn extract_tx_hashes<'a>(actions: impl IntoIterator<Item = &'a SyncAction>) -> Vec<H256> {
    actions
        .into_iter()
        .filter_map(|action| {
            if let SyncAction::Tx(tx) = action {
                Some(tx.hash())
            } else {
                None
            }
        })
        .collect()
}

#[tokio::test]
async fn external_io_basics() {
    let pool = ConnectionPool::test_pool().await;
    let open_l1_batch = open_l1_batch(1, 1, 1);
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = SyncAction::Tx(Box::new(tx.into()));
    let actions = vec![open_l1_batch, tx, SyncAction::SealMiniblock];

    let state_keeper = run_state_keeper(pool.clone(), &[&extract_tx_hashes(&actions)]).await;
    state_keeper.actions_sender.push_actions(actions).await;
    // Wait until the miniblock is sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(1))
        .await;

    // Check that the miniblock is persisted.
    let mut storage = pool.access_storage().await.unwrap();
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(1))
        .await
        .unwrap()
        .expect("Miniblock #1 is not persisted");
    assert_eq!(miniblock.timestamp, 1);
    assert_eq!(miniblock.l1_gas_price, 2);
    assert_eq!(miniblock.l2_fair_gas_price, 3);
    assert_eq!(miniblock.l1_tx_count, 0);
    assert_eq!(miniblock.l2_tx_count, 1);

    let tx_receipt = storage
        .transactions_web3_dal()
        .get_transaction_receipt(tx_hash)
        .await
        .unwrap()
        .expect("Transaction not persisted");
    assert_eq!(tx_receipt.block_number, Some(1.into()));
    assert_eq!(tx_receipt.transaction_index, 0.into());
}

async fn run_state_keeper_with_multiple_miniblocks(pool: ConnectionPool) -> Vec<H256> {
    let open_l1_batch = open_l1_batch(1, 1, 1);
    let txs = (0..5).map(|_| {
        let tx = create_l2_transaction(10, 100);
        SyncAction::Tx(Box::new(tx.into()))
    });
    let first_miniblock_actions: Vec<_> = iter::once(open_l1_batch)
        .chain(txs)
        .chain([SyncAction::SealMiniblock])
        .collect();

    let open_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(2),
        timestamp: 2,
        virtual_blocks: 1,
    };
    let more_txs = (0..3).map(|_| {
        let tx = create_l2_transaction(10, 100);
        SyncAction::Tx(Box::new(tx.into()))
    });
    let second_miniblock_actions: Vec<_> = iter::once(open_miniblock)
        .chain(more_txs)
        .chain([SyncAction::SealMiniblock])
        .collect();

    let tx_hashes = extract_tx_hashes(
        first_miniblock_actions
            .iter()
            .chain(&second_miniblock_actions),
    );
    let state_keeper = run_state_keeper(pool, &[&tx_hashes]).await;
    state_keeper
        .actions_sender
        .push_actions(first_miniblock_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(second_miniblock_actions)
        .await;
    // Wait until both miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(2))
        .await;
    tx_hashes
}

#[tokio::test]
async fn external_io_with_multiple_miniblocks() {
    let pool = ConnectionPool::test_pool().await;
    let tx_hashes = run_state_keeper_with_multiple_miniblocks(pool.clone()).await;
    assert_eq!(tx_hashes.len(), 8);

    // Check that both miniblocks are persisted.
    let tx_hashes_by_miniblock = [(1, &tx_hashes[..5]), (2, &tx_hashes[5..])];
    let mut storage = pool.access_storage().await.unwrap();
    for (number, expected_tx_hashes) in tx_hashes_by_miniblock {
        let miniblock = storage
            .blocks_dal()
            .get_miniblock_header(MiniblockNumber(number))
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Miniblock #{} is not persisted", number));
        assert_eq!(miniblock.l2_tx_count, expected_tx_hashes.len() as u16);
        assert_eq!(miniblock.timestamp, u64::from(number));

        let sync_block = storage
            .sync_dal()
            .sync_block(MiniblockNumber(number), Address::repeat_byte(1), true)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Sync block #{} is not persisted", number));

        let transactions = sync_block.transactions.unwrap();
        assert_eq!(transactions.len(), expected_tx_hashes.len());
        let tx_hashes: Vec<_> = transactions.iter().map(Transaction::hash).collect();
        assert_eq!(tx_hashes, expected_tx_hashes);
    }
    drop(storage);

    test_external_io_recovery(pool, tx_hashes).await;
}

async fn test_external_io_recovery(pool: ConnectionPool, mut tx_hashes: Vec<H256>) {
    let new_tx = create_l2_transaction(10, 100);
    tx_hashes.push(new_tx.hash());
    let new_tx = SyncAction::Tx(Box::new(new_tx.into()));

    let state_keeper = run_state_keeper(pool.clone(), &[&tx_hashes]).await;
    // Check that the state keeper state is restored.
    assert_eq!(
        state_keeper.sync_state.get_local_block(),
        MiniblockNumber(2)
    );

    // Send new actions and wait until the new miniblock is sealed.
    let open_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(3),
        timestamp: 3,
        virtual_blocks: 1,
    };
    let actions = vec![open_miniblock, new_tx, SyncAction::SealMiniblock];
    state_keeper.actions_sender.push_actions(actions).await;
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(3))
        .await;

    let mut storage = pool.access_storage().await.unwrap();
    let miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(3))
        .await
        .unwrap()
        .expect("Miniblock #3 is not persisted");
    assert_eq!(miniblock.l2_tx_count, 1);
    assert_eq!(miniblock.timestamp, 3);
}

async fn mock_l1_batch_hash_computation(pool: ConnectionPool, number: u32) {
    loop {
        let mut storage = pool.access_storage().await.unwrap();
        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        if last_l1_batch_number < L1BatchNumber(number) {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        let metadata = create_l1_batch_metadata(number);
        storage
            .blocks_dal()
            .save_l1_batch_metadata(L1BatchNumber(1), &metadata, H256::zero(), false)
            .await
            .unwrap();
        break;
    }
}

#[tokio::test]
async fn external_io_with_multiple_l1_batches() {
    let pool = ConnectionPool::test_pool().await;
    let l1_batch = open_l1_batch(1, 1, 1);
    let first_tx = create_l2_transaction(10, 100);
    let first_tx_hash = first_tx.hash();
    let first_tx = SyncAction::Tx(Box::new(first_tx.into()));
    let first_l1_batch_actions = vec![l1_batch, first_tx, SyncAction::SealMiniblock];

    let fictive_miniblock = SyncAction::Miniblock {
        number: MiniblockNumber(2),
        timestamp: 2,
        virtual_blocks: 0,
    };
    let seal_l1_batch = SyncAction::SealBatch { virtual_blocks: 0 };
    let fictive_miniblock_actions = vec![fictive_miniblock, seal_l1_batch];

    let l1_batch = open_l1_batch(2, 3, 3);
    let second_tx = create_l2_transaction(10, 100);
    let second_tx_hash = second_tx.hash();
    let second_tx = SyncAction::Tx(Box::new(second_tx.into()));
    let second_l1_batch_actions = vec![l1_batch, second_tx, SyncAction::SealMiniblock];

    let state_keeper = run_state_keeper(pool.clone(), &[&[first_tx_hash], &[second_tx_hash]]).await;
    state_keeper
        .actions_sender
        .push_actions(first_l1_batch_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(fictive_miniblock_actions)
        .await;
    state_keeper
        .actions_sender
        .push_actions(second_l1_batch_actions)
        .await;

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(pool.clone(), 1));
    // Wait until the miniblocks are sealed.
    state_keeper
        .wait(|state| state.get_local_block() == MiniblockNumber(3))
        .await;
    hash_task.await.unwrap();

    let mut storage = pool.access_storage().await.unwrap();
    let l1_batch_header = storage
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("L1 batch #1 is not persisted");
    assert_eq!(l1_batch_header.timestamp, 1);
    assert_eq!(l1_batch_header.l2_tx_count, 1);

    let (first_miniblock, last_miniblock) = storage
        .blocks_dal()
        .get_miniblock_range_of_l1_batch(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("Miniblock range for L1 batch #1 is not persisted");
    assert_eq!(first_miniblock, MiniblockNumber(1));
    assert_eq!(last_miniblock, MiniblockNumber(2));

    let fictive_miniblock = storage
        .blocks_dal()
        .get_miniblock_header(MiniblockNumber(2))
        .await
        .unwrap()
        .expect("Fictive miniblock #2 is not persisted");
    assert_eq!(fictive_miniblock.timestamp, 2);
    assert_eq!(fictive_miniblock.l2_tx_count, 0);
}

#[tokio::test]
async fn fetcher_basics() {
    let pool = ConnectionPool::test_pool().await;
    let mut storage = pool.access_storage().await.unwrap();
    ensure_genesis(&mut storage).await;
    let fetcher_cursor = MainNodeFetcherCursor::new(&mut storage).await.unwrap();
    assert_eq!(fetcher_cursor.l1_batch, L1BatchNumber(0));
    assert_eq!(fetcher_cursor.miniblock, MiniblockNumber(1));
    drop(storage);

    let mut mock_client = MockMainNodeClient::default();
    mock_client.push_l1_batch(0);
    // ^ The genesis L1 batch will not be queried, so we're OK with filling it with non-authentic data
    let mut tx_hashes = VecDeque::from(mock_client.push_l1_batch(1));
    tx_hashes.extend(mock_client.push_l1_batch(2));

    let (actions_sender, mut actions) = ActionQueue::new();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let sync_state = SyncState::default();
    let fetcher = fetcher_cursor.into_fetcher(
        Box::new(mock_client),
        actions_sender,
        sync_state.clone(),
        stop_receiver,
    );
    let fetcher_task = tokio::spawn(fetcher.run());

    // Check that sync_state is updated.
    while sync_state.get_main_node_block() < MiniblockNumber(5) {
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    // Check generated actions. Some basic checks are performed by `ActionQueueSender`.
    let mut current_l1_batch_number = L1BatchNumber(0);
    let mut current_miniblock_number = MiniblockNumber(0);
    let mut tx_count_in_miniblock = 0;
    let started_at = Instant::now();
    loop {
        assert!(
            started_at.elapsed() <= TEST_TIMEOUT,
            "Timed out waiting for fetcher"
        );
        let Some(action) = actions.pop_action() else {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        };
        match action {
            SyncAction::OpenBatch { number, .. } => {
                current_l1_batch_number += 1;
                current_miniblock_number += 1; // First miniblock is implicitly opened
                tx_count_in_miniblock = 0;
                assert_eq!(number, current_l1_batch_number);
            }
            SyncAction::Miniblock { number, .. } => {
                current_miniblock_number += 1;
                tx_count_in_miniblock = 0;
                assert_eq!(number, current_miniblock_number);
            }
            SyncAction::SealBatch { virtual_blocks } => {
                assert_eq!(virtual_blocks, 0);
                assert_eq!(tx_count_in_miniblock, 0);
                if current_miniblock_number == MiniblockNumber(5) {
                    break;
                }
            }
            SyncAction::Tx(tx) => {
                assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                tx_count_in_miniblock += 1;
            }
            SyncAction::SealMiniblock => {
                assert_eq!(tx_count_in_miniblock, 1);
            }
        }
    }

    stop_sender.send_replace(true);
    fetcher_task.await.unwrap().unwrap();
}

#[tokio::test]
async fn fetcher_with_real_server() {
    let pool = ConnectionPool::test_pool().await;
    // Fill in transactions grouped in multiple miniblocks in the storage.
    let tx_hashes = run_state_keeper_with_multiple_miniblocks(pool.clone()).await;
    let mut tx_hashes = VecDeque::from(tx_hashes);

    // Start the API server.
    let network_config = NetworkConfig::for_tests();
    let (stop_sender, stop_receiver) = watch::channel(false);
    let server_handles =
        spawn_http_server(&network_config, pool.clone(), stop_receiver.clone()).await;
    server_handles.wait_until_ready().await;
    let server_addr = &server_handles.local_addr;

    // Start the fetcher connected to the API server.
    let sync_state = SyncState::default();
    let (actions_sender, mut actions) = ActionQueue::new();
    let client = <dyn MainNodeClient>::json_rpc(&format!("http://{server_addr}/")).unwrap();
    let fetcher_cursor = MainNodeFetcherCursor {
        miniblock: MiniblockNumber(1),
        l1_batch: L1BatchNumber(0),
    };
    let fetcher = fetcher_cursor.into_fetcher(
        Box::new(client),
        actions_sender,
        sync_state.clone(),
        stop_receiver,
    );
    let fetcher_task = tokio::spawn(fetcher.run());

    // Check generated actions.
    let mut current_miniblock_number = MiniblockNumber(0);
    let mut tx_count_in_miniblock = 0;
    let miniblock_number_to_tx_count = HashMap::from([(1, 5), (2, 3)]);
    let started_at = Instant::now();
    loop {
        assert!(
            started_at.elapsed() <= TEST_TIMEOUT,
            "Timed out waiting for fetcher actions"
        );
        let Some(action) = actions.pop_action() else {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        };
        match action {
            SyncAction::OpenBatch {
                number,
                first_miniblock_info,
                ..
            } => {
                assert_eq!(number, L1BatchNumber(1));
                current_miniblock_number += 1; // First miniblock is implicitly opened
                tx_count_in_miniblock = 0;
                assert_eq!(first_miniblock_info.0, current_miniblock_number);
            }
            SyncAction::SealBatch { .. } => unreachable!("L1 batches are not sealed in test"),
            SyncAction::Miniblock { number, .. } => {
                current_miniblock_number += 1;
                tx_count_in_miniblock = 0;
                assert_eq!(number, current_miniblock_number);
            }
            SyncAction::Tx(tx) => {
                assert_eq!(tx.hash(), tx_hashes.pop_front().unwrap());
                tx_count_in_miniblock += 1;
            }
            SyncAction::SealMiniblock => {
                assert_eq!(
                    tx_count_in_miniblock,
                    miniblock_number_to_tx_count[&current_miniblock_number]
                );
                if current_miniblock_number == MiniblockNumber(2) {
                    break;
                }
            }
        }
    }

    stop_sender.send_replace(true);
    fetcher_task.await.unwrap().unwrap();
    server_handles.shutdown().await;
}
