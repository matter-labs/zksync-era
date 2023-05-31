use assert_matches::assert_matches;
use db_test_macro::db_test;
use tempfile::TempDir;
use tokio::sync::watch;

use std::{
    ops, panic,
    path::Path,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crate::genesis::{create_genesis_block, save_genesis_block_metadata};
use crate::metadata_calculator::{MetadataCalculator, MetadataCalculatorMode, TreeImplementation};
use zksync_config::ZkSyncConfig;
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

const RUN_TIMEOUT: Duration = Duration::from_secs(5);

fn run_with_timeout<T, F>(timeout: Duration, action: F) -> T
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (termination_sx, termination_rx) = mpsc::channel();
    let join_handle = thread::spawn(move || {
        termination_sx.send(action()).ok();
    });
    let output = termination_rx
        .recv_timeout(timeout)
        .expect("timed out waiting for metadata calculator");
    match join_handle.join() {
        Ok(()) => output,
        Err(panic_object) => panic::resume_unwind(panic_object),
    }
}

fn test_genesis_creation(pool: &ConnectionPool, implementation: TreeImplementation) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, _) = setup_calculator(temp_dir.path(), pool, implementation);
    assert!(calculator.tree_tag().starts_with("full"));
    run_calculator(calculator, pool.clone());
    let (calculator, _) = setup_calculator(temp_dir.path(), pool, implementation);
    assert_eq!(calculator.updater.tree().block_number(), 1);
}

#[db_test]
async fn genesis_creation(pool: ConnectionPool) {
    test_genesis_creation(&pool, TreeImplementation::Old);
}

#[db_test]
async fn genesis_creation_with_new_tree(pool: ConnectionPool) {
    test_genesis_creation(&pool, TreeImplementation::New);
}

fn test_basic_workflow(
    pool: &ConnectionPool,
    implementation: TreeImplementation,
) -> PrepareBasicCircuitsJob {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, object_store) = setup_calculator(temp_dir.path(), pool, implementation);
    reset_db_state(pool, 1);
    run_calculator(calculator, pool.clone());

    let job: PrepareBasicCircuitsJob = object_store.get(L1BatchNumber(1)).unwrap();
    assert!(job.next_enumeration_index() > 0);
    let merkle_paths: Vec<_> = job.clone().into_merkle_paths().collect();
    assert!(!merkle_paths.is_empty() && merkle_paths.len() <= 100);
    // ^ The exact values depend on ops in genesis block
    assert!(merkle_paths.iter().all(|log| log.is_write));

    let (calculator, _) = setup_calculator(temp_dir.path(), pool, implementation);
    assert_eq!(calculator.updater.tree().block_number(), 2);
    job
}

#[db_test]
async fn basic_workflow(pool: ConnectionPool) {
    let old_job = test_basic_workflow(&pool, TreeImplementation::Old);
    let new_job = test_basic_workflow(&pool, TreeImplementation::New);
    assert_jobs_eq(old_job, new_job);
}

fn assert_jobs_eq(old_job: PrepareBasicCircuitsJob, new_job: PrepareBasicCircuitsJob) {
    assert_eq!(
        old_job.next_enumeration_index(),
        new_job.next_enumeration_index()
    );
    let old_merkle_paths = old_job.into_merkle_paths();
    let new_merkle_paths = new_job.into_merkle_paths();
    assert_eq!(old_merkle_paths.len(), new_merkle_paths.len());
    for (old_path, new_path) in old_merkle_paths.zip(new_merkle_paths) {
        assert_eq!(old_path, new_path);
    }
}

#[db_test]
async fn status_receiver_has_correct_states(pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let (calculator, _) = setup_calculator(temp_dir.path(), &pool, TreeImplementation::Old);
    let tree_health_check = calculator.tree_health_check();
    assert_matches!(
        tree_health_check.check_health(),
        CheckHealthStatus::NotReady(msg) if msg.contains("full")
    );
    let other_tree_health_check = calculator.tree_health_check();
    assert_matches!(
        other_tree_health_check.check_health(),
        CheckHealthStatus::NotReady(msg) if msg.contains("full")
    );
    reset_db_state(&pool, 1);
    run_calculator(calculator, pool);
    assert_eq!(tree_health_check.check_health(), CheckHealthStatus::Ready);
    assert_eq!(
        other_tree_health_check.check_health(),
        CheckHealthStatus::Ready
    );
}

fn test_multi_block_workflow(
    pool: ConnectionPool,
    implementation: TreeImplementation,
) -> Box<dyn ObjectStore> {
    // Run all transactions as a single block
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, _) = setup_calculator(temp_dir.path(), &pool, implementation);
    reset_db_state(&pool, 1);
    let root_hash = run_calculator(calculator, pool.clone());

    // Run the same transactions as multiple blocks
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let (calculator, object_store) = setup_calculator(temp_dir.path(), &pool, implementation);
    reset_db_state(&pool, 10);
    let multi_block_root_hash = run_calculator(calculator, pool);
    assert_eq!(multi_block_root_hash, root_hash);

    let mut prev_index = None;
    for block_number in 1..=10 {
        let block_number = L1BatchNumber(block_number);
        let job: PrepareBasicCircuitsJob = object_store.get(block_number).unwrap();
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
    object_store
}

#[db_test]
async fn multi_block_workflow(pool: ConnectionPool) {
    let old_store = test_multi_block_workflow(pool.clone(), TreeImplementation::Old);
    let new_store = test_multi_block_workflow(pool, TreeImplementation::New);

    for block_number in 1..=10 {
        let old_job: PrepareBasicCircuitsJob = old_store.get(L1BatchNumber(block_number)).unwrap();
        let new_job: PrepareBasicCircuitsJob = new_store.get(L1BatchNumber(block_number)).unwrap();
        assert_jobs_eq(old_job, new_job);
    }
}

fn test_switch_from_old_to_new_tree_without_catchup(pool: ConnectionPool, block_count: usize) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let calculator = setup_lightweight_calculator(temp_dir.path(), &pool, TreeImplementation::Old);
    assert!(calculator.tree_tag().starts_with("lightweight"));
    reset_db_state(&pool, block_count);
    let old_root_hash = run_calculator(calculator, pool.clone());

    let calculator = setup_lightweight_calculator(temp_dir.path(), &pool, TreeImplementation::New);
    let new_root_hash = run_calculator(calculator, pool);
    assert_eq!(new_root_hash, old_root_hash);
}

#[db_test]
async fn switching_from_old_to_new_tree_without_catchup(pool: ConnectionPool) {
    test_switch_from_old_to_new_tree_without_catchup(pool, 1);
}

#[db_test]
async fn switching_from_old_to_new_tree_in_multiple_blocks_without_catchup(pool: ConnectionPool) {
    test_switch_from_old_to_new_tree_without_catchup(pool, 10);
}

#[db_test]
async fn switching_between_tree_impls_with_additional_blocks(pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    let calculator = setup_lightweight_calculator(temp_dir.path(), &pool, TreeImplementation::Old);
    reset_db_state(&pool, 5);
    run_calculator(calculator, pool.clone());

    let mut calculator =
        setup_lightweight_calculator(temp_dir.path(), &pool, TreeImplementation::New);
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, delay_rx) = mpsc::channel();
    calculator.delayer.delay_notifier = delay_sx;

    let calculator_handle = {
        let pool = pool.clone();
        thread::spawn(move || calculator.run(&pool, stop_rx))
    };
    // Wait until the calculator has processed initial blocks.
    let (block_count, _) = delay_rx
        .recv_timeout(RUN_TIMEOUT)
        .expect("metadata calculator timed out processing initial blocks");
    assert_eq!(block_count, 6);

    // Add some new blocks to the storage.
    let new_logs = gen_storage_logs(100..200, 10);
    extend_db_state(&mut pool.access_storage_blocking(), new_logs);

    // Wait until these blocks are processed. The calculator may have spurious delays,
    // thus we wait in a loop.
    let updated_root_hash = loop {
        let (block_count, root_hash) = delay_rx
            .recv_timeout(RUN_TIMEOUT)
            .expect("metadata calculator shut down prematurely");
        if block_count == 16 {
            stop_sx.send(true).unwrap(); // Shut down the calculator.
            break root_hash;
        }
    };
    run_with_timeout(RUN_TIMEOUT, || calculator_handle.join()).unwrap();

    // Switch back to the old implementation. It should process new blocks independently
    // and result in the same tree root hash.
    let calculator = setup_lightweight_calculator(temp_dir.path(), &pool, TreeImplementation::Old);
    let root_hash_for_old_tree = run_calculator(calculator, pool);
    assert_eq!(root_hash_for_old_tree, updated_root_hash);
}

#[db_test]
async fn throttling_new_tree(pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let mut config = create_config(temp_dir.path());
    config.db.new_merkle_tree_throttle_ms = 100;
    let mut calculator = setup_calculator_with_options(
        &config,
        &pool,
        &ObjectStoreFactory::mock(),
        MetadataCalculatorMode::Lightweight(TreeImplementation::New),
    );
    let (delay_sx, delay_rx) = mpsc::channel();
    calculator.throttler.delay_notifier = delay_sx;
    reset_db_state(&pool, 5);

    let start = Instant::now();
    run_calculator(calculator, pool);
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(100), "{:?}", elapsed);

    // Throttling should be enabled only once, when we have no more blocks to process
    let (block_count, _) = delay_rx.try_recv().unwrap();
    assert_eq!(block_count, 6);
    delay_rx.try_recv().unwrap_err();
}

fn setup_calculator(
    db_path: &Path,
    pool: &ConnectionPool,
    implementation: TreeImplementation,
) -> (MetadataCalculator, Box<dyn ObjectStore>) {
    let store_factory = ObjectStoreFactory::mock();
    let config = create_config(db_path);
    let mode = MetadataCalculatorMode::Full(implementation);
    let calculator = setup_calculator_with_options(&config, pool, &store_factory, mode);
    (calculator, store_factory.create_store())
}

fn setup_lightweight_calculator(
    db_path: &Path,
    pool: &ConnectionPool,
    implementation: TreeImplementation,
) -> MetadataCalculator {
    let mode = MetadataCalculatorMode::Lightweight(implementation);
    let config = create_config(db_path);
    setup_calculator_with_options(&config, pool, &ObjectStoreFactory::mock(), mode)
}

fn create_config(db_path: &Path) -> ZkSyncConfig {
    let mut config = ZkSyncConfig::from_env();
    config.chain.operations_manager.delay_interval = 50; // ms
    config.db.path = path_to_string(db_path);
    config.db.merkle_tree_fast_ssd_path = path_to_string(&db_path.join("old"));
    config.db.new_merkle_tree_ssd_path = path_to_string(&db_path.join("new"));
    config.db.backup_interval_ms = 0;
    config
}

fn setup_calculator_with_options(
    config: &ZkSyncConfig,
    pool: &ConnectionPool,
    store_factory: &ObjectStoreFactory,
    mode: MetadataCalculatorMode,
) -> MetadataCalculator {
    let store_factory = matches!(mode, MetadataCalculatorMode::Full(_)).then_some(store_factory);
    let metadata_calculator = MetadataCalculator::new(config, store_factory, mode);

    let mut storage = pool.access_storage_blocking();
    if storage.blocks_dal().is_genesis_needed() {
        let chain_id = L2ChainId(config.chain.eth.zksync_network_id);
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
        create_genesis_block(&mut storage, fee_address, chain_id, base_system_contracts);
        save_genesis_block_metadata(
            &mut storage,
            &block_commitment,
            metadata_calculator.updater.tree().root_hash(),
            1,
        );
    }
    metadata_calculator
}

fn path_to_string(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn run_calculator(mut calculator: MetadataCalculator, pool: ConnectionPool) -> H256 {
    let (stop_sx, stop_rx) = watch::channel(false);
    let (delay_sx, delay_rx) = mpsc::channel();
    calculator.delayer.delay_notifier = delay_sx;
    let delayer_handle = thread::spawn(move || {
        // Wait until the calculator has processed all initially available blocks,
        // then stop it via signal.
        let (_, root_hash) = delay_rx
            .recv()
            .expect("metadata calculator shut down prematurely");
        stop_sx.send(true).unwrap();
        root_hash
    });

    run_with_timeout(RUN_TIMEOUT, move || calculator.run(&pool, stop_rx));
    delayer_handle.join().unwrap()
}

fn reset_db_state(pool: &ConnectionPool, num_blocks: usize) {
    let mut storage = pool.access_storage_blocking();
    // Drops all blocks (except the block with number = 0) and their storage logs.
    storage
        .storage_logs_dal()
        .rollback_storage_logs(MiniblockNumber(0));
    storage.blocks_dal().delete_miniblocks(MiniblockNumber(0));
    storage.blocks_dal().delete_l1_batches(L1BatchNumber(0));

    let logs = gen_storage_logs(0..100, num_blocks);
    extend_db_state(&mut storage, logs);
}

fn extend_db_state(
    storage: &mut StorageProcessor<'_>,
    new_logs: impl IntoIterator<Item = Vec<StorageLog>>,
) {
    let next_block = storage.blocks_dal().get_sealed_block_number().0 + 1;

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
            .insert_l1_batch(header, BlockGasCount::default());
        storage.blocks_dal().insert_miniblock(miniblock_header);
        storage
            .storage_logs_dal()
            .insert_storage_logs(miniblock_number, &[(H256::zero(), block_logs)]);
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(block_number);
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
