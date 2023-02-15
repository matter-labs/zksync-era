use itertools::Itertools;
use std::path::Path;
use std::str::FromStr;

use db_test_macro::db_test;
use tempfile::TempDir;
use tokio::sync::watch;
use zksync_types::FAIR_L2_GAS_PRICE;

use crate::genesis::{chain_schema_genesis, operations_schema_genesis};
use crate::metadata_calculator::MetadataCalculator;

use crate::MetadataCalculatorMode;
use zksync_config::ZkSyncConfig;
use zksync_dal::ConnectionPool;
use zksync_merkle_tree::ZkSyncTree;
use zksync_storage::db::Database;
use zksync_storage::RocksDB;
use zksync_types::{
    block::{L1BatchHeader, MiniblockHeader},
    commitment::BlockCommitment,
    AccountTreeId, Address, L1BatchNumber, MiniblockNumber, StorageKey, StorageLog, H256,
};
use zksync_utils::{miniblock_hash, u32_to_h256};

#[db_test]
async fn genesis_creation(connection_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    {
        let metadata_calculator =
            setup_metadata_calculator(temp_dir.path(), connection_pool.clone()).await;
        metadata_calculator
            .run(connection_pool.clone(), watch::channel(false).1)
            .await;
    }

    let mut metadata_calculator = setup_metadata_calculator(temp_dir.path(), connection_pool).await;
    assert_eq!(
        metadata_calculator.get_current_rocksdb_block_number(),
        L1BatchNumber(1)
    );
}

#[ignore]
#[db_test]
async fn backup_recovery(connection_pool: ConnectionPool) {
    let backup_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let backup_path = backup_dir.path().to_str().unwrap().to_string();

    {
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let metadata_calculator = setup_metadata_calculator_with_options(
            temp_dir.path(),
            connection_pool.clone(),
            MetadataCalculatorMode::Backup,
            Some(backup_path.clone()),
        )
        .await;
        reset_db_state(connection_pool.clone(), 1).await;
        metadata_calculator
            .run(connection_pool.clone(), watch::channel(false).1)
            .await;
    }

    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let mut metadata_calculator = setup_metadata_calculator_with_options(
        temp_dir.path(),
        connection_pool,
        MetadataCalculatorMode::Full,
        Some(backup_path),
    )
    .await;
    assert_eq!(
        metadata_calculator.get_current_rocksdb_block_number(),
        L1BatchNumber(2)
    );
}

#[db_test]
async fn basic_workflow(connection_pool: ConnectionPool) {
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    {
        let metadata_calculator =
            setup_metadata_calculator(temp_dir.path(), connection_pool.clone()).await;
        reset_db_state(connection_pool.clone(), 1).await;
        metadata_calculator
            .run(connection_pool.clone(), watch::channel(false).1)
            .await;
    }

    let mut metadata_calculator = setup_metadata_calculator(temp_dir.path(), connection_pool).await;
    assert_eq!(
        metadata_calculator.get_current_rocksdb_block_number(),
        L1BatchNumber(2)
    );
}

#[db_test]
async fn multi_block_workflow(connection_pool: ConnectionPool) {
    // run all transactions as single block
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");

    {
        let metadata_calculator =
            setup_metadata_calculator(temp_dir.path(), connection_pool.clone()).await;
        reset_db_state(connection_pool.clone(), 1).await;
        metadata_calculator
            .run(connection_pool.clone(), watch::channel(false).1)
            .await;
    }

    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let root_hash = {
        let tree = ZkSyncTree::new(db);
        tree.root_hash()
    };

    // run same transactions as multiple blocks
    let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    {
        let metadata_calculator =
            setup_metadata_calculator(temp_dir.path(), connection_pool.clone()).await;
        reset_db_state(connection_pool.clone(), 10).await;
        metadata_calculator
            .run(connection_pool.clone(), watch::channel(false).1)
            .await;
    }
    let db = RocksDB::new(Database::MerkleTree, temp_dir.as_ref(), false);
    let tree = ZkSyncTree::new(db);
    let multi_block_root_hash = tree.root_hash();

    // verify that hashes match
    assert_eq!(multi_block_root_hash, root_hash);
}

async fn setup_metadata_calculator(db_path: &Path, pool: ConnectionPool) -> MetadataCalculator {
    setup_metadata_calculator_with_options(db_path, pool, MetadataCalculatorMode::Full, None).await
}

async fn setup_metadata_calculator_with_options(
    db_path: &Path,
    pool: ConnectionPool,
    mode: MetadataCalculatorMode,
    backup_directory: Option<String>,
) -> MetadataCalculator {
    let backup_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
    let mut config = ZkSyncConfig::from_env().clone();
    config.db.path = db_path.to_str().unwrap().to_string();
    config.db.merkle_tree_fast_ssd_path = config.db.path.clone();
    config.db.merkle_tree_backup_path =
        backup_directory.unwrap_or_else(|| backup_dir.path().to_str().unwrap().to_string());
    config.db.backup_interval_ms = 0;
    let fee_address = Address::repeat_byte(0x01);
    let mut storage = pool.access_storage().await;
    let metadata_calculator = MetadataCalculator::new(&config, mode);

    if storage.blocks_dal().is_genesis_needed() {
        let chain_id = H256::from_low_u64_be(config.chain.eth.zksync_network_id as u64);
        chain_schema_genesis(&mut storage, fee_address, chain_id).await;
        let block_commitment = BlockCommitment::new(vec![], 0, Default::default(), vec![], vec![]);

        operations_schema_genesis(
            &mut storage,
            &block_commitment,
            H256::from_slice(&metadata_calculator.tree.root_hash()),
            1,
        );
    }
    metadata_calculator
}

async fn reset_db_state(pool: ConnectionPool, num_blocks: usize) {
    let mut storage = pool.access_storage().await;
    // Drops all blocks (except the block with number = 0) and theirs storage logs.
    storage
        .storage_logs_dal()
        .rollback_storage_logs(MiniblockNumber(0));
    storage.blocks_dal().delete_miniblocks(MiniblockNumber(0));
    storage.blocks_dal().delete_l1_batches(L1BatchNumber(0));

    let all_logs = gen_storage_logs(num_blocks);
    for (block_number, block_logs) in (1..=(num_blocks as u32)).zip(all_logs) {
        let mut header = L1BatchHeader::mock(L1BatchNumber(block_number));
        header.is_finished = true;
        // Assumes that L1 batch consists of only one miniblock.
        let miniblock_header = MiniblockHeader {
            number: MiniblockNumber(block_number),
            timestamp: header.timestamp,
            hash: miniblock_hash(MiniblockNumber(block_number)),
            l1_tx_count: header.l1_tx_count,
            l2_tx_count: header.l2_tx_count,
            base_fee_per_gas: header.base_fee_per_gas,
            l1_gas_price: 0,
            l2_fair_gas_price: FAIR_L2_GAS_PRICE,
        };

        storage
            .blocks_dal()
            .insert_l1_batch(header.clone(), Default::default());
        storage.blocks_dal().insert_miniblock(miniblock_header);
        storage.storage_logs_dal().insert_storage_logs(
            MiniblockNumber(block_number),
            &[(H256::default(), block_logs)],
        );
        storage
            .blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(header.number);
    }
}

fn gen_storage_logs(num_blocks: usize) -> Vec<Vec<StorageLog>> {
    // Note, addresses and keys of storage logs must be sorted for the multi_block_workflow test.
    let addrs = vec![
        "4b3af74f66ab1f0da3f2e4ec7a3cb99baf1af7b2",
        "ef4bb7b21c5fe7432a7d63876cc59ecc23b46636",
        "89b8988a018f5348f52eeac77155a793adf03ecc",
        "782806db027c08d36b2bed376b4271d1237626b3",
        "b2b57b76717ee02ae1327cc3cf1f40e76f692311",
    ]
    .into_iter()
    .map(|s| Address::from_str(s).unwrap())
    .sorted();

    let proof_keys: Vec<_> = addrs
        .flat_map(|addr| {
            (0..20).map(move |i| StorageKey::new(AccountTreeId::new(addr), u32_to_h256(i)))
        })
        .collect();
    let proof_values: Vec<_> = (0..100).map(u32_to_h256).collect();

    let logs = proof_keys
        .iter()
        .zip(proof_values.iter())
        .map(|(proof_key, &proof_value)| StorageLog::new_write_log(*proof_key, proof_value))
        .collect::<Vec<_>>();
    logs.chunks(logs.len() / num_blocks)
        .map(|v| v.into())
        .collect()
}
