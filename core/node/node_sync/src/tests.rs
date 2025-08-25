//! High-level sync layer tests.

use std::{iter, sync::Arc, time::Duration};

use backon::{ConstantBuilder, Retryable};
use test_casing::test_casing;
use tokio::{sync::watch, task::JoinHandle};
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
use zksync_node_test_utils::{
    create_l1_batch_metadata, create_l2_transaction, prepare_recovery_snapshot,
};
use zksync_shared_resources::api::SyncState;
use zksync_state_keeper::{
    io::{L1BatchParams, L2BlockParams},
    seal_criteria::NoopSealer,
    testonly::test_batch_executor::{MockReadStorageFactory, TestBatchExecutorBuilder},
    OutputHandler, StateKeeperBuilder, StateKeeperPersistence, TreeWritesPersistence,
};
use zksync_types::{
    api,
    block::{L2BlockHasher, UnsealedL1BatchHeader},
    fee_model::{BatchFeeInput, PubdataIndependentBatchFeeModelInput},
    settlement::SettlementLayer,
    snapshots::SnapshotRecoveryStatus,
    Address, L1BatchNumber, L2BlockNumber, L2ChainId, ProtocolVersionId, Transaction, H256,
};

use super::{
    fetcher::FetchedTransaction, sync_action::SyncAction, testonly::MockMainNodeClient, *,
};

const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
pub(crate) const OPERATOR_ADDRESS: Address = Address::repeat_byte(1);

fn open_l1_batch(number: u32, timestamp: u64, first_l2_block_number: u32) -> SyncAction {
    SyncAction::OpenBatch {
        params: L1BatchParams {
            protocol_version: ProtocolVersionId::latest(),
            validation_computational_gas_limit: u32::MAX,
            operator_address: OPERATOR_ADDRESS,
            fee_input: BatchFeeInput::pubdata_independent(2, 3, 4),
            first_l2_block: L2BlockParams::new(timestamp * 1000),
            pubdata_params: Default::default(),
            pubdata_limit: Some(100_000),
        },
        number: L1BatchNumber(number),
        first_l2_block_number: L2BlockNumber(first_l2_block_number),
    }
}

impl MockMainNodeClient {
    pub(crate) fn for_snapshot_recovery(snapshot: &SnapshotRecoveryStatus) -> Self {
        // This block may be requested during node initialization
        let last_l2_block_in_snapshot_batch = api::en::SyncBlock {
            number: snapshot.l2_block_number,
            l1_batch_number: snapshot.l1_batch_number,
            last_in_batch: true,
            timestamp: snapshot.l2_block_timestamp,
            l1_gas_price: 2,
            l2_fair_gas_price: 3,
            fair_pubdata_price: Some(24),
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
            operator_address: Address::repeat_byte(2),
            transactions: Some(vec![]),
            virtual_blocks: Some(0),
            hash: Some(snapshot.l2_block_hash),
            protocol_version: ProtocolVersionId::latest(),
            pubdata_params: Default::default(),
            pubdata_limit: Some(100_000),
            interop_roots: Some(vec![]),
        };

        Self {
            l2_blocks: vec![last_l2_block_in_snapshot_batch],
            block_number_offset: snapshot.l2_block_number.0,
            ..Self::default()
        }
    }

    pub fn insert_protocol_version(&mut self, version: api::ProtocolVersionInfo) {
        self.system_contracts
            .insert(version.bootloader_code_hash, vec![]);
        self.system_contracts
            .insert(version.default_account_code_hash, vec![]);
        self.protocol_versions
            .insert(version.minor_version, version);
    }
}

#[derive(Debug)]
pub(super) struct StateKeeperHandles {
    pub stop_sender: watch::Sender<bool>,
    pub sync_state: SyncState,
    pub task: JoinHandle<anyhow::Result<()>>,
}

impl StateKeeperHandles {
    /// `tx_hashes` are grouped by the L1 batch.
    pub async fn new(
        pool: ConnectionPool<Core>,
        main_node_client: MockMainNodeClient,
        actions: ActionQueue,
        tx_hashes: &[&[H256]],
    ) -> Self {
        assert!(!tx_hashes.is_empty());
        assert!(tx_hashes.iter().all(|tx_hashes| !tx_hashes.is_empty()));

        let sync_state = SyncState::default();
        let (persistence, l2_block_sealer) =
            StateKeeperPersistence::new(pool.clone(), Some(Address::repeat_byte(1)), 5)
                .await
                .unwrap();
        let tree_writes_persistence = TreeWritesPersistence::new(pool.clone());
        let output_handler = OutputHandler::new(Box::new(persistence.with_tx_insertion()))
            .with_handler(Box::new(tree_writes_persistence))
            .with_handler(Box::new(sync_state.clone()));

        tokio::spawn(l2_block_sealer.run());
        let io = ExternalIO::new(
            pool.clone(),
            actions,
            Box::new(main_node_client),
            L2ChainId::default(),
        )
        .unwrap();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let mut batch_executor = TestBatchExecutorBuilder::default();
        for &tx_hashes_in_l1_batch in tx_hashes {
            batch_executor.push_successful_transactions(tx_hashes_in_l1_batch);
        }

        let builder = StateKeeperBuilder::new(
            Box::new(io),
            Box::new(batch_executor),
            output_handler,
            Arc::new(NoopSealer),
            Arc::new(MockReadStorageFactory),
            None,
            SettlementLayer::for_tests(),
        );
        let state_keeper = builder.build(&stop_receiver).await.unwrap();

        Self {
            stop_sender,
            sync_state,
            task: tokio::spawn(state_keeper.run(stop_receiver)),
        }
    }

    /// Waits for the given condition.
    pub async fn wait_for_local_block(mut self, want: L2BlockNumber) {
        let mut sync_state_sub = self.sync_state.subscribe();
        tokio::select! {
            task_result = &mut self.task => {
                match task_result {
                    Err(err) => panic!("State keeper panicked: {err}"),
                    Ok(Err(err)) => panic!("State keeper finished with an error: {err:?}"),
                    Ok(Ok(())) => unreachable!(),
                }
            }
            () = tokio::time::sleep(TEST_TIMEOUT) => {
                panic!("Timed out waiting for L2 block to be sealed");
            }
            _ = sync_state_sub.wait_for(|state| state.local_block() >= Some(want)) => {
                self.stop_sender.send_replace(true);
                self.task.await.unwrap().unwrap();
            }
        }
    }
}

async fn ensure_genesis(storage: &mut Connection<'_, Core>) {
    if storage.blocks_dal().is_genesis_needed().await.unwrap() {
        insert_genesis_batch(storage, &GenesisParams::mock())
            .await
            .unwrap();
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

/// Returns a mock snapshot recovery status equivalent to "recovering" from the genesis block.
fn genesis_snapshot_recovery_status() -> SnapshotRecoveryStatus {
    SnapshotRecoveryStatus {
        l1_batch_number: L1BatchNumber(0),
        l1_batch_root_hash: H256::zero(), // unused
        l1_batch_timestamp: 0,
        l2_block_number: L2BlockNumber(0),
        l2_block_hash: L2BlockHasher::legacy_hash(L2BlockNumber(0)),
        l2_block_timestamp: 0,
        protocol_version: ProtocolVersionId::default(),
        storage_logs_chunks_processed: vec![],
    }
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_basics(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };

    let open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.l2_block_timestamp + 1,
        snapshot.l2_block_number.0 + 1,
    );
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = FetchedTransaction::new(tx.into());
    let actions = vec![open_l1_batch, tx.into(), SyncAction::SealL2Block];

    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        client,
        action_queue,
        &[&extract_tx_hashes(&actions)],
    )
    .await;
    actions_sender.push_actions(actions).await.unwrap();
    // Wait until the L2 block is sealed.
    state_keeper
        .wait_for_local_block(snapshot.l2_block_number + 1)
        .await;

    // Check that the L2 block is persisted.
    let l2_block = storage
        .blocks_dal()
        .get_l2_block_header(snapshot.l2_block_number + 1)
        .await
        .unwrap()
        .expect("New L2 block is not persisted");
    assert_eq!(l2_block.timestamp, snapshot.l2_block_timestamp + 1);

    let expected_fee_input =
        BatchFeeInput::PubdataIndependent(PubdataIndependentBatchFeeModelInput {
            fair_l2_gas_price: 3,
            fair_pubdata_price: 4,
            l1_gas_price: 2,
        });

    assert_eq!(l2_block.batch_fee_input, expected_fee_input);
    assert_eq!(l2_block.l1_tx_count, 0);
    assert_eq!(l2_block.l2_tx_count, 1);

    let tx_receipts = storage
        .transactions_web3_dal()
        .get_transaction_receipts(&[tx_hash])
        .await
        .unwrap();
    let tx_receipt = &tx_receipts
        .first()
        .expect("Transaction not persisted")
        .inner;
    assert_eq!(
        tx_receipt.block_number,
        (snapshot.l2_block_number.0 + 1).into()
    );
    assert_eq!(tx_receipt.transaction_index, 0.into());
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_works_without_local_protocol_version(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };

    let mut open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.l2_block_timestamp + 1,
        snapshot.l2_block_number.0 + 1,
    );
    if let SyncAction::OpenBatch { params, .. } = &mut open_l1_batch {
        params.protocol_version = ProtocolVersionId::next();
    } else {
        unreachable!();
    };

    let tx = create_l2_transaction(10, 100);
    let tx = FetchedTransaction::new(tx.into());
    let actions = vec![open_l1_batch, tx.into(), SyncAction::SealL2Block];

    let (actions_sender, action_queue) = ActionQueue::new();
    let mut client = MockMainNodeClient::default();
    let next_protocol_version = api::ProtocolVersionInfo {
        minor_version: ProtocolVersionId::next() as u16,
        timestamp: snapshot.l2_block_timestamp + 1,
        bootloader_code_hash: H256::repeat_byte(1),
        default_account_code_hash: H256::repeat_byte(1),
        evm_emulator_code_hash: Some(H256::repeat_byte(1)),
        l2_system_upgrade_tx_hash: None,
    };
    client.insert_protocol_version(next_protocol_version.clone());

    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        client,
        action_queue,
        &[&extract_tx_hashes(&actions)],
    )
    .await;
    actions_sender.push_actions(actions).await.unwrap();
    // Wait until the L2 block is sealed.
    state_keeper
        .wait_for_local_block(snapshot.l2_block_number + 1)
        .await;

    // Check that the L2 block and the protocol version for it are persisted.
    let persisted_protocol_version = storage
        .protocol_versions_dal()
        .get_protocol_version_with_latest_patch(ProtocolVersionId::next())
        .await
        .unwrap()
        .expect("next protocol version not persisted");
    assert_eq!(
        persisted_protocol_version.timestamp,
        next_protocol_version.timestamp
    );
    assert_eq!(
        persisted_protocol_version
            .base_system_contracts_hashes
            .bootloader,
        next_protocol_version.bootloader_code_hash
    );
    assert_eq!(
        persisted_protocol_version
            .base_system_contracts_hashes
            .default_aa,
        next_protocol_version.default_account_code_hash
    );

    assert_eq!(
        persisted_protocol_version
            .base_system_contracts_hashes
            .evm_emulator,
        next_protocol_version.evm_emulator_code_hash
    );

    let l2_block = storage
        .blocks_dal()
        .get_l2_block_header(snapshot.l2_block_number + 1)
        .await
        .unwrap()
        .expect("New L2 block is not persisted");
    assert_eq!(l2_block.timestamp, snapshot.l2_block_timestamp + 1);
    assert_eq!(l2_block.protocol_version, Some(ProtocolVersionId::next()));
}

pub(super) async fn run_state_keeper_with_multiple_l2_blocks(
    pool: ConnectionPool<Core>,
    snapshot_recovery: bool,
) -> (SnapshotRecoveryStatus, Vec<H256>) {
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };
    drop(storage);

    let open_l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.l2_block_timestamp + 1,
        snapshot.l2_block_number.0 + 1,
    );
    let txs = (0..5).map(|_| {
        let tx = create_l2_transaction(10, 100);
        FetchedTransaction::new(tx.into()).into()
    });
    let first_l2_block_actions: Vec<_> = iter::once(open_l1_batch)
        .chain(txs)
        .chain([SyncAction::SealL2Block])
        .collect();

    let open_l2_block = SyncAction::L2Block {
        params: L2BlockParams::new((snapshot.l2_block_timestamp + 2) * 1000),
        number: snapshot.l2_block_number + 2,
    };
    let more_txs = (0..3).map(|_| {
        let tx = create_l2_transaction(10, 100);
        FetchedTransaction::new(tx.into()).into()
    });
    let second_l2_block_actions: Vec<_> = iter::once(open_l2_block)
        .chain(more_txs)
        .chain([SyncAction::SealL2Block])
        .collect();

    let tx_hashes = extract_tx_hashes(
        first_l2_block_actions
            .iter()
            .chain(&second_l2_block_actions),
    );
    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper = StateKeeperHandles::new(pool, client, action_queue, &[&tx_hashes]).await;
    actions_sender
        .push_actions(first_l2_block_actions)
        .await
        .unwrap();
    actions_sender
        .push_actions(second_l2_block_actions)
        .await
        .unwrap();
    // Wait until both L2 blocks are sealed.
    state_keeper
        .wait_for_local_block(snapshot.l2_block_number + 2)
        .await;
    (snapshot, tx_hashes)
}

#[test_casing(2, [false, true])]
#[tokio::test]
async fn external_io_with_multiple_l2_blocks(snapshot_recovery: bool) {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let (snapshot, tx_hashes) =
        run_state_keeper_with_multiple_l2_blocks(pool.clone(), snapshot_recovery).await;
    assert_eq!(tx_hashes.len(), 8);

    // Check that both L2 blocks are persisted.
    let tx_hashes_by_l2_block = [
        (snapshot.l2_block_number + 1, &tx_hashes[..5]),
        (snapshot.l2_block_number + 2, &tx_hashes[5..]),
    ];
    let mut storage = pool.connection().await.unwrap();
    for (number, expected_tx_hashes) in tx_hashes_by_l2_block {
        let l2_block = storage
            .blocks_dal()
            .get_l2_block_header(number)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("L2 block #{number} is not persisted"));
        assert_eq!(l2_block.l2_tx_count, expected_tx_hashes.len() as u16);
        assert_eq!(l2_block.timestamp, u64::from(number.0));

        let sync_block = storage
            .sync_dal()
            .sync_block(number, true)
            .await
            .unwrap()
            .unwrap_or_else(|| panic!("Sync block #{number} is not persisted"));

        let transactions = sync_block.transactions.unwrap();
        assert_eq!(transactions.len(), expected_tx_hashes.len());
        let tx_hashes: Vec<_> = transactions.iter().map(Transaction::hash).collect();
        assert_eq!(tx_hashes, expected_tx_hashes);
    }
    drop(storage);

    test_external_io_recovery(pool, &snapshot, tx_hashes).await;
}

async fn test_external_io_recovery(
    pool: ConnectionPool<Core>,
    snapshot: &SnapshotRecoveryStatus,
    mut tx_hashes: Vec<H256>,
) {
    let new_tx = create_l2_transaction(10, 100);
    tx_hashes.push(new_tx.hash());
    let new_tx = FetchedTransaction::new(new_tx.into());

    let (actions_sender, action_queue) = ActionQueue::new();
    let client = if snapshot.l1_batch_number > L1BatchNumber(0) {
        MockMainNodeClient::for_snapshot_recovery(snapshot)
    } else {
        MockMainNodeClient::default()
    };

    let state_keeper =
        StateKeeperHandles::new(pool.clone(), client, action_queue, &[&tx_hashes]).await;
    // Check that the state keeper state is restored.
    state_keeper
        .sync_state
        .subscribe()
        .wait_for(|state| state.local_block() >= Some(snapshot.l2_block_number + 2))
        .await
        .unwrap();

    // Send new actions and wait until the new L2 block is sealed.
    let open_l2_block = SyncAction::L2Block {
        params: L2BlockParams::new((snapshot.l2_block_timestamp + 3) * 1000),
        number: snapshot.l2_block_number + 3,
    };
    let actions = vec![open_l2_block, new_tx.into(), SyncAction::SealL2Block];
    actions_sender.push_actions(actions).await.unwrap();
    state_keeper
        .wait_for_local_block(snapshot.l2_block_number + 3)
        .await;

    let mut storage = pool.connection().await.unwrap();
    let l2_block = storage
        .blocks_dal()
        .get_l2_block_header(snapshot.l2_block_number + 3)
        .await
        .unwrap()
        .expect("New L2 block is not persisted");
    assert_eq!(l2_block.l2_tx_count, 1);
    assert_eq!(l2_block.timestamp, snapshot.l2_block_timestamp + 3);
}

pub(super) async fn mock_l1_batch_hash_computation(pool: ConnectionPool<Core>, number: u32) {
    loop {
        let mut storage = pool.connection().await.unwrap();
        let last_l1_batch_number = storage
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await
            .unwrap();
        if last_l1_batch_number < Some(L1BatchNumber(number)) {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        let metadata = create_l1_batch_metadata(number);
        storage
            .blocks_dal()
            .save_l1_batch_tree_data(L1BatchNumber(number), &metadata.tree_data())
            .await
            .unwrap();
        break;
    }
}

/// Returns tx hashes of all generated transactions, grouped by the L1 batch.
pub(super) async fn run_state_keeper_with_multiple_l1_batches(
    pool: ConnectionPool<Core>,
    snapshot_recovery: bool,
) -> (SnapshotRecoveryStatus, Vec<Vec<H256>>) {
    let mut storage = pool.connection().await.unwrap();
    let snapshot = if snapshot_recovery {
        prepare_recovery_snapshot(&mut storage, L1BatchNumber(23), L2BlockNumber(42), &[]).await
    } else {
        ensure_genesis(&mut storage).await;
        genesis_snapshot_recovery_status()
    };
    drop(storage);

    let l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 1,
        snapshot.l2_block_timestamp + 1,
        snapshot.l2_block_number.0 + 1,
    );
    let first_tx = create_l2_transaction(10, 100);
    let first_tx_hash = first_tx.hash();
    let first_tx = FetchedTransaction::new(first_tx.into());
    let first_l1_batch_actions = vec![l1_batch, first_tx.into(), SyncAction::SealL2Block];

    let fictive_l2_block = SyncAction::L2Block {
        params: L2BlockParams::new((snapshot.l2_block_timestamp + 2) * 1000),
        number: snapshot.l2_block_number + 2,
    };
    let fictive_l2_block_actions = vec![fictive_l2_block, SyncAction::SealBatch];

    let l1_batch = open_l1_batch(
        snapshot.l1_batch_number.0 + 2,
        snapshot.l2_block_timestamp + 3,
        snapshot.l2_block_number.0 + 3,
    );
    let second_tx = create_l2_transaction(10, 100);
    let second_tx_hash = second_tx.hash();
    let second_tx = FetchedTransaction::new(second_tx.into());
    let second_l1_batch_actions = vec![l1_batch, second_tx.into(), SyncAction::SealL2Block];

    let (actions_sender, action_queue) = ActionQueue::new();
    let state_keeper = StateKeeperHandles::new(
        pool.clone(),
        MockMainNodeClient::default(),
        action_queue,
        &[&[first_tx_hash], &[second_tx_hash]],
    )
    .await;
    actions_sender
        .push_actions(first_l1_batch_actions)
        .await
        .unwrap();
    actions_sender
        .push_actions(fictive_l2_block_actions)
        .await
        .unwrap();
    actions_sender
        .push_actions(second_l1_batch_actions)
        .await
        .unwrap();

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(
        pool.clone(),
        snapshot.l1_batch_number.0 + 1,
    ));
    // Wait until the L2 blocks are sealed.
    state_keeper
        .wait_for_local_block(snapshot.l2_block_number + 3)
        .await;
    hash_task.await.unwrap();

    (snapshot, vec![vec![first_tx_hash], vec![second_tx_hash]])
}

#[tokio::test]
async fn external_io_with_multiple_l1_batches() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    run_state_keeper_with_multiple_l1_batches(pool.clone(), false).await;

    let mut storage = pool.connection().await.unwrap();
    let l1_batch_header = storage
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("L1 batch #1 is not persisted");
    assert_eq!(l1_batch_header.timestamp, 1);
    assert_eq!(l1_batch_header.l2_tx_count, 1);

    let (first_l2_block, last_l2_block) = storage
        .blocks_dal()
        .get_l2_block_range_of_l1_batch(L1BatchNumber(1))
        .await
        .unwrap()
        .expect("L2 block range for L1 batch #1 is not persisted");
    assert_eq!(first_l2_block, L2BlockNumber(1));
    assert_eq!(last_l2_block, L2BlockNumber(2));

    let fictive_l2_block = storage
        .blocks_dal()
        .get_l2_block_header(L2BlockNumber(2))
        .await
        .unwrap()
        .expect("Fictive L2 block #2 is not persisted");
    assert_eq!(fictive_l2_block.timestamp, 2);
    assert_eq!(fictive_l2_block.l2_tx_count, 0);
}

async fn wait_for_batch_to_be_open(
    pool: &ConnectionPool<Core>,
    number: L1BatchNumber,
) -> anyhow::Result<UnsealedL1BatchHeader> {
    (|| async {
        let mut storage = pool.connection().await.unwrap();
        let unsealed_batch = storage.blocks_dal().get_unsealed_l1_batch().await?;

        if let Some(unsealed_batch) = unsealed_batch {
            if unsealed_batch.number == number {
                Ok(unsealed_batch)
            } else {
                Err(anyhow::anyhow!("L1 batch #{number} is not open yet"))
            }
        } else {
            Err(anyhow::anyhow!("No unsealed L1 batch found yet"))
        }
    })
    .retry(
        &ConstantBuilder::default()
            .with_delay(Duration::from_millis(200))
            .with_max_times(20),
    )
    .await
}

#[tokio::test]
async fn external_io_empty_unsealed_batch() {
    let pool = ConnectionPool::<Core>::test_pool().await;
    let mut storage = pool.connection().await.unwrap();
    ensure_genesis(&mut storage).await;
    drop(storage);

    let open_batch_one = open_l1_batch(1, 1, 1);
    let tx = create_l2_transaction(10, 100);
    let tx_hash = tx.hash();
    let tx = FetchedTransaction::new(tx.into());
    let open_batch_two = open_l1_batch(2, 2, 3);
    let fictive_l2_block = SyncAction::L2Block {
        params: L2BlockParams::new(2000),
        number: L2BlockNumber(2),
    };
    let actions1 = vec![open_batch_one, tx.into(), SyncAction::SealL2Block];
    let actions2 = vec![fictive_l2_block, SyncAction::SealBatch];

    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper =
        StateKeeperHandles::new(pool.clone(), client, action_queue, &[&[tx_hash]]).await;
    actions_sender.push_actions(actions1).await.unwrap();
    actions_sender.push_actions(actions2).await.unwrap();
    // Unchecked insert of batch #2 to simulate restart in the middle of processing an action sequence
    // In other words batch #2 is inserted completely empty with no blocks/txs present in it
    actions_sender
        .push_action_unchecked(open_batch_two.clone())
        .await
        .unwrap();
    // Wait until the L2 block is sealed.
    state_keeper.wait_for_local_block(L2BlockNumber(2)).await;

    // Wait until L1 batch #2 is opened and persisted.
    let unsealed_batch = wait_for_batch_to_be_open(&pool, L1BatchNumber(2))
        .await
        .unwrap();
    assert_eq!(unsealed_batch.number, L1BatchNumber(2));
    assert_eq!(unsealed_batch.timestamp, 2);

    // Prepare the rest of batch #2
    let tx = create_l2_transaction(20, 200);
    let tx_hash = tx.hash();
    let tx = FetchedTransaction::new(tx.into());
    let fictive_l2_block = SyncAction::L2Block {
        params: L2BlockParams::new(4000),
        number: L2BlockNumber(4),
    };
    let actions1 = vec![open_batch_two, tx.into(), SyncAction::SealL2Block];
    let actions2 = vec![fictive_l2_block, SyncAction::SealBatch];

    // Restart state keeper
    let (actions_sender, action_queue) = ActionQueue::new();
    let client = MockMainNodeClient::default();
    let state_keeper =
        StateKeeperHandles::new(pool.clone(), client, action_queue, &[&[tx_hash]]).await;
    actions_sender.push_actions(actions1).await.unwrap();
    actions_sender.push_actions(actions2).await.unwrap();

    let hash_task = tokio::spawn(mock_l1_batch_hash_computation(pool.clone(), 1));
    // Wait until the block #4 is sealed.
    state_keeper.wait_for_local_block(L2BlockNumber(4)).await;
    hash_task.await.unwrap();
}
