use assert_matches::assert_matches;
use db_test_macro::db_test;
use tempfile::TempDir;
use tokio::sync::{mpsc, watch};

use std::{
    future::Future,
    ops, panic,
    path::Path,
    time::{Duration, Instant},
};

use zksync_config::configs::chain::{NetworkConfig, OperationsManagerConfig};

use zksync_config::DBConfig;
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_health_check::{CheckHealth, CheckHealthStatus};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::{
    block::{BlockGasCount, L1BatchHeader, MiniblockHeader},
    commitment::BlockCommitment,
    proofs::PrepareBasicCircuitsJob,
    AccountTreeId, Address, L1BatchNumber, L2ChainId, MiniblockNumber, StorageKey, StorageLog,
    H256,
};
use zksync_utils::{miniblock_hash, u32_to_h256};

use super::{MetadataCalculator, MetadataCalculatorConfig, MetadataCalculatorModeConfig};
use crate::genesis::{create_genesis_block, save_genesis_block_metadata};

const RUN_TIMEOUT: Duration = Duration::from_secs(15);

async fn run_with_timeout<T, F>(timeout: Duration, action: F) -> T
where
    F: Future<Output = T>,
{
    let timeout_handle = tokio::time::timeout(timeout, action);
    match timeout_handle.await {
        Ok(res) => res,
        Err(_) => panic!("timed out waiting for metadata calculator"),
    }
}

#[db_test]
async fn genesis_creation(pool: ConnectionPool, prover_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    assert!(calculator.tree_tag().starts_with("full"));
    run_calculator(calculator, pool.clone(), prover_pool).await;
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    assert_eq!(calculator.updater.tree().block_number(), 1);
}

#[db_test]
async fn basic_workflow(pool: ConnectionPool, prover_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, object_store) = setup_calculator(temp_dir.path(), &pool).await;
    reset_db_state(&pool, 1).await;
    run_calculator(calculator, pool.clone(), prover_pool).await;

    let job: PrepareBasicCircuitsJob = object_store.get(L1BatchNumber(1)).await.unwrap();
    assert!(job.next_enumeration_index() > 0);
    let merkle_paths: Vec<_> = job.clone().into_merkle_paths().collect();
    assert!(!merkle_paths.is_empty() && merkle_paths.len() <= 100);
    // ^ The exact values depend on ops in genesis block
    assert!(merkle_paths.iter().all(|log| log.is_write));

    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    assert_eq!(calculator.updater.tree().block_number(), 2);
}

#[db_test]
async fn status_receiver_has_correct_states(pool: ConnectionPool, prover_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    let tree_health_check = calculator.tree_health_check();
    assert_matches!(
        tree_health_check.check_health().await,
        CheckHealthStatus::NotReady(msg) if msg.contains("full")
    );
    let other_tree_health_check = calculator.tree_health_check();
    assert_matches!(
        other_tree_health_check.check_health().await,
        CheckHealthStatus::NotReady(msg) if msg.contains("full")
    );
    reset_db_state(&pool, 1).await;
    run_calculator(calculator, pool, prover_pool).await;
    assert_eq!(
        tree_health_check.check_health().await,
        CheckHealthStatus::Ready
    );
    assert_eq!(
        other_tree_health_check.check_health().await,
        CheckHealthStatus::Ready
    );
}

#[db_test]
async fn multi_block_workflow(pool: ConnectionPool, prover_pool: ConnectionPool) {
    // Run all transactions as a single block
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    reset_db_state(&pool, 1).await;
    let root_hash = run_calculator(calculator, pool.clone(), prover_pool.clone()).await;

    // Run the same transactions as multiple blocks
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, object_store) = setup_calculator(temp_dir.path(), &pool).await;
    reset_db_state(&pool, 10).await;
    let multi_block_root_hash = run_calculator(calculator, pool, prover_pool).await;
    assert_eq!(multi_block_root_hash, root_hash);

    let mut prev_index = None;
    for block_number in 1..=10 {
        let block_number = L1BatchNumber(block_number);
        let job: PrepareBasicCircuitsJob = object_store.get(block_number).await.unwrap();
        let next_enumeration_index = job.next_enumeration_index();
        let merkle_paths: Vec<_> = job.into_merkle_paths().collect();
        assert!(!merkle_paths.is_empty() && merkle_paths.len() <= 10);

        if let Some(prev_index) = prev_index {
            assert_eq!(next_enumeration_index, prev_index + 1);
        }
        let max_leaf_index_in_block = merkle_paths
            .iter()
            .filter_map(|log| log.first_write.then_some(log.leaf_enumeration_index))
            .max();
        prev_index = max_leaf_index_in_block.or(prev_index);
    }
}

#[db_test]
async fn running_metadata_calculator_with_additional_blocks(
    pool: ConnectionPool,
    prover_pool: ConnectionPool,
) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let calculator = setup_lightweight_calculator(temp_dir.path(), &pool).await;
    reset_db_state(&pool, 5).await;
    run_calculator(calculator, pool.clone(), prover_pool.clone()).await;

    let mut calculator = setup_lightweight_calculator(temp_dir.path(), &pool).await;
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;

    let calculator_handle = {
        let pool = pool.clone();
        let prover_pool = prover_pool.clone();
        tokio::task::spawn(calculator.run(pool, prover_pool, stop_rx))
    };
    // Wait until the calculator has processed initial blocks.
    let (block_count, _) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
        .await
        .expect("metadata calculator timed out processing initial blocks")
        .unwrap();
    assert_eq!(block_count, 6);

    // Add some new blocks to the storage.
    let new_logs = gen_storage_logs(100..200, 10);
    extend_db_state(
        &mut pool.access_storage_tagged("metadata_calculator").await,
        new_logs,
    )
    .await;

    // Wait until these blocks are processed. The calculator may have spurious delays,
    // thus we wait in a loop.
    let updated_root_hash = loop {
        let (block_count, root_hash) = tokio::time::timeout(RUN_TIMEOUT, delay_rx.recv())
            .await
            .expect("metadata calculator shut down prematurely")
            .unwrap();
        if block_count == 16 {
            stop_sx.send(true).unwrap(); // Shut down the calculator.
            break root_hash;
        }
    };
    tokio::time::timeout(RUN_TIMEOUT, calculator_handle)
        .await
        .expect("timed out waiting for calculator")
        .unwrap();

    // Switch to the full tree. It should pick up from the same spot and result in the same tree root hash.
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool).await;
    let root_hash_for_full_tree = run_calculator(calculator, pool, prover_pool).await;
    assert_eq!(root_hash_for_full_tree, updated_root_hash);
}

#[db_test]
async fn throttling_tree(pool: ConnectionPool, prover_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (mut db_config, operation_config, eth) = create_config(temp_dir.path());
    db_config.new_merkle_tree_throttle_ms = 100;
    let mut calculator = setup_calculator_with_options(
        &db_config,
        &operation_config,
        &eth,
        &pool,
        MetadataCalculatorModeConfig::Lightweight,
    )
    .await;
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.throttler.delay_notifier = delay_sx;
    reset_db_state(&pool, 5).await;

    let start = Instant::now();
    run_calculator(calculator, pool, prover_pool).await;
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(100), "{:?}", elapsed);

    // Throttling should be enabled only once, when we have no more blocks to process
    let (block_count, _) = delay_rx.try_recv().unwrap();
    assert_eq!(block_count, 6);
    delay_rx.try_recv().unwrap_err();
}

#[db_test]
async fn shutting_down_calculator(pool: ConnectionPool, prover_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (mut db_config, mut operation_config, eth) = create_config(temp_dir.path());
    operation_config.delay_interval = 30_000; // ms; chosen to be larger than `RUN_TIMEOUT`
    db_config.new_merkle_tree_throttle_ms = 30_000;

    let calculator = setup_calculator_with_options(
        &db_config,
        &operation_config,
        &eth,
        &pool,
        MetadataCalculatorModeConfig::Lightweight,
    )
    .await;

    reset_db_state(&pool, 5).await;

    let (stop_sx, stop_rx) = watch::channel(false);
    let calculator_task = tokio::spawn(calculator.run(pool, prover_pool, stop_rx));
    tokio::time::sleep(Duration::from_millis(100)).await;
    stop_sx.send_replace(true);
    run_with_timeout(RUN_TIMEOUT, calculator_task)
        .await
        .unwrap();
}

async fn setup_calculator(
    db_path: &Path,
    pool: &ConnectionPool,
) -> (MetadataCalculator, Box<dyn ObjectStore>) {
    let store_factory = &ObjectStoreFactory::mock();
    let (db_config, operation_manager, eth) = create_config(db_path);
    let mode = MetadataCalculatorModeConfig::Full { store_factory };
    let calculator =
        setup_calculator_with_options(&db_config, &operation_manager, &eth, pool, mode).await;
    (calculator, store_factory.create_store().await)
}

async fn setup_lightweight_calculator(db_path: &Path, pool: &ConnectionPool) -> MetadataCalculator {
    let mode = MetadataCalculatorModeConfig::Lightweight;
    let (db_config, operation_config, eth) = create_config(db_path);
    setup_calculator_with_options(&db_config, &operation_config, &eth, pool, mode).await
}

fn create_config(db_path: &Path) -> (DBConfig, OperationsManagerConfig, NetworkConfig) {
    let mut db_config = DBConfig::from_env();
    let mut operation_config = OperationsManagerConfig::from_env();
    let eth_config = NetworkConfig::from_env();
    operation_config.delay_interval = 50; // ms
    db_config.new_merkle_tree_ssd_path = path_to_string(&db_path.join("new"));
    db_config.backup_interval_ms = 0;
    (db_config, operation_config, eth_config)
}

async fn setup_calculator_with_options(
    db_config: &DBConfig,
    operation_config: &OperationsManagerConfig,
    eth: &NetworkConfig,
    pool: &ConnectionPool,
    mode: MetadataCalculatorModeConfig<'_>,
) -> MetadataCalculator {
    let calculator_config =
        MetadataCalculatorConfig::for_main_node(db_config, operation_config, mode);
    let metadata_calculator = MetadataCalculator::new(&calculator_config).await;

    let mut storage = pool.access_storage_tagged("metadata_calculator").await;
    if storage.blocks_dal().is_genesis_needed().await {
        let chain_id = L2ChainId(eth.zksync_network_id);
        let base_system_contracts = BaseSystemContracts::load_from_disk();
        let block_commitment = BlockCommitment::new(
            vec![],
            0,
            Default::default(),
            vec![],
            vec![],
            base_system_contracts.bootloader.hash,
            base_system_contracts.default_aa.hash,
        );

        let fee_address = Address::repeat_byte(0x01);
        create_genesis_block(&mut storage, fee_address, chain_id, base_system_contracts).await;
        save_genesis_block_metadata(
            &mut storage,
            &block_commitment,
            metadata_calculator.updater.tree().root_hash(),
            1,
        )
        .await;
    }
    metadata_calculator
}

fn path_to_string(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

async fn run_calculator(
    mut calculator: MetadataCalculator,
    pool: ConnectionPool,
    prover_pool: ConnectionPool,
) -> H256 {
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, mut delay_rx) = mpsc::unbounded_channel();
    calculator.delayer.delay_notifier = delay_sx;
    let delayer_handle = tokio::spawn(async move {
        // Wait until the calculator has processed all initially available blocks,
        // then stop it via signal.
        let (_, root_hash) = delay_rx
            .recv()
            .await
            .expect("metadata calculator shut down prematurely");
        stop_sx.send(true).unwrap();
        root_hash
    });

    run_with_timeout(RUN_TIMEOUT, calculator.run(pool, prover_pool, stop_rx)).await;
    delayer_handle.await.unwrap()
}

async fn reset_db_state(pool: &ConnectionPool, num_blocks: usize) {
    let mut storage = pool.access_storage_tagged("metadata_calculator").await;
    // Drops all blocks (except the block with number = 0) and their storage logs.
    storage
        .storage_logs_dal()
        .rollback_storage_logs(MiniblockNumber(0))
        .await;
    storage
        .blocks_dal()
        .delete_miniblocks(MiniblockNumber(0))
        .await;
    storage
        .blocks_dal()
        .delete_l1_batches(L1BatchNumber(0))
        .await;

    let logs = gen_storage_logs(0..100, num_blocks);
    extend_db_state(&mut storage, logs).await;
}

async fn extend_db_state(
    storage: &mut StorageProcessor<'_>,
    new_logs: impl IntoIterator<Item = Vec<StorageLog>>,
) {
    let next_block = storage.blocks_dal().get_sealed_block_number().await.0 + 1;

    let base_system_contracts = BaseSystemContracts::load_from_disk();
    for (idx, block_logs) in (next_block..).zip(new_logs) {
        let block_number = L1BatchNumber(idx);
        let mut header = L1BatchHeader::new(
            block_number,
            0,
            Address::default(),
            base_system_contracts.hashes(),
        );
        header.is_finished = true;

        // Assumes that L1 batch consists of only one miniblock.
        let miniblock_number = MiniblockNumber(idx);
        let miniblock_header = MiniblockHeader {
            number: miniblock_number,
            timestamp: header.timestamp,
            hash: miniblock_hash(miniblock_number),
            l1_tx_count: header.l1_tx_count,
            l2_tx_count: header.l2_tx_count,
            base_fee_per_gas: header.base_fee_per_gas,
            l1_gas_price: 0,
            l2_fair_gas_price: 0,
            base_system_contracts_hashes: base_system_contracts.hashes(),
        };

        storage
            .blocks_dal()
            .insert_l1_batch(&header, BlockGasCount::default())
            .await;
        storage
            .blocks_dal()
            .insert_miniblock(&miniblock_header)
            .await;
        storage
            .storage_logs_dal()
            .insert_storage_logs(miniblock_number, &[(H256::zero(), block_logs)])
            .await;
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(block_number)
            .await;
    }
}

fn gen_storage_logs(indices: ops::Range<u32>, num_blocks: usize) -> Vec<Vec<StorageLog>> {
    // Addresses and keys of storage logs must be sorted for the `multi_block_workflow` test.
    let mut accounts = [
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .map(|s| AccountTreeId::new(s.parse::<Address>().unwrap()));
    accounts.sort_unstable();

    let account_keys = (indices.start / 5)..(indices.end / 5);
    let proof_keys = accounts.iter().flat_map(|&account| {
        account_keys
            .clone()
            .map(move |i| StorageKey::new(account, u32_to_h256(i)))
    });
    let proof_values = indices.map(u32_to_h256);

    let logs: Vec<_> = proof_keys
        .zip(proof_values)
        .map(|(proof_key, proof_value)| StorageLog::new_write_log(proof_key, proof_value))
        .collect();
    for window in logs.windows(2) {
        let [prev, next] = window else { unreachable!() };
        assert!(prev.key < next.key);
    }

    logs.chunks(logs.len() / num_blocks)
        .map(<[_]>::to_vec)
        .collect()
}
